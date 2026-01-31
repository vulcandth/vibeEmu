#![allow(dead_code)]
#![allow(unused_imports)]

mod audio;
mod keybinds;
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
use ui::debugger::{BreakpointSpec, DebuggerPauseReason, DebuggerState};
use ui::snapshot::UiSnapshot;
use ui_config::{EmulationMode, UiConfig, WindowSize};

const DEFAULT_WINDOW_SCALE: u32 = 2;
const GB_WIDTH: f32 = 160.0;
const GB_HEIGHT: f32 = 144.0;
const MENU_BAR_HEIGHT: f32 = 24.0;
const STATUS_BAR_HEIGHT: f32 = 24.0;
const GB_FPS: f64 = 59.7275;
const FRAME_TIME: Duration = Duration::from_nanos((1e9_f64 / GB_FPS) as u64);
const FF_MULT: f32 = 4.0;

use std::sync::LazyLock;
static VIEWPORT_DEBUGGER: LazyLock<egui::ViewportId> =
    LazyLock::new(|| egui::ViewportId::from_hash_of("debugger"));
static VIEWPORT_VRAM_VIEWER: LazyLock<egui::ViewportId> =
    LazyLock::new(|| egui::ViewportId::from_hash_of("vram_viewer"));
static VIEWPORT_WATCHPOINTS: LazyLock<egui::ViewportId> =
    LazyLock::new(|| egui::ViewportId::from_hash_of("watchpoints"));
static VIEWPORT_OPTIONS: LazyLock<egui::ViewportId> =
    LazyLock::new(|| egui::ViewportId::from_hash_of("options"));

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
    logger.filter_module("wgpu_hal", log::LevelFilter::Off);
    logger.filter_module("naga", log::LevelFilter::Warn);
    logger.filter_module("egui_wgpu", log::LevelFilter::Warn);
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
    MobileAdapter,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum VramTab {
    #[default]
    BgMap,
    Tiles,
    Oam,
    Palettes,
}

struct VramViewerState {
    bg_map_tex: Option<egui::TextureHandle>,
    bg_map_buf: Vec<u8>,
    tiles_tex: Option<egui::TextureHandle>,
    tiles_buf: Vec<u8>,
    tiles_banks: u8,
    oam_sprite_textures: Vec<Option<egui::TextureHandle>>,
    oam_sprite_bufs: Vec<Vec<u8>>,
    oam_selected: usize,
    oam_sprite_h: u8,
    palette_sel_is_bg: bool,
    palette_sel_pal: u8,
    palette_sel_col: u8,
    last_frame: u64,
}

impl Default for VramViewerState {
    fn default() -> Self {
        Self {
            bg_map_tex: None,
            bg_map_buf: vec![0; 256 * 256 * 4],
            tiles_tex: None,
            tiles_buf: vec![0; 256 * 192 * 4],
            tiles_banks: 1,
            oam_sprite_textures: vec![None; 40],
            oam_sprite_bufs: (0..40).map(|_| vec![0u8; 8 * 16 * 4]).collect(),
            oam_selected: 0,
            oam_sprite_h: 8,
            palette_sel_is_bg: true,
            palette_sel_pal: 0,
            palette_sel_col: 0,
            last_frame: 0,
        }
    }
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

        let frame_duration = Duration::from_secs_f64(1.0 / (GB_FPS * speed.factor as f64));

        if !speed.fast {
            let now = Instant::now();
            if now < next_frame {
                std::thread::sleep(next_frame - now);
            }

            // Advance next_frame by the frame duration, keeping a fixed schedule.
            // Allow catching up from small delays (up to ~3 frames behind) naturally,
            // only reset if we fall too far behind to prevent runaway catch-up.
            next_frame += frame_duration;
            let max_behind = frame_duration * 3;
            if next_frame + max_behind < Instant::now() {
                next_frame = Instant::now();
            }
        } else {
            // Fast forward: run as fast as possible, reset timing when we exit fast mode
            next_frame = Instant::now() + frame_duration;
        }

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
    _audio_stream: Option<cpal::Stream>,

    framebuffer: Vec<u32>,
    texture: Option<egui::TextureHandle>,
    paused: bool,
    current_rom_path: Option<std::path::PathBuf>,
    keybinds: KeyBindings,
    keybinds_path: std::path::PathBuf,
    joypad_state: u8,
    fast_forward: bool,

    show_debugger: bool,
    show_vram_viewer: bool,
    show_options: bool,

    // Options window state
    emulation_mode: EmulationMode,
    dmg_bootrom_path: String,
    cgb_bootrom_path: String,
    selected_window_scale: usize,
    rebinding: Option<RebindTarget>,
    options_tab: OptionsTab,

    // Debugger state
    debugger_snapshot: Option<UiSnapshot>,
    debugger_state: DebuggerState,
    add_breakpoint_input: String,
    goto_disasm_input: String,

    // VRAM Viewer state
    vram_tab: VramTab,
    vram_viewer: VramViewerState,
    cached_ppu_snapshot: Option<ui::snapshot::PpuSnapshot>,

    // Watchpoints window state
    show_watchpoints: bool,
    watchpoints: Vec<vibe_emu_core::watchpoints::Watchpoint>,
    next_watchpoint_id: u32,
    wp_edit_start_addr: String,
    wp_edit_end_addr: String,
    wp_edit_on_read: bool,
    wp_edit_on_write: bool,

    // Mobile Adapter state
    mobile_enabled: bool,
    mobile_dns1: String,
    mobile_dns2: String,
    mobile_relay: String,

    // Status bar state
    last_fps_update: std::time::Instant,
    frame_count_since_update: u64,
    current_fps: f64,
}

impl VibeEmuApp {
    #[allow(clippy::too_many_arguments)]
    fn new(
        _cc: &eframe::CreationContext<'_>,
        gb: Arc<Mutex<GameBoy>>,
        emu_tx: mpsc::Sender<EmuCommand>,
        frame_rx: cb::Receiver<EmuEvent>,
        frame_pool_tx: cb::Sender<Vec<u32>>,
        rom_path: Option<std::path::PathBuf>,
        keybinds: KeyBindings,
        keybinds_path: std::path::PathBuf,
        emulation_mode: EmulationMode,
    ) -> Self {
        let paused = rom_path.is_none();
        if paused {
            let _ = emu_tx.send(EmuCommand::SetPaused(true));
        }

        let audio_stream = if let Ok(mut gb_lock) = gb.lock() {
            audio::start_stream(&mut gb_lock.mmu.apu, true)
        } else {
            None
        };

        let mut app = Self {
            gb,
            emu_tx,
            frame_rx,
            frame_pool_tx,
            _audio_stream: audio_stream,
            framebuffer: vec![0u32; 160 * 144],
            texture: None,
            paused,
            current_rom_path: rom_path,
            keybinds,
            keybinds_path,
            joypad_state: 0xFF,
            fast_forward: false,
            show_debugger: false,
            show_vram_viewer: false,
            show_options: false,
            emulation_mode,
            dmg_bootrom_path: String::new(),
            cgb_bootrom_path: String::new(),
            selected_window_scale: (DEFAULT_WINDOW_SCALE - 1) as usize,
            rebinding: None,
            options_tab: OptionsTab::default(),
            debugger_snapshot: None,
            debugger_state: DebuggerState::default(),
            add_breakpoint_input: String::new(),
            goto_disasm_input: String::new(),
            vram_tab: VramTab::default(),
            vram_viewer: VramViewerState::default(),
            cached_ppu_snapshot: None,
            show_watchpoints: false,
            watchpoints: Vec::new(),
            next_watchpoint_id: 1,
            wp_edit_start_addr: String::new(),
            wp_edit_end_addr: String::new(),
            wp_edit_on_read: true,
            wp_edit_on_write: true,
            mobile_enabled: false,
            mobile_dns1: String::new(),
            mobile_dns2: String::new(),
            mobile_relay: String::new(),
            last_fps_update: std::time::Instant::now(),
            frame_count_since_update: 0,
            current_fps: 0.0,
        };

        // Load symbols for ROM if one was provided at startup
        if let Some(ref path) = app.current_rom_path {
            app.debugger_state.load_symbols_for_rom_path(Some(path));
        }

        app
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }

        let mut new_state = 0xFFu8;
        let mut new_fast_forward = false;

        ctx.input(|i| {
            for (action, key) in self.keybinds.iter() {
                if i.key_down(*key) {
                    match action.as_str() {
                        "right" => new_state &= !0x01,
                        "left" => new_state &= !0x02,
                        "up" => new_state &= !0x04,
                        "down" => new_state &= !0x08,
                        "a" => new_state &= !0x10,
                        "b" => new_state &= !0x20,
                        "select" => new_state &= !0x40,
                        "start" => new_state &= !0x80,
                        _ => {}
                    }
                }
            }

            new_fast_forward = i.key_down(self.keybinds.fast_forward_key());
        });

        if new_state != self.joypad_state {
            self.joypad_state = new_state;
            let _ = self.emu_tx.send(EmuCommand::UpdateInput(new_state));
        }

        if new_fast_forward != self.fast_forward {
            self.fast_forward = new_fast_forward;
            let _ = self.emu_tx.send(EmuCommand::SetSpeed(Speed {
                factor: 1.0,
                fast: self.fast_forward,
            }));
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
            self.frame_count_since_update += 1;
        }

        let elapsed = self.last_fps_update.elapsed();
        if elapsed >= Duration::from_secs(1) {
            let instant_fps = self.frame_count_since_update as f64 / elapsed.as_secs_f64();
            // Exponential moving average for smoother display
            self.current_fps = self.current_fps * 0.7 + instant_fps * 0.3;
            self.frame_count_since_update = 0;
            self.last_fps_update = std::time::Instant::now();
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
        match Cartridge::from_file(&path) {
            Ok(cart) => {
                let cgb_mode = match self.emulation_mode {
                    EmulationMode::ForceDmg => false,
                    EmulationMode::ForceCgb => true,
                    EmulationMode::Auto => cart.cgb,
                };
                info!(
                    "Loading ROM: {} (CGB header: {}, mode: {:?} → cgb_mode: {})",
                    cart.title, cart.cgb, self.emulation_mode, cgb_mode
                );
                if let Ok(mut gb) = self.gb.lock() {
                    gb.mmu.save_cart_ram();
                    *gb = GameBoy::new_with_mode(cgb_mode);
                    gb.mmu.load_cart(cart);
                    self._audio_stream = audio::start_stream(&mut gb.mmu.apu, true);
                }
                self.current_rom_path = Some(path.clone());
                self.debugger_state.load_symbols_for_rom_path(Some(&path));
                self.paused = false;
                let _ = self.emu_tx.send(EmuCommand::SetPaused(false));
                info!("ROM loaded successfully");
            }
            Err(e) => {
                error!("Failed to load ROM: {e}");
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
                            self._audio_stream = audio::start_stream(&mut gb.mmu.apu, true);
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("Mode", |ui| {
                        if ui
                            .radio_value(
                                &mut self.emulation_mode,
                                EmulationMode::Auto,
                                "Auto (detect from ROM)",
                            )
                            .clicked()
                        {
                            ui.close_menu();
                        }
                        if ui
                            .radio_value(
                                &mut self.emulation_mode,
                                EmulationMode::ForceDmg,
                                "Force DMG",
                            )
                            .clicked()
                        {
                            ui.close_menu();
                        }
                        if ui
                            .radio_value(
                                &mut self.emulation_mode,
                                EmulationMode::ForceCgb,
                                "Force CGB",
                            )
                            .clicked()
                        {
                            ui.close_menu();
                        }
                    });
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
                    if ui.button("Watchpoints").clicked() {
                        self.show_watchpoints = !self.show_watchpoints;
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

        // Status bar at the bottom
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(4.0))
            .show(ctx, |ui| {
                let total_width = ui.available_width();

                ui.horizontal(|ui| {
                    // Emulation status (left side, always visible)
                    let status_text = if self.paused {
                        ("⏸ Paused", egui::Color32::YELLOW)
                    } else if self.fast_forward {
                        ("⏩ Fast", egui::Color32::GREEN)
                    } else {
                        ("▶ Running", egui::Color32::GREEN)
                    };
                    ui.colored_label(status_text.1, status_text.0);

                    // Calculate space needed for FPS (approximate)
                    let fps_text = format!("{:.1} FPS", self.current_fps);
                    let fps_reserve = 80.0;

                    let remaining = total_width - ui.min_rect().width() - fps_reserve - 30.0;

                    // ROM name only if there's enough room
                    if remaining > 60.0
                        && let Some(path) = &self.current_rom_path
                        && let Some(name) = path.file_name().and_then(|n| n.to_str())
                    {
                        ui.separator();
                        ui.add_sized(
                            [remaining.min(200.0), ui.available_height()],
                            egui::Label::new(name)
                                .truncate()
                                .wrap_mode(egui::TextWrapMode::Truncate),
                        );
                    }

                    // FPS counter (right-aligned)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(fps_text);
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                if let Some(tex) = &self.texture {
                    let available = ui.available_size();
                    let scale = (available.x / GB_WIDTH)
                        .min(available.y / GB_HEIGHT)
                        .floor()
                        .max(1.0);
                    let size = egui::vec2(GB_WIDTH * scale, GB_HEIGHT * scale);
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

        if self.show_watchpoints {
            self.draw_watchpoints_window(ctx);
        }

        if self.show_options {
            self.draw_options_window(ctx);
        }

        if !self.paused {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self) {
        if let Ok(mut gb) = self.gb.lock() {
            gb.mmu.save_cart_ram();
        }
        let _ = self.emu_tx.send(EmuCommand::Shutdown);
    }
}

impl VibeEmuApp {
    fn draw_options_window(&mut self, ctx: &egui::Context) {
        ctx.show_viewport_immediate(
            *VIEWPORT_OPTIONS,
            egui::ViewportBuilder::default()
                .with_title("Options")
                .with_inner_size([400.0, 300.0]),
            |ctx, class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.show_options = false;
                }

                match class {
                    egui::ViewportClass::Embedded => {
                        egui::Window::new("Options").show(ctx, |ui| {
                            self.draw_options_content(ui, ctx);
                        });
                    }
                    _ => {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            self.draw_options_content(ui, ctx);
                        });
                    }
                }
            },
        );
    }

    fn draw_options_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
            if ui
                .selectable_label(
                    self.options_tab == OptionsTab::MobileAdapter,
                    "Mobile Adapter",
                )
                .clicked()
            {
                self.options_tab = OptionsTab::MobileAdapter;
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
                                if let Err(e) = self.keybinds.save_to_file(&self.keybinds_path) {
                                    log::warn!("Failed to save keybinds: {e}");
                                }
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

                        ui.separator();
                        ui.end_row();

                        ui.label("Fast Forward");
                        ui.label(format!("{:?}", self.keybinds.fast_forward_key()));
                        if ui.button("Rebind").clicked() {
                            self.rebinding = Some(RebindTarget::FastForward);
                        }
                        ui.end_row();
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
                    let prev_scale = self.selected_window_scale;
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
                    if self.selected_window_scale != prev_scale {
                        let scale = (self.selected_window_scale + 1) as f32;
                        let new_size = egui::vec2(
                            GB_WIDTH * scale,
                            GB_HEIGHT * scale + MENU_BAR_HEIGHT + STATUS_BAR_HEIGHT,
                        );
                        ctx.send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::InnerSize(new_size),
                        );
                    }
                });
            }
            OptionsTab::MobileAdapter => {
                ui.checkbox(&mut self.mobile_enabled, "Enable Mobile Adapter");

                ui.add_enabled_ui(self.mobile_enabled, |ui| {
                    ui.separator();
                    ui.label("DNS Servers:");
                    ui.horizontal(|ui| {
                        ui.label("Primary:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.mobile_dns1)
                                .desired_width(150.0)
                                .hint_text("e.g. 8.8.8.8"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Secondary:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.mobile_dns2)
                                .desired_width(150.0)
                                .hint_text("e.g. 8.8.4.4"),
                        );
                    });

                    ui.separator();
                    ui.label("Relay Server:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.mobile_relay)
                            .desired_width(250.0)
                            .hint_text("relay.example.com:port"),
                    );

                    ui.separator();
                    ui.label("Note: Changes require ROM reload to take effect.");
                });
            }
        }
    }

    fn draw_debugger_window(&mut self, ctx: &egui::Context) {
        // Use try_lock to avoid blocking the emulator thread during fast forward
        if let Ok(mut gb) = self.gb.try_lock() {
            self.debugger_snapshot = Some(UiSnapshot::from_gb(&mut gb, self.paused));
        }

        ctx.show_viewport_immediate(
            *VIEWPORT_DEBUGGER,
            egui::ViewportBuilder::default()
                .with_title("Debugger")
                .with_inner_size([700.0, 500.0]),
            |ctx, class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.show_debugger = false;
                }

                match class {
                    egui::ViewportClass::Embedded => {
                        egui::Window::new("Debugger").show(ctx, |ui| {
                            self.draw_debugger_content(ui);
                        });
                    }
                    _ => {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            self.draw_debugger_content(ui);
                        });
                    }
                }
            },
        );
    }

    fn draw_debugger_content(&mut self, ui: &mut egui::Ui) {
        let Some(snapshot) = self.debugger_snapshot.clone() else {
            ui.label("Unable to access emulator state");
            return;
        };

        self.draw_debugger_toolbar(ui, &snapshot);
        ui.separator();

        ui.columns(2, |columns| {
            self.draw_disassembly_pane(&mut columns[0], &snapshot);
            self.draw_state_panes(&mut columns[1], &snapshot);
        });

        if let Some(status) = self.debugger_state.status_line() {
            ui.separator();
            ui.label(egui::RichText::new(status).weak());
        }
    }

    fn draw_debugger_toolbar(&mut self, ui: &mut egui::Ui, snapshot: &UiSnapshot) {
        let paused = self.paused;

        ui.horizontal(|ui| {
            let run_label = if paused { "▶ Run" } else { "⏸ Pause" };
            if ui.button(run_label).clicked() {
                if paused {
                    self.debugger_state.request_continue_and_focus_main();
                    self.paused = false;
                    let _ = self.emu_tx.send(EmuCommand::SetPaused(false));
                } else {
                    self.debugger_state.request_pause();
                    self.paused = true;
                    let _ = self.emu_tx.send(EmuCommand::SetPaused(true));
                }
            }

            if ui.button("⏭ Step").clicked() && paused {
                self.do_single_step();
            }

            if paused {
                if ui.button("Step Over").clicked() {
                    self.debugger_state.request_step_over();
                }
                if ui.button("Step Out").clicked() {
                    self.debugger_state.request_step_out();
                }
                if ui.button("Jump").clicked() {
                    self.debugger_state.request_jump_to_cursor();
                }
                if ui.button("Call").clicked() {
                    self.debugger_state.request_call_cursor();
                }
            }

            if let Some(reason) = self.debugger_state.pause_reason() {
                ui.separator();
                let reason_text = match reason {
                    DebuggerPauseReason::Manual => "Paused (manual)".to_string(),
                    DebuggerPauseReason::Step => "Paused (step)".to_string(),
                    DebuggerPauseReason::DebuggerFocus => "Paused (debugger focus)".to_string(),
                    DebuggerPauseReason::Breakpoint { bank, addr } => {
                        format!("Paused (breakpoint {:02X}:{:04X})", bank, addr)
                    }
                    DebuggerPauseReason::Watchpoint {
                        trigger,
                        addr,
                        value,
                        pc,
                    } => {
                        let label = match trigger {
                            vibe_emu_core::watchpoints::WatchpointTrigger::Read => "read",
                            vibe_emu_core::watchpoints::WatchpointTrigger::Write => "write",
                            vibe_emu_core::watchpoints::WatchpointTrigger::Execute => "execute",
                            vibe_emu_core::watchpoints::WatchpointTrigger::Jump => "jump",
                        };
                        let value_str = value.map(|v| format!("=${v:02X} ")).unwrap_or_default();
                        let pc_str = pc.map(|p| format!("pc={p:04X} ")).unwrap_or_default();
                        format!("Paused (watchpoint {label} {pc_str}{value_str}@ {addr:04X})")
                    }
                };
                ui.label(egui::RichText::new(reason_text).weak());
            }
        });

        ui.horizontal(|ui| {
            ui.label("BP:");
            let bp_resp = ui.add(
                egui::TextEdit::singleline(&mut self.add_breakpoint_input)
                    .desired_width(100.0)
                    .font(egui::TextStyle::Monospace),
            );
            let bp_submitted =
                bp_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Add").clicked() || bp_submitted)
                && let Some(bp) = self
                    .debugger_state
                    .parse_breakpoint_input(&self.add_breakpoint_input, snapshot)
            {
                self.debugger_state.add_breakpoint(bp);
                self.add_breakpoint_input.clear();
            }
            if ui.button("Clear").clicked() {
                self.debugger_state.clear_breakpoints();
            }

            ui.separator();

            ui.label("Go:");
            let goto_resp = ui.add(
                egui::TextEdit::singleline(&mut self.goto_disasm_input)
                    .desired_width(120.0)
                    .font(egui::TextStyle::Monospace),
            );
            let goto_submitted =
                goto_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Go##goto_btn").clicked() || goto_submitted {
                self.debugger_state
                    .goto_address(&self.goto_disasm_input, snapshot);
                self.goto_disasm_input.clear();
            }

            if ui.button("Reload .sym").clicked() {
                self.debugger_state.reload_symbols();
            }
        });
    }

    fn do_single_step(&mut self) {
        if let Ok(mut gb) = self.gb.lock() {
            let GameBoy { cpu, mmu, .. } = &mut *gb;
            cpu.step(mmu);
            // Update snapshot immediately after step so disassembly shows correct memory
            self.debugger_snapshot = Some(UiSnapshot::from_gb(&mut gb, true));
        }
        self.debugger_state
            .set_pause_reason(DebuggerPauseReason::Step);
    }

    fn draw_disassembly_pane(&mut self, ui: &mut egui::Ui, snapshot: &UiSnapshot) {
        ui.heading("Disassembly");

        let pc = snapshot.cpu.pc;
        let dbg = &snapshot.debugger;
        let active_bank = dbg.active_rom_bank.min(0xFF) as u8;

        let start_addr = pc.saturating_sub(0x30);

        let mem: Vec<u8> = if let Some(mem_image) = &dbg.mem_image {
            (0..0x100)
                .map(|i| {
                    let addr = start_addr.wrapping_add(i);
                    mem_image.get(addr as usize).copied().unwrap_or(0)
                })
                .collect()
        } else {
            vec![0; 0x100]
        };

        let mut bp_toggle: Option<BreakpointSpec> = None;
        let mut cursor_click: Option<BreakpointSpec> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("disasm_grid")
                    .num_columns(4)
                    .spacing([8.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("BP");
                        ui.strong("Addr");
                        ui.strong("Bytes");
                        ui.strong("Instruction");
                        ui.end_row();

                        let mut addr = start_addr;
                        while addr < start_addr.wrapping_add(0x80) {
                            let rel_addr = addr.wrapping_sub(start_addr) as usize;
                            if rel_addr >= mem.len() {
                                break;
                            }

                            let (mnemonic, len) = ui::disasm::decode_sm83(&mem[rel_addr..], addr);
                            let bytes = ui::disasm::format_bytes(&mem[rel_addr..], 0, len);

                            let bp_bank = if (0x4000..=0x7FFF).contains(&addr) {
                                active_bank
                            } else if addr < 0x4000 {
                                0
                            } else {
                                0xFF
                            };

                            let bp_spec = BreakpointSpec {
                                bank: bp_bank,
                                addr,
                            };
                            let bp_enabled = self.debugger_state.has_breakpoint(&bp_spec);
                            let is_cursor = self.debugger_state.cursor() == Some(bp_spec);
                            let is_pc = addr == pc;

                            let bp_symbol = match bp_enabled {
                                Some(true) => "●",
                                Some(false) => "○",
                                None => " ",
                            };
                            let bp_color = match bp_enabled {
                                Some(true) => egui::Color32::RED,
                                Some(false) => egui::Color32::DARK_RED,
                                None => egui::Color32::GRAY,
                            };

                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(bp_symbol).color(bp_color),
                                    )
                                    .frame(false)
                                    .min_size(egui::vec2(16.0, 0.0)),
                                )
                                .clicked()
                            {
                                bp_toggle = Some(bp_spec);
                            }

                            let display_bank = if addr < 0x4000 {
                                0
                            } else if (0x4000..=0x7FFF).contains(&addr) {
                                active_bank
                            } else {
                                0xFF
                            };

                            let addr_text = if display_bank == 0xFF {
                                format!("--:{:04X}", addr)
                            } else {
                                format!("{:02X}:{:04X}", display_bank, addr)
                            };

                            let text_color = if is_pc {
                                egui::Color32::YELLOW
                            } else if is_cursor {
                                egui::Color32::LIGHT_BLUE
                            } else {
                                ui.style().visuals.text_color()
                            };

                            let addr_resp = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&addr_text)
                                        .color(text_color)
                                        .monospace(),
                                )
                                .sense(egui::Sense::click()),
                            );
                            if addr_resp.clicked() {
                                cursor_click = Some(bp_spec);
                            }

                            let bytes_resp = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&bytes).color(text_color).monospace(),
                                )
                                .sense(egui::Sense::click()),
                            );
                            if bytes_resp.clicked() {
                                cursor_click = Some(bp_spec);
                            }

                            let label = self.debugger_state.first_label_for(bp_bank, addr);
                            let instr_text = if let Some(lbl) = label {
                                format!("{lbl}: {mnemonic}")
                            } else {
                                mnemonic
                            };

                            let instr_resp = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&instr_text)
                                        .color(text_color)
                                        .monospace(),
                                )
                                .sense(egui::Sense::click()),
                            );
                            if instr_resp.clicked() {
                                cursor_click = Some(bp_spec);
                            }

                            ui.end_row();
                            addr = addr.wrapping_add(len);
                        }
                    });
            });

        if let Some(bp) = bp_toggle {
            self.debugger_state.toggle_breakpoint(bp);
        }
        if let Some(bp) = cursor_click {
            self.debugger_state.set_cursor(bp);
        }
    }

    fn draw_state_panes(&mut self, ui: &mut egui::Ui, snapshot: &UiSnapshot) {
        let cpu = &snapshot.cpu;

        ui.heading("CPU");
        egui::Grid::new("cpu_regs")
            .num_columns(2)
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                let af = ((cpu.a as u16) << 8) | cpu.f as u16;
                let bc = ((cpu.b as u16) << 8) | cpu.c as u16;
                let de = ((cpu.d as u16) << 8) | cpu.e as u16;
                let hl = ((cpu.h as u16) << 8) | cpu.l as u16;

                ui.monospace("AF");
                ui.monospace(format!("{:04X}", af));
                ui.end_row();
                ui.monospace("BC");
                ui.monospace(format!("{:04X}", bc));
                ui.end_row();
                ui.monospace("DE");
                ui.monospace(format!("{:04X}", de));
                ui.end_row();
                ui.monospace("HL");
                ui.monospace(format!("{:04X}", hl));
                ui.end_row();
                ui.monospace("SP");
                ui.monospace(format!("{:04X}", cpu.sp));
                ui.end_row();
                ui.monospace("PC");
                ui.monospace(format!("{:04X}", cpu.pc));
                ui.end_row();

                let f = cpu.f;
                let z = if (f & 0x80) != 0 { 'Z' } else { '-' };
                let n = if (f & 0x40) != 0 { 'N' } else { '-' };
                let h = if (f & 0x20) != 0 { 'H' } else { '-' };
                let c = if (f & 0x10) != 0 { 'C' } else { '-' };
                ui.monospace("Flags");
                ui.monospace(format!("{z}{n}{h}{c}"));
                ui.end_row();

                ui.monospace("IME");
                ui.monospace(format!("{}", cpu.ime));
                ui.end_row();
                ui.monospace("Cycles");
                ui.monospace(format!("{}", cpu.cycles));
                ui.end_row();
            });

        ui.separator();
        ui.heading("I/O");
        egui::Grid::new("io_regs")
            .num_columns(2)
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                ui.monospace("LCDC");
                ui.monospace(format!("{:02X}", snapshot.ppu.lcdc));
                ui.end_row();
                ui.monospace("STAT");
                ui.monospace(format!("{:02X}", snapshot.ppu.stat));
                ui.end_row();
                ui.monospace("LY");
                ui.monospace(format!("{:02X}", snapshot.ppu.ly));
                ui.end_row();
                ui.monospace("SCX");
                ui.monospace(format!("{:02X}", snapshot.ppu.scx));
                ui.end_row();
                ui.monospace("SCY");
                ui.monospace(format!("{:02X}", snapshot.ppu.scy));
                ui.end_row();
                ui.monospace("IF");
                ui.monospace(format!("{:02X}", snapshot.debugger.if_reg));
                ui.end_row();
                ui.monospace("IE");
                ui.monospace(format!("{:02X}", snapshot.debugger.ie_reg));
                ui.end_row();
            });

        ui.separator();
        ui.heading("Breakpoints");

        let mut to_remove: Option<BreakpointSpec> = None;
        let entries: Vec<(BreakpointSpec, bool)> = self
            .debugger_state
            .all_breakpoints()
            .map(|(&bp, &en)| (bp, en))
            .collect();

        egui::ScrollArea::vertical()
            .id_salt("bp_list")
            .max_height(120.0)
            .show(ui, |ui| {
                for (bp, enabled) in entries {
                    ui.horizontal(|ui| {
                        let mut en = enabled;
                        if ui.checkbox(&mut en, "").changed() {
                            self.debugger_state.toggle_breakpoint(bp);
                        }

                        let sym_label = self.debugger_state.first_label_for(bp.bank, bp.addr);
                        let label = if let Some(sym) = sym_label {
                            format!("{:02X}:{:04X}  {sym}", bp.bank, bp.addr)
                        } else {
                            format!("{:02X}:{:04X}", bp.bank, bp.addr)
                        };
                        ui.monospace(&label);

                        if ui.small_button("Remove").clicked() {
                            to_remove = Some(bp);
                        }
                    });
                }
            });

        if let Some(bp) = to_remove {
            self.debugger_state.remove_breakpoint(&bp);
        }

        ui.separator();
        ui.heading("Stack");

        let base = snapshot.debugger.stack_base;
        let bytes = &snapshot.debugger.stack_bytes;

        egui::ScrollArea::vertical()
            .id_salt("stack_view")
            .max_height(100.0)
            .show(ui, |ui| {
                for (i, chunk) in bytes.chunks_exact(2).take(16).enumerate() {
                    let addr = base.wrapping_add((i as u16) * 2);
                    let val = (chunk[1] as u16) << 8 | (chunk[0] as u16);
                    ui.monospace(format!("{addr:04X}: {val:04X}"));
                }
            });
    }

    fn draw_watchpoints_window(&mut self, ctx: &egui::Context) {
        ctx.show_viewport_immediate(
            *VIEWPORT_WATCHPOINTS,
            egui::ViewportBuilder::default()
                .with_title("Watchpoints")
                .with_inner_size([400.0, 300.0]),
            |ctx, class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.show_watchpoints = false;
                }

                match class {
                    egui::ViewportClass::Embedded => {
                        egui::Window::new("Watchpoints").show(ctx, |ui| {
                            self.draw_watchpoints_content(ui);
                        });
                    }
                    _ => {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            self.draw_watchpoints_content(ui);
                        });
                    }
                }
            },
        );
    }

    fn draw_watchpoints_content(&mut self, ui: &mut egui::Ui) {
        ui.heading("Add Watchpoint");
        ui.horizontal(|ui| {
            ui.label("Start:");
            ui.add(
                egui::TextEdit::singleline(&mut self.wp_edit_start_addr)
                    .desired_width(60.0)
                    .font(egui::TextStyle::Monospace),
            );
            ui.label("End:");
            ui.add(
                egui::TextEdit::singleline(&mut self.wp_edit_end_addr)
                    .desired_width(60.0)
                    .font(egui::TextStyle::Monospace),
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.wp_edit_on_read, "Read");
            ui.checkbox(&mut self.wp_edit_on_write, "Write");
            if ui.button("Add").clicked() {
                let start_str = self.wp_edit_start_addr.trim();
                let start_str = start_str
                    .strip_prefix("$")
                    .or_else(|| start_str.strip_prefix("0x"))
                    .unwrap_or(start_str);
                let end_str = self.wp_edit_end_addr.trim();
                let end_str = end_str
                    .strip_prefix("$")
                    .or_else(|| end_str.strip_prefix("0x"))
                    .unwrap_or(end_str);

                if let Ok(start) = u16::from_str_radix(start_str, 16) {
                    let end = u16::from_str_radix(end_str, 16).unwrap_or(start);
                    let wp = vibe_emu_core::watchpoints::Watchpoint {
                        id: self.next_watchpoint_id,
                        enabled: true,
                        range: start..=end,
                        on_read: self.wp_edit_on_read,
                        on_write: self.wp_edit_on_write,
                        on_execute: false,
                        on_jump: false,
                        value_match: None,
                        message: None,
                    };
                    self.next_watchpoint_id += 1;
                    self.watchpoints.push(wp);
                    self.wp_edit_start_addr.clear();
                    self.wp_edit_end_addr.clear();
                }
            }
        });

        ui.separator();
        ui.heading("Active Watchpoints");

        let mut to_remove: Option<usize> = None;
        let mut to_toggle: Option<usize> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("watchpoints_grid")
                    .num_columns(5)
                    .spacing([12.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("En");
                        ui.strong("Range");
                        ui.strong("R");
                        ui.strong("W");
                        ui.strong("");
                        ui.end_row();

                        for (i, wp) in self.watchpoints.iter().enumerate() {
                            let enabled_text = if wp.enabled { "✓" } else { "○" };
                            if ui.button(enabled_text).clicked() {
                                to_toggle = Some(i);
                            }

                            let start = *wp.range.start();
                            let end = *wp.range.end();
                            if start == end {
                                ui.monospace(format!("${:04X}", start));
                            } else {
                                ui.monospace(format!("${:04X}-${:04X}", start, end));
                            }

                            ui.monospace(if wp.on_read { "R" } else { "-" });
                            ui.monospace(if wp.on_write { "W" } else { "-" });

                            if ui.button("✕").clicked() {
                                to_remove = Some(i);
                            }
                            ui.end_row();
                        }
                    });
            });

        if let Some(i) = to_toggle
            && let Some(wp) = self.watchpoints.get_mut(i)
        {
            wp.enabled = !wp.enabled;
        }
        if let Some(i) = to_remove {
            self.watchpoints.remove(i);
        }
    }

    fn draw_vram_viewer_window(&mut self, ctx: &egui::Context) {
        // Use try_lock to avoid blocking the emulator thread during fast forward
        if let Ok(mut gb) = self.gb.try_lock() {
            self.cached_ppu_snapshot = Some(UiSnapshot::from_gb(&mut gb, self.paused).ppu);
        }

        // Clone so we can pass it into the closure without borrowing self
        let ppu_snapshot = self.cached_ppu_snapshot.clone();

        ctx.show_viewport_immediate(
            *VIEWPORT_VRAM_VIEWER,
            egui::ViewportBuilder::default()
                .with_title("VRAM Viewer")
                .with_inner_size([520.0, 450.0]),
            |ctx, class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.show_vram_viewer = false;
                }

                match class {
                    egui::ViewportClass::Embedded => {
                        egui::Window::new("VRAM Viewer").show(ctx, |ui| {
                            self.draw_vram_viewer_content(ui, ctx, ppu_snapshot.as_ref());
                        });
                    }
                    _ => {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            self.draw_vram_viewer_content(ui, ctx, ppu_snapshot.as_ref());
                        });
                    }
                }
            },
        );
    }

    fn draw_vram_viewer_content(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        ppu_snapshot: Option<&ui::snapshot::PpuSnapshot>,
    ) {
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

        if let Some(ppu) = ppu_snapshot {
            match self.vram_tab {
                VramTab::BgMap => self.draw_bg_map_tab(ui, ctx, ppu),
                VramTab::Tiles => self.draw_tiles_tab(ui, ctx, ppu),
                VramTab::Oam => self.draw_oam_tab(ui, ctx, ppu),
                VramTab::Palettes => self.draw_palettes_tab(ui, ppu),
            }
        } else {
            ui.label("Unable to access emulator state");
        }
    }

    fn draw_bg_map_tab(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        ppu: &ui::snapshot::PpuSnapshot,
    ) {
        const DMG_COLORS: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];
        const MAP_W: usize = 32;
        const MAP_H: usize = 32;
        const TILE: usize = 8;
        const IMG_W: usize = MAP_W * TILE;
        const IMG_H: usize = MAP_H * TILE;

        let frame = ppu.frame_counter;
        if frame != self.vram_viewer.last_frame || self.vram_viewer.bg_map_tex.is_none() {
            self.vram_viewer.last_frame = frame;
            let rgba = &mut self.vram_viewer.bg_map_buf;
            rgba.fill(0);

            let lcdc = ppu.lcdc;
            let map_base = if lcdc & 0x08 != 0 { 0x1C00 } else { 0x1800 };
            let signed_mode = lcdc & 0x10 == 0;
            let bgp = ppu.bgp;
            let cgb = ppu.cgb;

            for tile_y in 0..MAP_H {
                for tile_x in 0..MAP_W {
                    let tile_idx = ppu.vram0[map_base + tile_y * MAP_W + tile_x];
                    let attr = if cgb {
                        ppu.vram1[map_base + tile_y * MAP_W + tile_x]
                    } else {
                        0
                    };
                    let tile_num = if signed_mode {
                        tile_idx as i8 as i16
                    } else {
                        tile_idx as i16
                    };

                    let tile_addr = if signed_mode {
                        (0x1000i32 + (tile_num as i32) * 16) as usize
                    } else {
                        (tile_num as usize) * 16
                    };

                    let bank = if cgb && attr & 0x08 != 0 { 1 } else { 0 };
                    let x_flip = cgb && attr & 0x20 != 0;
                    let y_flip = cgb && attr & 0x40 != 0;
                    let vram = ppu.vram_bank(bank);
                    if tile_addr + 16 > vram.len() {
                        continue;
                    }
                    for row in 0..TILE {
                        let actual_row = if y_flip { 7 - row } else { row };
                        let lo = vram[tile_addr + actual_row * 2];
                        let hi = vram[tile_addr + actual_row * 2 + 1];
                        for col in 0..TILE {
                            let actual_col = if x_flip { col } else { 7 - col };
                            let bit = actual_col;
                            let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
                            let color = if cgb {
                                let pal = (attr & 0x07) as usize;
                                ppu.cgb_bg_colors[pal][idx as usize]
                            } else {
                                let shade = (bgp >> (idx * 2)) & 0x03;
                                DMG_COLORS[shade as usize]
                            };
                            let x = tile_x * TILE + col;
                            let y = tile_y * TILE + row;
                            let off = (y * IMG_W + x) * 4;
                            rgba[off] = ((color >> 16) & 0xFF) as u8;
                            rgba[off + 1] = ((color >> 8) & 0xFF) as u8;
                            rgba[off + 2] = (color & 0xFF) as u8;
                            rgba[off + 3] = 0xFF;
                        }
                    }
                }
            }

            let pixels: Vec<egui::Color32> = rgba
                .chunks_exact(4)
                .map(|c| egui::Color32::from_rgb(c[0], c[1], c[2]))
                .collect();
            let image = egui::ColorImage {
                size: [IMG_W, IMG_H],
                pixels,
            };
            match &mut self.vram_viewer.bg_map_tex {
                Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
                None => {
                    self.vram_viewer.bg_map_tex =
                        Some(ctx.load_texture("bg_map", image, egui::TextureOptions::NEAREST));
                }
            }
        }

        if let Some(tex) = &self.vram_viewer.bg_map_tex {
            let avail = ui.available_size();
            let scale = (avail.x / 256.0).min(avail.y / 256.0).clamp(1.0, 2.0);
            let draw_size = egui::vec2(256.0 * scale, 256.0 * scale);

            let (response, painter) = ui.allocate_painter(draw_size, egui::Sense::hover());
            let rect = response.rect;

            painter.image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );

            // Draw viewport rectangle(s) with wrapping support
            let scx = ppu.scx as f32;
            let scy = ppu.scy as f32;
            let vp_w = 160.0;
            let vp_h = 144.0;
            let map_size = 256.0;

            let stroke = egui::Stroke::new(1.0, egui::Color32::RED);

            // Calculate wrapped regions
            let x_wraps = scx + vp_w > map_size;
            let y_wraps = scy + vp_h > map_size;

            if !x_wraps && !y_wraps {
                // Simple case: no wrapping
                let viewport_rect = egui::Rect::from_min_size(
                    rect.min + egui::vec2(scx * scale, scy * scale),
                    egui::vec2(vp_w * scale, vp_h * scale),
                );
                painter.rect_stroke(viewport_rect, 0.0, stroke);
            } else {
                // Draw up to 4 rectangles for wrapped viewport
                let x1_start = scx;
                let x1_end = if x_wraps { map_size } else { scx + vp_w };
                let x1_w = x1_end - x1_start;

                let x2_start = 0.0;
                let x2_w = if x_wraps {
                    (scx + vp_w) - map_size
                } else {
                    0.0
                };

                let y1_start = scy;
                let y1_end = if y_wraps { map_size } else { scy + vp_h };
                let y1_h = y1_end - y1_start;

                let y2_start = 0.0;
                let y2_h = if y_wraps {
                    (scy + vp_h) - map_size
                } else {
                    0.0
                };

                // Top-left region (always present)
                let r1 = egui::Rect::from_min_size(
                    rect.min + egui::vec2(x1_start * scale, y1_start * scale),
                    egui::vec2(x1_w * scale, y1_h * scale),
                );
                painter.rect_stroke(r1, 0.0, stroke);

                // Top-right region (if x wraps)
                if x_wraps {
                    let r2 = egui::Rect::from_min_size(
                        rect.min + egui::vec2(x2_start * scale, y1_start * scale),
                        egui::vec2(x2_w * scale, y1_h * scale),
                    );
                    painter.rect_stroke(r2, 0.0, stroke);
                }

                // Bottom-left region (if y wraps)
                if y_wraps {
                    let r3 = egui::Rect::from_min_size(
                        rect.min + egui::vec2(x1_start * scale, y2_start * scale),
                        egui::vec2(x1_w * scale, y2_h * scale),
                    );
                    painter.rect_stroke(r3, 0.0, stroke);
                }

                // Bottom-right region (if both wrap)
                if x_wraps && y_wraps {
                    let r4 = egui::Rect::from_min_size(
                        rect.min + egui::vec2(x2_start * scale, y2_start * scale),
                        egui::vec2(x2_w * scale, y2_h * scale),
                    );
                    painter.rect_stroke(r4, 0.0, stroke);
                }
            }
        }
    }

    fn draw_tiles_tab(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        ppu: &ui::snapshot::PpuSnapshot,
    ) {
        const DMG_COLORS: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];
        const TILE_W: usize = 8;
        const TILE_H: usize = 8;
        const TILES_PER_ROW: usize = 16;
        const ROWS: usize = 24;

        let banks: usize = if ppu.cgb { 2 } else { 1 };
        let img_w = TILES_PER_ROW * TILE_W * banks;
        let img_h = ROWS * TILE_H;

        if banks != self.vram_viewer.tiles_banks as usize {
            self.vram_viewer.tiles_banks = banks as u8;
            self.vram_viewer.tiles_tex = None;
        }

        let frame = ppu.frame_counter;
        if frame != self.vram_viewer.last_frame || self.vram_viewer.tiles_tex.is_none() {
            let buf = &mut self.vram_viewer.tiles_buf[..img_w * img_h * 4];
            buf.fill(0);

            let bgp = ppu.bgp;

            for bank in 0..banks {
                for tile_idx in 0..384 {
                    let col = tile_idx % TILES_PER_ROW;
                    let row = tile_idx / TILES_PER_ROW;
                    let tile_addr = tile_idx * 16;

                    for y in 0..TILE_H {
                        let vram = ppu.vram_bank(bank);
                        let lo = vram[tile_addr + y * 2];
                        let hi = vram[tile_addr + y * 2 + 1];

                        for x in 0..TILE_W {
                            let bit = 7 - x;
                            let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);

                            let rgb = if ppu.cgb {
                                ppu.cgb_bg_colors[0][idx as usize]
                            } else {
                                let shade = (bgp >> (idx * 2)) & 0x03;
                                DMG_COLORS[shade as usize]
                            };

                            let px = (bank * 128) + (col * TILE_W) + x;
                            let py = row * TILE_H + y;
                            let off = (py * img_w + px) * 4;

                            buf[off] = ((rgb >> 16) & 0xFF) as u8;
                            buf[off + 1] = ((rgb >> 8) & 0xFF) as u8;
                            buf[off + 2] = (rgb & 0xFF) as u8;
                            buf[off + 3] = 0xFF;
                        }
                    }
                }
            }

            let pixels: Vec<egui::Color32> = buf
                .chunks_exact(4)
                .map(|c| egui::Color32::from_rgb(c[0], c[1], c[2]))
                .collect();
            let image = egui::ColorImage {
                size: [img_w, img_h],
                pixels,
            };
            match &mut self.vram_viewer.tiles_tex {
                Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
                None => {
                    self.vram_viewer.tiles_tex =
                        Some(ctx.load_texture("tiles", image, egui::TextureOptions::NEAREST));
                }
            }
        }

        if let Some(tex) = &self.vram_viewer.tiles_tex {
            let avail = ui.available_size();
            let tex_w = img_w as f32;
            let tex_h = img_h as f32;
            let scale = (avail.x / tex_w).min(avail.y / tex_h).clamp(1.0, 3.0);
            let draw_size = egui::vec2(tex_w * scale, tex_h * scale);
            ui.image((tex.id(), draw_size));
        }
    }

    fn draw_oam_tab(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        ppu: &ui::snapshot::PpuSnapshot,
    ) {
        let sprite_h: u8 = if ppu.lcdc & 0x04 != 0 { 16 } else { 8 };

        if sprite_h != self.vram_viewer.oam_sprite_h {
            self.vram_viewer.oam_sprite_h = sprite_h;
        }

        ui.columns(2, |columns| {
            columns[0].heading("OAM Entries");
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(&mut columns[0], |ui| {
                    egui::Grid::new("oam_grid")
                        .num_columns(6)
                        .spacing([8.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("#");
                            ui.strong("Y");
                            ui.strong("X");
                            ui.strong("Tile");
                            ui.strong("Attr");
                            ui.strong("");
                            ui.end_row();

                            for i in 0..40 {
                                let base = i * 4;
                                let y_pos = ppu.oam[base];
                                let x_pos = ppu.oam[base + 1];
                                let tile_num = ppu.oam[base + 2];
                                let attr = ppu.oam[base + 3];

                                let selected = self.vram_viewer.oam_selected == i;
                                if ui.selectable_label(selected, format!("{:02}", i)).clicked() {
                                    self.vram_viewer.oam_selected = i;
                                }
                                ui.monospace(format!("{:02X}", y_pos));
                                ui.monospace(format!("{:02X}", x_pos));
                                ui.monospace(format!("{:02X}", tile_num));
                                ui.monospace(format!("{:02X}", attr));
                                ui.label("");
                                ui.end_row();
                            }
                        });
                });

            columns[1].heading("Details");
            columns[1].separator();

            let i = self.vram_viewer.oam_selected;
            if i < 40 {
                let base = i * 4;
                let y_pos = ppu.oam[base];
                let x_pos = ppu.oam[base + 1];
                let tile_num = ppu.oam[base + 2];
                let attr = ppu.oam[base + 3];

                let x_flip = attr & 0x20 != 0;
                let y_flip = attr & 0x40 != 0;
                let priority = attr & 0x80 != 0;

                columns[1].monospace(format!("Sprite #{}", i));
                columns[1].monospace(format!("Position: ({}, {})", x_pos, y_pos));
                columns[1].monospace(format!("Tile: ${:02X}", tile_num));
                columns[1].monospace(format!("Attr: ${:02X}", attr));
                columns[1].add_space(4.0);
                columns[1].monospace(format!("X-flip: {}", x_flip));
                columns[1].monospace(format!("Y-flip: {}", y_flip));
                columns[1].monospace(format!("Priority: {}", priority));
                if ppu.cgb {
                    columns[1].monospace(format!("Palette: OBJ{}", attr & 0x07));
                    columns[1].monospace(format!("Bank: {}", if attr & 0x08 != 0 { 1 } else { 0 }));
                } else {
                    columns[1].monospace(format!(
                        "Palette: OBP{}",
                        if attr & 0x10 != 0 { 1 } else { 0 }
                    ));
                }
            }
        });
    }

    fn draw_palettes_tab(&mut self, ui: &mut egui::Ui, ppu: &ui::snapshot::PpuSnapshot) {
        const DMG_COLORS: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];
        let bg_pals = if ppu.cgb { 8 } else { 1 };
        let ob_pals = if ppu.cgb { 8 } else { 2 };

        ui.columns(3, |columns| {
            columns[0].heading("BG Palettes");
            for pal in 0..bg_pals {
                columns[0].horizontal(|ui| {
                    ui.label(format!("{}:", pal));
                    for col in 0..4 {
                        let rgb = if ppu.cgb {
                            ppu.cgb_bg_colors[pal][col]
                        } else {
                            let shade = (ppu.bgp >> (col * 2)) & 0x03;
                            DMG_COLORS[shade as usize]
                        };
                        let r = ((rgb >> 16) & 0xFF) as u8;
                        let g = ((rgb >> 8) & 0xFF) as u8;
                        let b = (rgb & 0xFF) as u8;
                        let color = egui::Color32::from_rgb(r, g, b);

                        let selected = self.vram_viewer.palette_sel_is_bg
                            && self.vram_viewer.palette_sel_pal as usize == pal
                            && self.vram_viewer.palette_sel_col as usize == col;

                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                        if response.clicked() {
                            self.vram_viewer.palette_sel_is_bg = true;
                            self.vram_viewer.palette_sel_pal = pal as u8;
                            self.vram_viewer.palette_sel_col = col as u8;
                        }
                        ui.painter().rect_filled(rect, 0.0, color);
                        let stroke_color = if selected {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::GRAY
                        };
                        ui.painter()
                            .rect_stroke(rect, 0.0, egui::Stroke::new(1.0, stroke_color));
                    }
                });
            }

            columns[1].heading("OBJ Palettes");
            for pal in 0..ob_pals {
                columns[1].horizontal(|ui| {
                    ui.label(format!("{}:", pal));
                    for col in 0..4 {
                        let rgb = if ppu.cgb {
                            ppu.cgb_ob_colors[pal][col]
                        } else {
                            let obp = if pal == 0 { ppu.obp0 } else { ppu.obp1 };
                            let shade = (obp >> (col * 2)) & 0x03;
                            DMG_COLORS[shade as usize]
                        };
                        let r = ((rgb >> 16) & 0xFF) as u8;
                        let g = ((rgb >> 8) & 0xFF) as u8;
                        let b = (rgb & 0xFF) as u8;
                        let color = egui::Color32::from_rgb(r, g, b);

                        let selected = !self.vram_viewer.palette_sel_is_bg
                            && self.vram_viewer.palette_sel_pal as usize == pal
                            && self.vram_viewer.palette_sel_col as usize == col;

                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                        if response.clicked() {
                            self.vram_viewer.palette_sel_is_bg = false;
                            self.vram_viewer.palette_sel_pal = pal as u8;
                            self.vram_viewer.palette_sel_col = col as u8;
                        }
                        ui.painter().rect_filled(rect, 0.0, color);
                        let stroke_color = if selected {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::GRAY
                        };
                        ui.painter()
                            .rect_stroke(rect, 0.0, egui::Stroke::new(1.0, stroke_color));
                    }
                });
            }

            columns[2].heading("Selected Color");
            let is_bg = self.vram_viewer.palette_sel_is_bg;
            let pal = self.vram_viewer.palette_sel_pal as usize;
            let col = self.vram_viewer.palette_sel_col as usize;

            let rgb = if is_bg {
                if ppu.cgb {
                    ppu.cgb_bg_colors.get(pal).and_then(|p| p.get(col)).copied()
                } else {
                    let shade = (ppu.bgp >> (col * 2)) & 0x03;
                    Some(DMG_COLORS[shade as usize])
                }
            } else if ppu.cgb {
                ppu.cgb_ob_colors.get(pal).and_then(|p| p.get(col)).copied()
            } else {
                let obp = if pal == 0 { ppu.obp0 } else { ppu.obp1 };
                let shade = (obp >> (col * 2)) & 0x03;
                Some(DMG_COLORS[shade as usize])
            };

            if let Some(rgb) = rgb {
                let r = ((rgb >> 16) & 0xFF) as u8;
                let g = ((rgb >> 8) & 0xFF) as u8;
                let b = (rgb & 0xFF) as u8;
                let color = egui::Color32::from_rgb(r, g, b);

                let (rect, _) =
                    columns[2].allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
                columns[2].painter().rect_filled(rect, 0.0, color);
                columns[2].painter().rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.0, egui::Color32::WHITE),
                );

                let r5 = (r >> 3) as u16;
                let g5 = (g >> 3) as u16;
                let b5 = (b >> 3) as u16;
                let word = r5 | (g5 << 5) | (b5 << 10);

                columns[2].add_space(4.0);
                columns[2].monospace(format!(
                    "{} Pal {} Col {}",
                    if is_bg { "BG" } else { "OBJ" },
                    pal,
                    col
                ));
                columns[2].monospace(format!("RGB: ({}, {}, {})", r, g, b));
                columns[2].monospace(format!("GBC: ${:04X}", word));
            }
        });
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

    let cart: Option<Cartridge> = rom_path
        .as_ref()
        .and_then(|p| match Cartridge::from_file(p) {
            Ok(cart) => Some(cart),
            Err(e) => {
                error!("Failed to load ROM: {e}");
                None
            }
        });

    if headless && cart.is_none() {
        error!("No ROM supplied (required for --headless)");
        std::process::exit(1);
    }

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
        enum Limit {
            Frames(usize),
            Seconds(u64),
        }

        let limit = if let Some(s) = args.seconds {
            Limit::Seconds(s)
        } else {
            Limit::Frames(args.frames.unwrap_or(600))
        };

        match limit {
            Limit::Frames(n) => {
                info!("Running headless for {n} frames");
                for _ in 0..n {
                    gb.mmu.ppu.clear_frame_flag();
                    while !gb.mmu.ppu.frame_ready() {
                        gb.cpu.step(&mut gb.mmu);
                    }
                }
            }
            Limit::Seconds(s) => {
                let target_frames = (s as f64 * GB_FPS).ceil() as usize;
                info!("Running headless for {s} seconds (~{target_frames} frames)");
                for _ in 0..target_frames {
                    gb.mmu.ppu.clear_frame_flag();
                    while !gb.mmu.ppu.frame_ready() {
                        gb.cpu.step(&mut gb.mmu);
                    }
                }
            }
        }

        info!("Headless run complete");
        return;
    }

    let keybinds_path = args
        .keybinds
        .clone()
        .unwrap_or_else(keybinds::default_keybinds_path);
    let keybinds = KeyBindings::load_from_file(&keybinds_path);

    if args.mobile {
        let config_path = args.mobile_config.clone().unwrap_or_else(|| {
            #[cfg(target_os = "windows")]
            {
                if let Some(appdata) = std::env::var_os("APPDATA") {
                    return std::path::PathBuf::from(appdata)
                        .join("vibeemu")
                        .join("mobile.config");
                }
            }

            if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
                return std::path::PathBuf::from(xdg)
                    .join("vibeemu")
                    .join("mobile.config");
            }

            if let Some(home) = std::env::var_os("HOME") {
                return std::path::PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("vibeemu")
                    .join("mobile.config");
            }

            std::path::PathBuf::from("mobile.config")
        });

        if let Some(parent) = config_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match MobileAdapter::new_std(config_path) {
            Ok(mut adapter) => {
                let dns1 = args.mobile_dns1.as_ref().and_then(|dns| {
                    dns.parse::<std::net::IpAddr>().ok().map(|ip| match ip {
                        std::net::IpAddr::V4(v4) => MobileAddr::V4 {
                            host: v4.octets(),
                            port: 53,
                        },
                        std::net::IpAddr::V6(v6) => MobileAddr::V6 {
                            host: v6.octets(),
                            port: 53,
                        },
                    })
                });

                let dns2 = args.mobile_dns2.as_ref().and_then(|dns| {
                    dns.parse::<std::net::IpAddr>().ok().map(|ip| match ip {
                        std::net::IpAddr::V4(v4) => MobileAddr::V4 {
                            host: v4.octets(),
                            port: 53,
                        },
                        std::net::IpAddr::V6(v6) => MobileAddr::V6 {
                            host: v6.octets(),
                            port: 53,
                        },
                    })
                });

                let config = MobileConfig {
                    device: args.mobile_device.into(),
                    unmetered: args.mobile_unmetered,
                    dns1: dns1.unwrap_or_default(),
                    dns2: dns2.unwrap_or_default(),
                    p2p_port: args.mobile_p2p_port,
                    relay: MobileAddr::None,
                    relay_token: None,
                };

                if let Err(e) = adapter.apply_config(&config) {
                    warn!("Failed to apply mobile adapter config: {e}");
                }

                if let Err(e) = adapter.start() {
                    warn!("Failed to start mobile adapter: {e}");
                } else {
                    info!("Mobile Adapter enabled");
                    let adapter = Arc::new(Mutex::new(adapter));
                    let link_port = MobileLinkPort::new(Arc::clone(&adapter));
                    gb.mmu.serial.connect(Box::new(link_port));
                }
            }
            Err(e) => {
                warn!("Failed to create mobile adapter: {e}");
            }
        }
    }

    let gb = Arc::new(Mutex::new(gb));

    let (to_emu_tx, to_emu_rx) = mpsc::channel();
    let (from_emu_frame_tx, from_emu_frame_rx) = cb::bounded(3);
    let (frame_pool_tx, frame_pool_rx) = cb::bounded::<Vec<u32>>(4);
    for _ in 0..4 {
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

    let scale = DEFAULT_WINDOW_SCALE as f32;
    let initial_size = [
        GB_WIDTH * scale,
        GB_HEIGHT * scale + MENU_BAR_HEIGHT + STATUS_BAR_HEIGHT,
    ];
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("vibeEmu")
            .with_inner_size(initial_size)
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
                keybinds_path,
                emulation_mode,
            )))
        }),
    ) {
        error!("eframe error: {e}");
    }
}
