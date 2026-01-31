#![allow(dead_code)]
#![allow(unused_imports)]

mod audio;
mod keybinds;
mod scaler;
mod ui;
mod ui_config;

use clap::{Parser, ValueEnum};
use cpal::traits::StreamTrait;
use eframe::egui;
use log::{debug, error, info, warn};
use rfd::FileDialog;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex, RwLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use vibe_emu_core::serial::{LinkPort, NullLinkPort};
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::CgbRevision, mmu::Mmu};
use vibe_emu_mobile::{
    MobileAdapter, MobileAdapterDevice, MobileAddr, MobileConfig, MobileHost, MobileLinkPort,
    MobileNumber, MobileSockType, StdMobileHost,
};

use crossbeam_channel as cb;
use keybinds::KeyBindings;
pub use scaler::GameScaler;
use ui::snapshot::UiSnapshot;
use ui_config::{EmulationMode, UiConfig, WindowSize};

const DEFAULT_WINDOW_SCALE: u32 = 2;
const GB_FPS: f64 = 59.7275;
const FRAME_TIME: Duration = Duration::from_nanos((1e9_f64 / GB_FPS) as u64);
const FF_MULT: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum LogLevelArg {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevelArg {
    fn as_filter_str(self) -> &'static str {
        match self {
            LogLevelArg::Off => "off",
            LogLevelArg::Error => "error",
            LogLevelArg::Warn => "warn",
            LogLevelArg::Info => "info",
            LogLevelArg::Debug => "debug",
            LogLevelArg::Trace => "trace",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum MobileDeviceArg {
    Blue,
    Yellow,
    Green,
    Red,
}

impl From<MobileDeviceArg> for MobileAdapterDevice {
    fn from(value: MobileDeviceArg) -> Self {
        match value {
            MobileDeviceArg::Blue => MobileAdapterDevice::Blue,
            MobileDeviceArg::Yellow => MobileAdapterDevice::Yellow,
            MobileDeviceArg::Green => MobileAdapterDevice::Green,
            MobileDeviceArg::Red => MobileAdapterDevice::Red,
        }
    }
}

#[derive(Parser)]
struct Args {
    rom: Option<std::path::PathBuf>,

    #[arg(long, conflicts_with = "cgb")]
    dmg: bool,

    #[arg(long)]
    dmg_neutral: bool,

    #[arg(long, conflicts_with = "dmg")]
    cgb: bool,

    #[arg(long)]
    bootrom: Option<std::path::PathBuf>,

    #[arg(long)]
    debug: bool,

    #[arg(long, value_enum)]
    log_level: Option<LogLevelArg>,

    #[arg(long)]
    headless: bool,

    #[arg(long)]
    frames: Option<usize>,

    #[arg(long)]
    seconds: Option<u64>,

    #[arg(long)]
    cycles: Option<u64>,

    #[arg(long)]
    mobile: bool,

    #[arg(long)]
    mobile_config: Option<std::path::PathBuf>,

    #[arg(long, value_enum, default_value_t = MobileDeviceArg::Blue)]
    mobile_device: MobileDeviceArg,

    #[arg(long)]
    mobile_unmetered: bool,

    #[arg(long)]
    mobile_dns1: Option<String>,

    #[arg(long)]
    mobile_dns2: Option<String>,

    #[arg(long)]
    mobile_relay: Option<String>,

    #[arg(long)]
    mobile_p2p_port: Option<u16>,

    #[arg(long)]
    mobile_diag: bool,

    #[arg(long)]
    keybinds: Option<std::path::PathBuf>,
}

fn init_logging(args: &Args) {
    let default_filter = if let Some(level) = args.log_level {
        level.as_filter_str()
    } else if cfg!(debug_assertions) {
        LogLevelArg::Info.as_filter_str()
    } else {
        LogLevelArg::Off.as_filter_str()
    };

    let mut logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_filter));
    logger.filter_module("wgpu", log::LevelFilter::Warn);
    logger.filter_module("wgpu_core", log::LevelFilter::Warn);
    logger.filter_module("wgpu_hal", log::LevelFilter::Warn);
    logger.filter_module("naga", log::LevelFilter::Warn);
    logger.format_timestamp_millis().init();

    struct CoreLogForwarder;

    impl vibe_emu_core::diagnostics::LogSink for CoreLogForwarder {
        fn log(
            &self,
            level: vibe_emu_core::diagnostics::Level,
            target: &'static str,
            args: std::fmt::Arguments,
        ) {
            match level {
                vibe_emu_core::diagnostics::Level::Trace => {
                    log::trace!(target: target, "{}", args);
                }
                vibe_emu_core::diagnostics::Level::Info => {
                    log::info!(target: target, "{}", args);
                }
                vibe_emu_core::diagnostics::Level::Warn => {
                    log::warn!(target: target, "{}", args);
                }
            }
        }
    }

    let _ = vibe_emu_core::diagnostics::try_set_log_sink(Box::new(CoreLogForwarder));
}

fn load_window_icon() -> Option<egui::IconData> {
    let icon_data = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../gfx/vibeEmu_512px.png"
    ));
    let cursor = Cursor::new(&icon_data[..]);
    let mut decoder = png::Decoder::new(cursor);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let buffer_size = reader.output_buffer_size()?;
    let mut buf = vec![0; buffer_size];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let pixel_count = info.width as usize * info.height as usize;
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    match reader.info().color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(data),
        png::ColorType::Rgb => {
            for chunk in data.chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 0xFF]);
            }
        }
        png::ColorType::Grayscale => {
            for &g in data {
                rgba.extend_from_slice(&[g, g, g, 0xFF]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for chunk in data.chunks_exact(2) {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
        }
        _ => return None,
    }
    Some(egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum SerialPeripheral {
    #[default]
    None,
    MobileAdapter,
}

#[derive(Clone)]
struct LoadConfig {
    emulation_mode: EmulationMode,
    dmg_neutral: bool,
    bootrom_override: Option<Vec<u8>>,
    dmg_bootrom_path: Option<std::path::PathBuf>,
    cgb_bootrom_path: Option<std::path::PathBuf>,
}

#[derive(Clone, Copy)]
struct Speed {
    factor: f32,
    fast: bool,
}

enum EmuCommand {
    SetPaused(bool),
    SetSpeed(Speed),
    UpdateInput(u8),
    Shutdown,
}

enum EmuEvent {
    Frame { frame: Vec<u32>, frame_index: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RebindTarget {
    Joypad(u8),
    Pause,
    FastForward,
    Quit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OptionsTab {
    #[default]
    Keybinds,
    Emulation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DebuggerTab {
    #[default]
    Registers,
    Disassembly,
    Memory,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum VramTab {
    #[default]
    BgMap,
    Tiles,
    Oam,
    Palettes,
}

struct EmuThreadChannels {
    rx: mpsc::Receiver<EmuCommand>,
    frame_tx: cb::Sender<EmuEvent>,
    frame_pool_tx: cb::Sender<Vec<u32>>,
    frame_pool_rx: cb::Receiver<Vec<u32>>,
}

fn run_emulator_thread(
    gb: Arc<Mutex<GameBoy>>,
    mut speed: Speed,
    initial_paused: bool,
    channels: EmuThreadChannels,
) {
    let EmuThreadChannels {
        rx,
        frame_tx,
        frame_pool_tx: _,
        frame_pool_rx,
    } = channels;

    let mut paused = initial_paused;
    let mut frame_count = 0u64;
    let mut next_frame = Instant::now() + FRAME_TIME;

    loop {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                EmuCommand::SetPaused(p) => {
                    paused = p;
                    next_frame = Instant::now() + FRAME_TIME;
                }
                EmuCommand::SetSpeed(s) => {
                    speed = s;
                }
                EmuCommand::UpdateInput(input) => {
                    if let Ok(mut gb) = gb.lock() {
                        gb.mmu.input.set_state(input);
                    }
                }
                EmuCommand::Shutdown => {
                    return;
                }
            }
        }

        if paused {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }

        let now = Instant::now();
        if now < next_frame {
            std::thread::sleep(next_frame - now);
        }

        let frame_duration = Duration::from_secs_f64(1.0 / (GB_FPS * speed.factor as f64));
        next_frame = Instant::now() + frame_duration;

        let mut frame_buf = frame_pool_rx
            .try_recv()
            .unwrap_or_else(|_| vec![0u32; 160 * 144]);

        if let Ok(mut gb) = gb.lock() {
            let GameBoy { cpu, mmu, .. } = &mut *gb;
            mmu.ppu.clear_frame_flag();
            while !mmu.ppu.frame_ready() {
                cpu.step(mmu);
            }
            frame_buf.copy_from_slice(mmu.ppu.framebuffer());
        }

        frame_count += 1;
        let _ = frame_tx.try_send(EmuEvent::Frame {
            frame: frame_buf,
            frame_index: frame_count,
        });
    }
}

struct VibeEmuApp {
    gb: Arc<Mutex<GameBoy>>,
    emu_tx: mpsc::Sender<EmuCommand>,
    frame_rx: cb::Receiver<EmuEvent>,
    frame_pool_tx: cb::Sender<Vec<u32>>,

    framebuffer: Vec<u32>,
    texture: Option<egui::TextureHandle>,
    paused: bool,
    current_rom_path: Option<std::path::PathBuf>,
    keybinds: KeyBindings,
    joypad_state: u8,
    fast_forward: bool,

    show_debugger: bool,
    show_vram_viewer: bool,
    show_options: bool,

    // Options window state
    dmg_bootrom_path: String,
    cgb_bootrom_path: String,
    selected_window_scale: usize,
    rebinding: Option<RebindTarget>,
    options_tab: OptionsTab,

    // Debugger state
    debugger_snapshot: Option<UiSnapshot>,
    debugger_tab: DebuggerTab,

    // VRAM Viewer state
    vram_tab: VramTab,
}

impl VibeEmuApp {
    fn new(
        _cc: &eframe::CreationContext<'_>,
        gb: Arc<Mutex<GameBoy>>,
        emu_tx: mpsc::Sender<EmuCommand>,
        frame_rx: cb::Receiver<EmuEvent>,
        frame_pool_tx: cb::Sender<Vec<u32>>,
        rom_path: Option<std::path::PathBuf>,
        keybinds: KeyBindings,
    ) -> Self {
        let paused = rom_path.is_none();
        if paused {
            let _ = emu_tx.send(EmuCommand::SetPaused(true));
        }

        Self {
            gb,
            emu_tx,
            frame_rx,
            frame_pool_tx,
            framebuffer: vec![0u32; 160 * 144],
            texture: None,
            paused,
            current_rom_path: rom_path,
            keybinds,
            joypad_state: 0xFF,
            fast_forward: false,
            show_debugger: false,
            show_vram_viewer: false,
            show_options: false,
            dmg_bootrom_path: String::new(),
            cgb_bootrom_path: String::new(),
            selected_window_scale: 1, // 2x
            rebinding: None,
            options_tab: OptionsTab::default(),
            debugger_snapshot: None,
            debugger_tab: DebuggerTab::default(),
            vram_tab: VramTab::default(),
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        let mut new_state = 0xFFu8;

        ctx.input(|i| {
            for (action, key) in self.keybinds.iter() {
                if i.key_down(*key) {
                    match action.as_str() {
                        "a" => new_state &= !0x01,
                        "b" => new_state &= !0x02,
                        "select" => new_state &= !0x04,
                        "start" => new_state &= !0x08,
                        "right" => new_state &= !0x10,
                        "left" => new_state &= !0x20,
                        "up" => new_state &= !0x40,
                        "down" => new_state &= !0x80,
                        _ => {}
                    }
                }
            }
        });

        if new_state != self.joypad_state {
            self.joypad_state = new_state;
            let _ = self.emu_tx.send(EmuCommand::UpdateInput(new_state));
        }
    }

    fn poll_frames(&mut self) {
        while let Ok(evt) = self.frame_rx.try_recv() {
            let EmuEvent::Frame {
                mut frame,
                frame_index: _,
            } = evt;
            std::mem::swap(&mut self.framebuffer, &mut frame);
            let _ = self.frame_pool_tx.try_send(frame);
        }
    }

    fn update_texture(&mut self, ctx: &egui::Context) {
        let pixels: Vec<egui::Color32> = self
            .framebuffer
            .iter()
            .map(|&rgba| {
                let r = ((rgba >> 16) & 0xFF) as u8;
                let g = ((rgba >> 8) & 0xFF) as u8;
                let b = (rgba & 0xFF) as u8;
                egui::Color32::from_rgb(r, g, b)
            })
            .collect();

        let image = egui::ColorImage {
            size: [160, 144],
            pixels,
        };

        match &mut self.texture {
            Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.texture =
                    Some(ctx.load_texture("gb_framebuffer", image, egui::TextureOptions::NEAREST));
            }
        }
    }

    fn load_rom(&mut self, path: std::path::PathBuf) {
        match std::fs::read(&path) {
            Ok(rom_data) => {
                let cart = Cartridge::load(rom_data);
                if let Ok(mut gb) = self.gb.lock() {
                    gb.mmu.load_cart(cart);
                    gb.reset();
                }
                self.current_rom_path = Some(path);
                self.paused = false;
                let _ = self.emu_tx.send(EmuCommand::SetPaused(false));
                info!("ROM loaded successfully");
            }
            Err(e) => {
                error!("Failed to read ROM file: {e}");
            }
        }
    }
}

impl eframe::App for VibeEmuApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_input(ctx);
        self.poll_frames();
        self.update_texture(ctx);

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open ROM...").clicked() {
                        if let Some(path) = FileDialog::new()
                            .add_filter("Game Boy ROMs", &["gb", "gbc"])
                            .pick_file()
                        {
                            self.load_rom(path);
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        let _ = self.emu_tx.send(EmuCommand::Shutdown);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Emulation", |ui| {
                    if ui
                        .button(if self.paused { "Resume" } else { "Pause" })
                        .clicked()
                    {
                        self.paused = !self.paused;
                        let _ = self.emu_tx.send(EmuCommand::SetPaused(self.paused));
                        ui.close_menu();
                    }
                    if ui.button("Reset").clicked() {
                        if let Ok(mut gb) = self.gb.lock() {
                            gb.reset();
                        }
                        ui.close_menu();
                    }
                });

                ui.menu_button("Debug", |ui| {
                    if ui.button("Debugger").clicked() {
                        self.show_debugger = !self.show_debugger;
                        ui.close_menu();
                    }
                    if ui.button("VRAM Viewer").clicked() {
                        self.show_vram_viewer = !self.show_vram_viewer;
                        ui.close_menu();
                    }
                });

                ui.menu_button("Options", |ui| {
                    if ui.button("Settings...").clicked() {
                        self.show_options = !self.show_options;
                        ui.close_menu();
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.texture {
                let available = ui.available_size();
                let scale = (available.x / 160.0)
                    .min(available.y / 144.0)
                    .floor()
                    .max(1.0);
                let size = egui::vec2(160.0 * scale, 144.0 * scale);
                let offset = (available - size) / 2.0;
                let rect = egui::Rect::from_min_size(
                    ui.min_rect().min + egui::vec2(offset.x, offset.y),
                    size,
                );
                ui.put(rect, egui::Image::new(tex).fit_to_exact_size(size));
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("No ROM loaded. Use File → Open ROM...");
                });
            }
        });

        if self.show_debugger {
            self.draw_debugger_window(ctx);
        }

        if self.show_vram_viewer {
            self.draw_vram_viewer_window(ctx);
        }

        if self.show_options {
            self.draw_options_window(ctx);
        }

        if !self.paused {
            ctx.request_repaint();
        }
    }
}

impl VibeEmuApp {
    fn draw_options_window(&mut self, ctx: &egui::Context) {
        let mut show = self.show_options;
        egui::Window::new("Options")
            .open(&mut show)
            .default_size([400.0, 300.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.options_tab == OptionsTab::Keybinds, "Keybinds")
                        .clicked()
                    {
                        self.options_tab = OptionsTab::Keybinds;
                    }
                    if ui
                        .selectable_label(self.options_tab == OptionsTab::Emulation, "Emulation")
                        .clicked()
                    {
                        self.options_tab = OptionsTab::Emulation;
                    }
                });

                ui.separator();
                ui.add_space(8.0);

                match self.options_tab {
                    OptionsTab::Keybinds => {
                        if self.rebinding.is_some() {
                            ui.horizontal(|ui| {
                                ui.colored_label(egui::Color32::YELLOW, "Waiting for key...");
                                if ui.button("Cancel").clicked() {
                                    self.rebinding = None;
                                }
                            });
                            ui.separator();

                            ctx.input(|i| {
                                for key in i.keys_down.iter() {
                                    if let Some(target) = self.rebinding {
                                        self.keybinds.rebind(target, *key);
                                        self.rebinding = None;
                                        break;
                                    }
                                }
                            });
                        }

                        ui.label("Click Rebind, then press a key.");
                        ui.add_space(4.0);

                        egui::Grid::new("keybinds_grid")
                            .num_columns(3)
                            .spacing([20.0, 4.0])
                            .show(ui, |ui| {
                                let fmt_joy = |keybinds: &KeyBindings, mask: u8| -> String {
                                    keybinds
                                        .key_for_joypad_mask(mask)
                                        .map(|k| format!("{k:?}"))
                                        .unwrap_or_else(|| "<unbound>".to_string())
                                };

                                for (label, mask) in [
                                    ("Up", 0x04u8),
                                    ("Down", 0x08),
                                    ("Left", 0x02),
                                    ("Right", 0x01),
                                ] {
                                    ui.label(label);
                                    ui.label(fmt_joy(&self.keybinds, mask));
                                    if ui.button("Rebind").clicked() {
                                        self.rebinding = Some(RebindTarget::Joypad(mask));
                                    }
                                    ui.end_row();
                                }

                                ui.separator();
                                ui.end_row();

                                for (label, mask) in [
                                    ("A", 0x10u8),
                                    ("B", 0x20),
                                    ("Select", 0x40),
                                    ("Start", 0x80),
                                ] {
                                    ui.label(label);
                                    ui.label(fmt_joy(&self.keybinds, mask));
                                    if ui.button("Rebind").clicked() {
                                        self.rebinding = Some(RebindTarget::Joypad(mask));
                                    }
                                    ui.end_row();
                                }
                            });
                    }
                    OptionsTab::Emulation => {
                        ui.horizontal(|ui| {
                            ui.label("DMG Boot ROM:");
                            ui.text_edit_singleline(&mut self.dmg_bootrom_path);
                            if ui.button("Browse...").clicked()
                                && let Some(path) = FileDialog::new().pick_file()
                            {
                                self.dmg_bootrom_path = path.to_string_lossy().to_string();
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("CGB Boot ROM:");
                            ui.text_edit_singleline(&mut self.cgb_bootrom_path);
                            if ui.button("Browse...").clicked()
                                && let Some(path) = FileDialog::new().pick_file()
                            {
                                self.cgb_bootrom_path = path.to_string_lossy().to_string();
                            }
                        });

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label("Window Scale:");
                            egui::ComboBox::from_id_salt("window_scale")
                                .selected_text(match self.selected_window_scale {
                                    0 => "1x",
                                    1 => "2x",
                                    2 => "3x",
                                    3 => "4x",
                                    4 => "5x",
                                    5 => "6x",
                                    _ => "2x",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.selected_window_scale, 0, "1x");
                                    ui.selectable_value(&mut self.selected_window_scale, 1, "2x");
                                    ui.selectable_value(&mut self.selected_window_scale, 2, "3x");
                                    ui.selectable_value(&mut self.selected_window_scale, 3, "4x");
                                    ui.selectable_value(&mut self.selected_window_scale, 4, "5x");
                                    ui.selectable_value(&mut self.selected_window_scale, 5, "6x");
                                });
                        });
                    }
                }
            });
        self.show_options = show;
    }

    fn draw_debugger_window(&mut self, ctx: &egui::Context) {
        let mut show = self.show_debugger;
        egui::Window::new("Debugger")
            .open(&mut show)
            .default_size([600.0, 400.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button(if self.paused {
                            "▶ Resume"
                        } else {
                            "⏸ Pause"
                        })
                        .clicked()
                    {
                        self.paused = !self.paused;
                        let _ = self.emu_tx.send(EmuCommand::SetPaused(self.paused));
                    }
                    if ui.button("⏭ Step").clicked() && self.paused {
                        // TODO: implement single step
                    }
                    if ui.button("🔄 Reset").clicked()
                        && let Ok(mut gb) = self.gb.lock()
                    {
                        gb.reset();
                    }
                });

                ui.separator();

                ui.columns(2, |columns| {
                    columns[0].heading("Registers");
                    if let Ok(gb) = self.gb.lock() {
                        let cpu = &gb.cpu;
                        let af = ((cpu.a as u16) << 8) | (cpu.f as u16);
                        let bc = ((cpu.b as u16) << 8) | (cpu.c as u16);
                        let de = ((cpu.d as u16) << 8) | (cpu.e as u16);
                        let hl = ((cpu.h as u16) << 8) | (cpu.l as u16);
                        columns[0].monospace(format!("AF: {:04X}", af));
                        columns[0].monospace(format!("BC: {:04X}", bc));
                        columns[0].monospace(format!("DE: {:04X}", de));
                        columns[0].monospace(format!("HL: {:04X}", hl));
                        columns[0].monospace(format!("SP: {:04X}", cpu.sp));
                        columns[0].monospace(format!("PC: {:04X}", cpu.pc));
                        columns[0].add_space(8.0);
                        columns[0].monospace(format!(
                            "Flags: {}{}{}{}",
                            if cpu.f & 0x80 != 0 { "Z" } else { "-" },
                            if cpu.f & 0x40 != 0 { "N" } else { "-" },
                            if cpu.f & 0x20 != 0 { "H" } else { "-" },
                            if cpu.f & 0x10 != 0 { "C" } else { "-" },
                        ));
                    }

                    columns[1].heading("Disassembly");
                    columns[1].label("(Disassembly view - TODO)");
                });
            });
        self.show_debugger = show;
    }

    fn draw_vram_viewer_window(&mut self, ctx: &egui::Context) {
        let mut show = self.show_vram_viewer;
        egui::Window::new("VRAM Viewer")
            .open(&mut show)
            .default_size([500.0, 400.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.vram_tab == VramTab::BgMap, "BG Map")
                        .clicked()
                    {
                        self.vram_tab = VramTab::BgMap;
                    }
                    if ui
                        .selectable_label(self.vram_tab == VramTab::Tiles, "Tiles")
                        .clicked()
                    {
                        self.vram_tab = VramTab::Tiles;
                    }
                    if ui
                        .selectable_label(self.vram_tab == VramTab::Oam, "OAM")
                        .clicked()
                    {
                        self.vram_tab = VramTab::Oam;
                    }
                    if ui
                        .selectable_label(self.vram_tab == VramTab::Palettes, "Palettes")
                        .clicked()
                    {
                        self.vram_tab = VramTab::Palettes;
                    }
                });

                ui.separator();

                match self.vram_tab {
                    VramTab::BgMap => {
                        ui.label("Background Map - TODO: render tilemap as texture");
                    }
                    VramTab::Tiles => {
                        ui.label("Tile Data - TODO: render tiles grid as texture");
                    }
                    VramTab::Oam => {
                        ui.label("OAM Sprites - TODO: show sprite table");
                    }
                    VramTab::Palettes => {
                        ui.label("Color Palettes - TODO: show palette swatches");
                    }
                }
            });
        self.show_vram_viewer = show;
    }
}

fn main() {
    let args = Args::parse();
    init_logging(&args);

    let headless = args.headless;
    let rom_path = args.rom.clone();
    let _debug_enabled = args.debug;

    let emulation_mode = if args.dmg {
        EmulationMode::ForceDmg
    } else if args.cgb {
        EmulationMode::ForceCgb
    } else {
        EmulationMode::Auto
    };

    let bootrom_data = args
        .bootrom
        .as_ref()
        .and_then(|path| match std::fs::read(path) {
            Ok(data) => Some(data),
            Err(e) => {
                warn!("Failed to read bootrom: {e}");
                None
            }
        });

    let load_config = LoadConfig {
        emulation_mode,
        dmg_neutral: args.dmg_neutral,
        bootrom_override: bootrom_data,
        dmg_bootrom_path: None,
        cgb_bootrom_path: None,
    };

    let cart: Option<Cartridge> = rom_path.as_ref().and_then(|p| match std::fs::read(p) {
        Ok(data) => Some(Cartridge::load(data)),
        Err(e) => {
            error!("Failed to read ROM: {e}");
            None
        }
    });

    let cgb_mode = match load_config.emulation_mode {
        EmulationMode::ForceDmg => false,
        EmulationMode::ForceCgb => true,
        EmulationMode::Auto => cart.as_ref().is_some_and(|c| c.cgb),
    };

    let mut gb = GameBoy::new_with_mode(cgb_mode);
    if let Some(c) = cart {
        gb.mmu.load_cart(c);
    }

    if headless {
        let frames = args.frames.unwrap_or(600);
        info!("Running headless for {frames} frames");
        for _ in 0..frames {
            gb.mmu.ppu.clear_frame_flag();
            while !gb.mmu.ppu.frame_ready() {
                gb.cpu.step(&mut gb.mmu);
            }
        }
        info!("Headless run complete");
        return;
    }

    let keybinds = KeyBindings::default();
    let gb = Arc::new(Mutex::new(gb));

    let (to_emu_tx, to_emu_rx) = mpsc::channel();
    let (from_emu_frame_tx, from_emu_frame_rx) = cb::bounded(1);
    let (frame_pool_tx, frame_pool_rx) = cb::bounded::<Vec<u32>>(2);
    for _ in 0..2 {
        let _ = frame_pool_tx.send(vec![0u32; 160 * 144]);
    }

    let emu_gb = Arc::clone(&gb);
    let speed = Speed {
        factor: 1.0,
        fast: false,
    };
    let initial_paused = rom_path.is_none();
    let frame_pool_tx_clone = frame_pool_tx.clone();

    let _emu_handle = thread::spawn(move || {
        run_emulator_thread(
            emu_gb,
            speed,
            initial_paused,
            EmuThreadChannels {
                rx: to_emu_rx,
                frame_tx: from_emu_frame_tx,
                frame_pool_tx,
                frame_pool_rx,
            },
        );
    });

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("vibeEmu")
            .with_inner_size([160.0 * 2.0, 144.0 * 2.0 + 24.0])
            .with_icon(load_window_icon().unwrap_or_default()),
        ..Default::default()
    };

    let rom_path_clone = rom_path.clone();

    if let Err(e) = eframe::run_native(
        "vibeEmu",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(VibeEmuApp::new(
                cc,
                gb,
                to_emu_tx,
                from_emu_frame_rx,
                frame_pool_tx_clone,
                rom_path_clone,
                keybinds,
            )))
        }),
    ) {
        error!("eframe error: {e}");
    }
}
