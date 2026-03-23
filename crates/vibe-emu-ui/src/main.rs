#![allow(dead_code)]

mod audio;
mod keybinds;
mod network_link;
mod ui;
mod ui_config;

use clap::{Parser, ValueEnum};
use cpal::traits::StreamTrait;
use eframe::{egui, egui_wgpu, wgpu};
use log::{debug, error, info, warn};
use rfd::FileDialog;
use std::io::{BufWriter, Cursor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use vibe_emu_core::serial::{LinkPort, NullLinkPort};
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision, Model},
};
use vibe_emu_mobile::{
    MobileAdapter, MobileAdapterDevice, MobileAddr, MobileConfig, MobileLinkPort,
};

#[cfg(not(target_os = "android"))]
use gilrs::{Axis as GamepadAxis, Button as GamepadButton, GamepadId, Gilrs};

use crossbeam_channel as cb;
use keybinds::KeyBindings;
use network_link::{LinkCommand, LinkEvent, NetworkLinkPort};
use ui::debugger::{BreakpointSpec, DebuggerPauseReason, DebuggerState};
use ui::snapshot::UiSnapshot;
use ui_config::{
    AxisFilter, DisplayEffect, EmulationMode, SerialPeripheralKind, UiConfig, VideoFilterConfig,
    WindowSize,
};

const DEFAULT_WINDOW_SCALE: u32 = 2;
const MAX_WINDOW_SCALE: usize = 6;
const MAX_RECENT_ROMS: usize = 10;
const GB_WIDTH: f32 = 160.0;
const GB_HEIGHT: f32 = 144.0;
const MENU_BAR_HEIGHT: f32 = 24.0;
const STATUS_BAR_HEIGHT: f32 = 24.0;
const GB_FPS: f64 = 59.7275;
const FRAME_TIME: Duration = Duration::from_nanos((1e9_f64 / GB_FPS) as u64);
const FF_MULT: f32 = 4.0;
const APP_NAME: &str = "vibeEmu";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_LICENSE: &str = env!("CARGO_PKG_LICENSE");
const APP_GITHUB_URL: &str = "https://github.com/vulcandth/vibeEmu";
const APP_SHORT_DESCRIPTION: &str =
    "Game Boy and Game Boy Color emulator written in Rust with an egui/eframe desktop frontend.";

#[cfg(not(target_os = "android"))]
struct GamepadInput {
    gilrs: Gilrs,
    active: Option<GamepadId>,
    axis_deadzone: f32,
}

#[cfg(not(target_os = "android"))]
impl GamepadInput {
    fn try_new() -> Option<Self> {
        let gilrs = Gilrs::new().ok()?;
        Some(Self {
            gilrs,
            active: None,
            axis_deadzone: 0.45,
        })
    }

    fn sample(&mut self) -> (u8, bool) {
        while let Some(ev) = self.gilrs.next_event() {
            self.active = Some(ev.id);
        }

        let active = self
            .active
            .filter(|&id| self.gilrs.gamepad(id).is_connected())
            .or_else(|| {
                self.gilrs
                    .gamepads()
                    .find(|(_, gp)| gp.is_connected())
                    .map(|(id, _)| id)
            });

        let Some(id) = active else {
            return (0xFF, false);
        };

        let gp = self.gilrs.gamepad(id);

        let mut state = 0xFFu8;

        let pressed =
            |btn: GamepadButton| gp.button_data(btn).map(|d| d.is_pressed()).unwrap_or(false);

        if pressed(GamepadButton::DPadRight) {
            state &= !0x01;
        }
        if pressed(GamepadButton::DPadLeft) {
            state &= !0x02;
        }
        if pressed(GamepadButton::DPadUp) {
            state &= !0x04;
        }
        if pressed(GamepadButton::DPadDown) {
            state &= !0x08;
        }

        if pressed(GamepadButton::South) {
            state &= !0x10;
        }
        if pressed(GamepadButton::East) {
            state &= !0x20;
        }
        if pressed(GamepadButton::Select) {
            state &= !0x40;
        }
        if pressed(GamepadButton::Start) {
            state &= !0x80;
        }

        if let Some(x) = gp.axis_data(GamepadAxis::LeftStickX).map(|d| d.value()) {
            if x > self.axis_deadzone {
                state &= !0x01;
            } else if x < -self.axis_deadzone {
                state &= !0x02;
            }
        }
        if let Some(y) = gp.axis_data(GamepadAxis::LeftStickY).map(|d| d.value()) {
            if y < -self.axis_deadzone {
                state &= !0x04;
            } else if y > self.axis_deadzone {
                state &= !0x08;
            }
        }

        let fast_forward =
            pressed(GamepadButton::RightTrigger) || pressed(GamepadButton::RightTrigger2);

        (state, fast_forward)
    }
}

use std::sync::LazyLock;
static VIEWPORT_DEBUGGER: LazyLock<egui::ViewportId> =
    LazyLock::new(|| egui::ViewportId::from_hash_of("debugger"));
static VIEWPORT_VRAM_VIEWER: LazyLock<egui::ViewportId> =
    LazyLock::new(|| egui::ViewportId::from_hash_of("vram_viewer"));
static VIEWPORT_WATCHPOINTS: LazyLock<egui::ViewportId> =
    LazyLock::new(|| egui::ViewportId::from_hash_of("watchpoints"));
static VIEWPORT_OPTIONS: LazyLock<egui::ViewportId> =
    LazyLock::new(|| egui::ViewportId::from_hash_of("options"));
static VIEWPORT_ABOUT: LazyLock<egui::ViewportId> =
    LazyLock::new(|| egui::ViewportId::from_hash_of("about"));

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
    LinkCable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum LinkCableState {
    #[default]
    Disconnected,
    Listening,
    Connecting,
    Connected,
}

#[derive(Clone)]
struct LoadConfig {
    emulation_mode: EmulationMode,
    dmg_neutral: bool,
    bootrom_override: Option<Vec<u8>>,
    dmg_bootrom_path: Option<std::path::PathBuf>,
    cgb_bootrom_path: Option<std::path::PathBuf>,
}

fn configured_bootrom_data(load_config: &LoadConfig, cgb_mode: bool) -> Option<Vec<u8>> {
    if let Some(data) = &load_config.bootrom_override {
        return Some(data.clone());
    }

    let configured_path = if cgb_mode {
        load_config.cgb_bootrom_path.as_ref()
    } else {
        load_config.dmg_bootrom_path.as_ref()
    }?;

    match std::fs::read(configured_path) {
        Ok(data) => Some(data),
        Err(e) => {
            warn!(
                "Failed to read {} boot ROM {}: {e}",
                if cgb_mode { "CGB" } else { "DMG" },
                configured_path.display()
            );
            None
        }
    }
}

#[derive(Clone, Copy)]
struct Speed {
    factor: f32,
    fast: bool,
}

enum EmuCommand {
    SetPaused(bool),
    Reset,
    SetSpeed(Speed),
    UpdateInput(u8),
    UpdateBreakpoints(Vec<ui::debugger::BreakpointSpec>),
    SetRegister { reg: RegisterId, value: u16 },
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegisterId {
    AF,
    BC,
    DE,
    HL,
    SP,
    PC,
}

enum EmuEvent {
    Frame { frame: Vec<u32>, frame_index: u64 },
    BreakpointHit { bank: u8, addr: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RebindTarget {
    Joypad(u8),
    Pause,
    FastForward,
    Screenshot,
    Quit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OptionsTab {
    #[default]
    Keybinds,
    Emulation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum VideoFilterPreset {
    #[default]
    CurrentMethod,
    HorizontalBlur,
    Bilinear,
    Scanlines,
    LcdGrid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum VramTab {
    #[default]
    BgMap,
    Tiles,
    Oam,
    Palettes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BgMapSelect {
    #[default]
    Auto,
    Map9800,
    Map9C00,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TileDataSelect {
    #[default]
    Auto,
    Addr8800,
    Addr8000,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GuessedPalette {
    Bg(usize),
    Obj(usize),
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

    // BG Map tab options
    bg_last_frame: u64,
    bg_show_grid: bool,
    bg_show_viewport: bool,
    bg_map_select: BgMapSelect,
    bg_tile_data_select: TileDataSelect,
    bg_selected_tile: Option<(u8, u8)>,
    bg_tile_preview_tex: Option<egui::TextureHandle>,
    bg_tile_preview_buf: Vec<u8>,

    // Tiles tab options
    tiles_last_frame: u64,
    tiles_show_grid: bool,
    tiles_show_paletted: bool,
    tiles_selected: Option<(u8, u16)>,
    tiles_preview_tex: Option<egui::TextureHandle>,
    tiles_preview_buf: Vec<u8>,

    // OAM tab options
    oam_last_frame: u64,
    oam_screen_tex: Option<egui::TextureHandle>,
    oam_screen_buf: Vec<u8>,
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
            bg_last_frame: 0,
            bg_show_grid: true,
            bg_show_viewport: true,
            bg_map_select: BgMapSelect::Auto,
            bg_tile_data_select: TileDataSelect::Auto,
            bg_selected_tile: None,
            bg_tile_preview_tex: None,
            bg_tile_preview_buf: vec![0; 8 * 8 * 4],
            tiles_last_frame: 0,
            tiles_show_grid: true,
            tiles_show_paletted: true,
            tiles_selected: None,
            tiles_preview_tex: None,
            tiles_preview_buf: vec![0; 8 * 8 * 4],
            oam_last_frame: 0,
            oam_screen_tex: None,
            oam_screen_buf: vec![0; 160 * 144 * 4],
        }
    }
}

struct EmuThreadChannels {
    rx: mpsc::Receiver<EmuCommand>,
    frame_tx: cb::Sender<EmuEvent>,
    frame_pool_tx: cb::Sender<Vec<u32>>,
    frame_pool_rx: cb::Receiver<Vec<u32>>,
}

#[allow(clippy::too_many_arguments)]
fn run_emulator_thread(
    gb: Arc<Mutex<GameBoy>>,
    mut speed: Speed,
    initial_paused: bool,
    channels: EmuThreadChannels,
    external_clock_pending: Arc<network_link::ExternalClockPending>,
    slave_ready: Arc<network_link::SlaveReadyState>,
    link_timestamp: Arc<std::sync::atomic::AtomicU32>,
    link_doublespeed: Arc<std::sync::atomic::AtomicBool>,
) {
    use std::collections::HashSet;

    let EmuThreadChannels {
        rx,
        frame_tx,
        frame_pool_tx: _,
        frame_pool_rx,
    } = channels;

    let mut paused = initial_paused;
    let mut frame_count = 0u64;
    let mut next_frame = Instant::now() + FRAME_TIME;
    let mut breakpoints: HashSet<(u8, u16)> = HashSet::new();
    let mut cumulative_bgb_timestamp: u32 = 0;

    loop {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                EmuCommand::SetPaused(p) => {
                    debug!("[emu] SetPaused({p})");
                    paused = p;
                    next_frame = Instant::now() + FRAME_TIME;
                }
                EmuCommand::Reset => {
                    info!("[reset] emu-thread reset start");
                    if let Ok(mut gb) = gb.lock() {
                        if gb.mmu.boot_rom.is_some() {
                            gb.reset_power_on();
                        } else {
                            gb.reset();
                        }
                        next_frame = Instant::now() + FRAME_TIME;
                        info!("[reset] emu-thread reset complete");
                    } else {
                        error!("[reset] emu-thread failed to lock gb for reset");
                    }
                }
                EmuCommand::SetSpeed(s) => {
                    speed = s;
                }
                EmuCommand::UpdateInput(input) => {
                    if let Ok(mut gb) = gb.lock() {
                        gb.mmu.input.set_state(input);
                    }
                }
                EmuCommand::UpdateBreakpoints(bps) => {
                    breakpoints.clear();
                    for bp in bps {
                        breakpoints.insert((bp.bank, bp.addr));
                    }
                }
                EmuCommand::SetRegister { reg, value } => {
                    if let Ok(mut gb) = gb.lock() {
                        match reg {
                            RegisterId::AF => {
                                gb.cpu.a = (value >> 8) as u8;
                                gb.cpu.f = (value & 0xF0) as u8;
                            }
                            RegisterId::BC => {
                                gb.cpu.b = (value >> 8) as u8;
                                gb.cpu.c = (value & 0xFF) as u8;
                            }
                            RegisterId::DE => {
                                gb.cpu.d = (value >> 8) as u8;
                                gb.cpu.e = (value & 0xFF) as u8;
                            }
                            RegisterId::HL => {
                                gb.cpu.h = (value >> 8) as u8;
                                gb.cpu.l = (value & 0xFF) as u8;
                            }
                            RegisterId::SP => gb.cpu.sp = value,
                            RegisterId::PC => gb.cpu.pc = value,
                        }
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

        let mut bp_hit: Option<(u8, u16)> = None;

        if let Ok(mut gb) = gb.lock() {
            let GameBoy { cpu, mmu, .. } = &mut *gb;
            mmu.ppu.clear_frame_flag();

            let mut ext_clock_active = false;
            let mut ext_clock_bits_remaining: u8 = 0;
            let mut ext_clock_cycle_accum: u32 = 0;
            let mut ext_clock_dot_cycles_per_bit: u32 = 512;

            while !mmu.ppu.frame_ready() {
                // Check breakpoints before executing
                if !breakpoints.is_empty() {
                    let pc = cpu.pc;
                    let bank = if (0x4000..=0x7FFF).contains(&pc) {
                        mmu.cart
                            .as_ref()
                            .map(|c| c.current_rom_bank().min(0xFF) as u8)
                            .unwrap_or(1)
                    } else if pc < 0x4000 {
                        0
                    } else {
                        0xFF
                    };

                    if breakpoints.contains(&(bank, pc)) || breakpoints.contains(&(0xFF, pc)) {
                        bp_hit = Some((bank, pc));
                        break;
                    }
                }

                let prev_dot_div = mmu.dot_div;
                cpu.step(mmu);
                let dot_div_delta = mmu.dot_div.wrapping_sub(prev_dot_div) as u32;

                // Update cumulative timestamp for BGB protocol (2 MiHz = dot_div / 2)
                // BGB expects a 31-bit timestamp that grows continuously, not a wrapped 16-bit value
                cumulative_bgb_timestamp = cumulative_bgb_timestamp.wrapping_add(dot_div_delta / 2);
                let bgb_ts = cumulative_bgb_timestamp & 0x7FFF_FFFF;
                link_timestamp.store(bgb_ts, std::sync::atomic::Ordering::Release);
                link_doublespeed.store(cpu.double_speed, std::sync::atomic::Ordering::Release);

                // Update slave ready state for the network thread
                if let Some(byte) = mmu.serial.pending_external_clock_outgoing() {
                    slave_ready.set_ready(byte);
                } else {
                    slave_ready.set_not_ready();
                }

                // Poll for pending external clock transfers (link cable slave mode)
                // Check if the network received a byte from master AND the game has set up
                // an external clock transfer
                let ext_pending = external_clock_pending.is_pending();
                let has_transfer = mmu.serial.has_external_clock_transfer_pending();
                if ext_pending && has_transfer {
                    if !ext_clock_active {
                        ext_clock_active = true;
                        ext_clock_bits_remaining = 8;
                        ext_clock_cycle_accum = 0;
                        ext_clock_dot_cycles_per_bit =
                            external_clock_pending.dot_cycles_per_bit().max(1);
                        log::debug!(
                            "External clock transfer armed; pacing clock pulses at {} dot cycles/bit",
                            ext_clock_dot_cycles_per_bit
                        );
                    }

                    ext_clock_cycle_accum = ext_clock_cycle_accum.saturating_add(dot_div_delta);

                    while ext_clock_bits_remaining != 0
                        && ext_clock_cycle_accum >= ext_clock_dot_cycles_per_bit
                    {
                        ext_clock_cycle_accum -= ext_clock_dot_cycles_per_bit;
                        mmu.serial.external_clock_pulse(1, &mut mmu.if_reg);

                        if !mmu.serial.has_external_clock_transfer_pending() {
                            break;
                        }

                        ext_clock_bits_remaining = ext_clock_bits_remaining.saturating_sub(1);
                    }

                    if !mmu.serial.has_external_clock_transfer_pending()
                        || ext_clock_bits_remaining == 0
                    {
                        ext_clock_active = false;
                        ext_clock_bits_remaining = 0;
                        ext_clock_cycle_accum = 0;
                        ext_clock_dot_cycles_per_bit = 512;
                        external_clock_pending.clear();
                        log::debug!("External clock transfer completed");
                    }
                } else {
                    // Reset pacing state if the pending flag clears or the transfer disappears.
                    ext_clock_active = false;
                    ext_clock_bits_remaining = 0;
                    ext_clock_cycle_accum = 0;
                    ext_clock_dot_cycles_per_bit = 512;
                }
            }
            frame_buf.copy_from_slice(mmu.ppu.framebuffer());
        }

        if let Some((bank, addr)) = bp_hit {
            paused = true;
            let _ = frame_tx.try_send(EmuEvent::BreakpointHit { bank, addr });
            continue;
        }

        frame_count += 1;
        let _ = frame_tx.try_send(EmuEvent::Frame {
            frame: frame_buf,
            frame_index: frame_count,
        });
    }
}

struct StatusNotification {
    message: String,
    color: egui::Color32,
    expires_at: std::time::Instant,
}

impl StatusNotification {
    fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            color: egui::Color32::GREEN,
            expires_at: std::time::Instant::now() + Duration::from_secs(3),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            color: egui::Color32::RED,
            expires_at: std::time::Instant::now() + Duration::from_secs(5),
        }
    }

    fn is_expired(&self) -> bool {
        std::time::Instant::now() >= self.expires_at
    }
}

struct VibeEmuApp {
    gb: Arc<Mutex<GameBoy>>,
    emu_tx: mpsc::Sender<EmuCommand>,
    frame_rx: cb::Receiver<EmuEvent>,
    frame_pool_tx: cb::Sender<Vec<u32>>,
    _audio_stream: Option<cpal::Stream>,

    sound_enabled: Arc<AtomicBool>,

    ui_config_path: std::path::PathBuf,
    ui_config: UiConfig,

    framebuffer: Vec<u32>,
    texture: Option<egui::TextureHandle>,
    display_horizontal_filter: AxisFilter,
    display_vertical_filter: AxisFilter,
    display_effect: DisplayEffect,
    paused: bool,
    current_rom_path: Option<std::path::PathBuf>,
    keybinds: KeyBindings,
    keybinds_path: std::path::PathBuf,
    joypad_state: u8,
    fast_forward: bool,
    pending_rom_load: Option<std::path::PathBuf>,

    #[cfg(not(target_os = "android"))]
    gamepad: Option<GamepadInput>,

    show_debugger: bool,
    show_vram_viewer: bool,
    show_options: bool,
    show_about: bool,

    // Options window state
    emulation_mode: EmulationMode,
    bootrom_override: Option<Vec<u8>>,
    dmg_bootrom_path: String,
    cgb_bootrom_path: String,
    selected_window_scale: usize,
    current_display_scale: f32,
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
    wp_edit_addr_range: String,
    wp_edit_value: String,
    wp_edit_on_read: bool,
    wp_edit_on_write: bool,
    wp_edit_on_execute: bool,
    wp_edit_on_jump: bool,
    wp_edit_debug_msg: String,
    wp_selected_index: Option<usize>,

    // Register editing popup state
    reg_edit_popup: Option<RegisterId>,
    reg_edit_value: String,

    // Memory viewer state
    mem_viewer_addr: u16,
    mem_viewer_cursor: u16,
    mem_viewer_goto: String,
    mem_viewer_scroll_to: Option<usize>,
    mem_viewer_display_bank: Option<u8>,

    // Mobile Adapter state
    mobile_dns1: String,
    mobile_dns2: String,
    mobile_relay: String,
    mobile_adapter: Option<Arc<Mutex<MobileAdapter>>>,

    // Serial peripheral state
    serial_peripheral: SerialPeripheral,
    link_cable_state: LinkCableState,
    link_cmd_tx: Option<mpsc::Sender<LinkCommand>>,
    link_event_rx: Option<cb::Receiver<LinkEvent>>,
    link_network_state: Option<Arc<Mutex<network_link::NetworkState>>>,
    link_transfer_condvar: Option<Arc<network_link::TransferCondvar>>,
    link_timestamp: Arc<std::sync::atomic::AtomicU32>,
    link_doublespeed: Arc<std::sync::atomic::AtomicBool>,
    link_external_clock_pending: Arc<network_link::ExternalClockPending>,
    link_pending_timestamp: Arc<network_link::PendingTimestamp>,
    link_slave_ready: Arc<network_link::SlaveReadyState>,
    link_host: String,
    link_port: String,

    // Status bar state
    last_fps_update: std::time::Instant,
    frame_count_since_update: u64,
    current_fps: f64,
    status_notification: Option<StatusNotification>,
}

impl VibeEmuApp {
    fn current_video_filter_config(&self) -> VideoFilterConfig {
        VideoFilterConfig {
            horizontal: self.display_horizontal_filter,
            vertical: self.display_vertical_filter,
            effect: self.display_effect,
        }
    }

    fn apply_video_filter_config(&mut self, config: VideoFilterConfig) {
        self.display_horizontal_filter = config.horizontal;
        self.display_vertical_filter = config.vertical;
        self.display_effect = config.effect;
    }

    fn video_filter_preset_label(preset: VideoFilterPreset) -> &'static str {
        match preset {
            VideoFilterPreset::CurrentMethod => "Current method (default)",
            VideoFilterPreset::HorizontalBlur => "Horizontal blur (30fps.net)",
            VideoFilterPreset::Bilinear => "Bilinear",
            VideoFilterPreset::Scanlines => "Scanlines",
            VideoFilterPreset::LcdGrid => "LCD grid",
        }
    }

    fn video_filter_preset_config(preset: VideoFilterPreset) -> VideoFilterConfig {
        match preset {
            VideoFilterPreset::CurrentMethod => VideoFilterConfig {
                horizontal: AxisFilter::Nearest,
                vertical: AxisFilter::Nearest,
                effect: DisplayEffect::None,
            },
            VideoFilterPreset::HorizontalBlur => VideoFilterConfig {
                horizontal: AxisFilter::Linear,
                vertical: AxisFilter::Nearest,
                effect: DisplayEffect::None,
            },
            VideoFilterPreset::Bilinear => VideoFilterConfig {
                horizontal: AxisFilter::Linear,
                vertical: AxisFilter::Linear,
                effect: DisplayEffect::None,
            },
            VideoFilterPreset::Scanlines => VideoFilterConfig {
                horizontal: AxisFilter::Nearest,
                vertical: AxisFilter::Nearest,
                effect: DisplayEffect::Scanlines,
            },
            VideoFilterPreset::LcdGrid => VideoFilterConfig {
                horizontal: AxisFilter::Nearest,
                vertical: AxisFilter::Nearest,
                effect: DisplayEffect::LcdGrid,
            },
        }
    }

    fn video_filter_preset_for_config(config: VideoFilterConfig) -> Option<VideoFilterPreset> {
        [
            VideoFilterPreset::CurrentMethod,
            VideoFilterPreset::HorizontalBlur,
            VideoFilterPreset::Bilinear,
            VideoFilterPreset::Scanlines,
            VideoFilterPreset::LcdGrid,
        ]
        .into_iter()
        .find(|preset| Self::video_filter_preset_config(*preset) == config)
    }

    fn axis_filter_label(filter: AxisFilter) -> &'static str {
        match filter {
            AxisFilter::Nearest => "Nearest",
            AxisFilter::Linear => "Linear",
        }
    }

    fn display_effect_label(effect: DisplayEffect) -> &'static str {
        match effect {
            DisplayEffect::None => "None",
            DisplayEffect::Scanlines => "Scanlines",
            DisplayEffect::LcdGrid => "LCD grid",
        }
    }

    fn blend_two_rgb(a: u32, b: u32) -> u32 {
        let r = (((a >> 16) & 0xFF) + ((b >> 16) & 0xFF)).div_ceil(2) as u8;
        let g = (((a >> 8) & 0xFF) + ((b >> 8) & 0xFF)).div_ceil(2) as u8;
        let blue = ((a & 0xFF) + (b & 0xFF)).div_ceil(2) as u8;
        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(blue)
    }

    fn blend_three_rgb(a: u32, b: u32, c: u32) -> u32 {
        let r =
            ((((a >> 16) & 0xFF) + (((b >> 16) & 0xFF) * 2) + ((c >> 16) & 0xFF) + 2) / 4) as u8;
        let g = ((((a >> 8) & 0xFF) + (((b >> 8) & 0xFF) * 2) + ((c >> 8) & 0xFF) + 2) / 4) as u8;
        let blue = (((a & 0xFF) + ((b & 0xFF) * 2) + (c & 0xFF) + 2) / 4) as u8;
        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(blue)
    }

    fn lerp_rgb(a: u32, b: u32, t_num: usize, t_den: usize) -> u32 {
        if t_den == 0 || t_num == 0 {
            return a;
        }
        if t_num >= t_den {
            return b;
        }

        let t_num_u32 = t_num as u32;
        let t_den_u32 = t_den as u32;
        let inv = t_den_u32 - t_num_u32;

        let r = ((((a >> 16) & 0xFF) * inv + ((b >> 16) & 0xFF) * t_num_u32 + (t_den_u32 / 2))
            / t_den_u32) as u8;
        let g = ((((a >> 8) & 0xFF) * inv + ((b >> 8) & 0xFF) * t_num_u32 + (t_den_u32 / 2))
            / t_den_u32) as u8;
        let blue =
            (((a & 0xFF) * inv + (b & 0xFF) * t_num_u32 + (t_den_u32 / 2)) / t_den_u32) as u8;

        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(blue)
    }

    fn blend_four_rgb(a: u32, b: u32, c: u32, d: u32) -> u32 {
        let r = ((((a >> 16) & 0xFF)
            + ((b >> 16) & 0xFF)
            + ((c >> 16) & 0xFF)
            + ((d >> 16) & 0xFF)
            + 2)
            / 4) as u8;
        let g =
            ((((a >> 8) & 0xFF) + ((b >> 8) & 0xFF) + ((c >> 8) & 0xFF) + ((d >> 8) & 0xFF) + 2)
                / 4) as u8;
        let blue = (((a & 0xFF) + (b & 0xFF) + (c & 0xFF) + (d & 0xFF) + 2) / 4) as u8;
        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(blue)
    }

    fn darken_rgb(rgb: u32, factor: u16) -> u32 {
        let factor = factor.min(256);
        let r = ((((rgb >> 16) & 0xFF) * u32::from(factor) + 128) / 256) as u8;
        let g = ((((rgb >> 8) & 0xFF) * u32::from(factor) + 128) / 256) as u8;
        let blue = (((rgb & 0xFF) * u32::from(factor) + 128) / 256) as u8;
        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(blue)
    }

    fn apply_display_effect(rgb: u32, effect: DisplayEffect, x: usize, y: usize) -> u32 {
        match effect {
            DisplayEffect::None => rgb,
            DisplayEffect::Scanlines => {
                if y % 2 == 1 {
                    Self::darken_rgb(rgb, 205)
                } else {
                    rgb
                }
            }
            DisplayEffect::LcdGrid => {
                let odd_x = x % 2 == 1;
                let odd_y = y % 2 == 1;
                let factor = if odd_x && odd_y {
                    190
                } else if odd_x || odd_y {
                    225
                } else {
                    256
                };
                Self::darken_rgb(rgb, factor)
            }
        }
    }

    fn window_size_to_index(window_size: WindowSize) -> usize {
        match window_size {
            WindowSize::X1 => 0,
            WindowSize::X2 => 1,
            WindowSize::X3 => 2,
            WindowSize::X4 => 3,
            WindowSize::X5 => 4,
            WindowSize::X6 => 5,
            WindowSize::Fullscreen | WindowSize::FullscreenStretched => {
                (DEFAULT_WINDOW_SCALE - 1) as usize
            }
        }
    }

    fn selected_window_size(&self) -> WindowSize {
        match self.selected_window_scale {
            0 => WindowSize::X1,
            1 => WindowSize::X2,
            2 => WindowSize::X3,
            3 => WindowSize::X4,
            4 => WindowSize::X5,
            5 => WindowSize::X6,
            _ => WindowSize::X2,
        }
    }

    fn persist_runtime_settings(&mut self) {
        self.ui_config.window_size = self.selected_window_size();
        self.ui_config.sound_enabled = self.sound_enabled.load(Ordering::Relaxed);
        self.ui_config.emulation_mode = self.emulation_mode;
        self.ui_config.video_filter = VideoFilterConfig {
            horizontal: self.display_horizontal_filter,
            vertical: self.display_vertical_filter,
            effect: self.display_effect,
        };
        self.save_ui_config();
    }

    fn add_recent_rom(&mut self, path: &std::path::Path) {
        self.ui_config.recent_roms.retain(|p| p != path);
        self.ui_config.recent_roms.insert(0, path.to_path_buf());
        self.ui_config.recent_roms.truncate(MAX_RECENT_ROMS);
        self.save_ui_config();
    }

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
        load_config: LoadConfig,
        ui_config_path: std::path::PathBuf,
        ui_config: UiConfig,
        external_clock_pending: Arc<network_link::ExternalClockPending>,
        pending_timestamp: Arc<network_link::PendingTimestamp>,
        local_timestamp: Arc<std::sync::atomic::AtomicU32>,
        link_doublespeed: Arc<std::sync::atomic::AtomicBool>,
        slave_ready: Arc<network_link::SlaveReadyState>,
    ) -> Self {
        let paused = rom_path.is_none();
        if paused {
            let _ = emu_tx.send(EmuCommand::SetPaused(true));
        }

        let sound_enabled = Arc::new(AtomicBool::new(ui_config.sound_enabled));
        let selected_window_scale = Self::window_size_to_index(ui_config.window_size);
        let display_horizontal_filter = ui_config.video_filter.horizontal;
        let display_vertical_filter = ui_config.video_filter.vertical;
        let display_effect = ui_config.video_filter.effect;

        let LoadConfig {
            emulation_mode,
            dmg_neutral: _,
            bootrom_override,
            dmg_bootrom_path,
            cgb_bootrom_path,
        } = load_config;

        let audio_stream = if let Ok(mut gb_lock) = gb.lock() {
            audio::start_stream(&mut gb_lock.mmu.apu, true, sound_enabled.clone())
        } else {
            None
        };

        let mut app = Self {
            gb,
            emu_tx,
            frame_rx,
            frame_pool_tx,
            _audio_stream: audio_stream,

            sound_enabled,

            ui_config_path,
            ui_config,
            framebuffer: vec![0u32; 160 * 144],
            texture: None,
            display_horizontal_filter,
            display_vertical_filter,
            display_effect,
            paused,
            current_rom_path: rom_path,
            keybinds,
            keybinds_path,
            joypad_state: 0xFF,
            fast_forward: false,
            pending_rom_load: None,

            #[cfg(not(target_os = "android"))]
            gamepad: GamepadInput::try_new(),
            show_debugger: false,
            show_vram_viewer: false,
            show_options: false,
            show_about: false,
            emulation_mode,
            bootrom_override,
            dmg_bootrom_path: dmg_bootrom_path
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            cgb_bootrom_path: cgb_bootrom_path
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            selected_window_scale,
            current_display_scale: 1.0,
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
            wp_edit_addr_range: String::new(),
            wp_edit_value: String::new(),
            wp_edit_on_read: false,
            wp_edit_on_write: false,
            wp_edit_on_execute: true,
            wp_edit_on_jump: false,
            wp_edit_debug_msg: String::new(),
            wp_selected_index: None,
            reg_edit_popup: None,
            reg_edit_value: String::new(),
            mem_viewer_addr: 0,
            mem_viewer_cursor: 0,
            mem_viewer_goto: String::new(),
            mem_viewer_scroll_to: None,
            mem_viewer_display_bank: None,
            mobile_dns1: String::new(),
            mobile_dns2: String::new(),
            mobile_relay: String::new(),
            mobile_adapter: None,
            serial_peripheral: SerialPeripheral::None,
            link_cable_state: LinkCableState::Disconnected,
            link_cmd_tx: None,
            link_event_rx: None,
            link_network_state: None,
            link_transfer_condvar: None,
            link_timestamp: local_timestamp,
            link_doublespeed,
            link_external_clock_pending: external_clock_pending,
            link_pending_timestamp: pending_timestamp,
            link_slave_ready: slave_ready,
            link_host: "127.0.0.1".to_string(),
            link_port: "5000".to_string(),
            last_fps_update: std::time::Instant::now(),
            frame_count_since_update: 0,
            current_fps: 0.0,
            status_notification: None,
        };

        app.apply_persisted_serial_settings();

        // Load symbols for ROM if one was provided at startup
        if let Some(ref path) = app.current_rom_path {
            app.debugger_state.load_symbols_for_rom_path(Some(path));
        }

        app
    }

    fn save_ui_config(&self) {
        if let Err(e) = ui_config::save_to_file(&self.ui_config_path, &self.ui_config) {
            log::warn!(
                "Failed to save UI config {}: {e}",
                self.ui_config_path.display()
            );
        }
    }

    fn optional_path_from_input(path_input: &str) -> Option<std::path::PathBuf> {
        let trimmed = path_input.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(trimmed))
        }
    }

    fn persist_bootrom_paths(&mut self) {
        self.ui_config.dmg_bootrom_path = Self::optional_path_from_input(&self.dmg_bootrom_path);
        self.ui_config.cgb_bootrom_path = Self::optional_path_from_input(&self.cgb_bootrom_path);
        self.save_ui_config();
    }

    fn configured_bootrom_data(&self, cgb_mode: bool) -> Option<Vec<u8>> {
        if let Some(data) = &self.bootrom_override {
            return Some(data.clone());
        }

        let configured_path = if cgb_mode {
            Self::optional_path_from_input(&self.cgb_bootrom_path)
        } else {
            Self::optional_path_from_input(&self.dmg_bootrom_path)
        }?;

        match std::fs::read(&configured_path) {
            Ok(data) => Some(data),
            Err(e) => {
                warn!(
                    "Failed to read {} boot ROM {}: {e}",
                    if cgb_mode { "CGB" } else { "DMG" },
                    configured_path.display()
                );
                None
            }
        }
    }

    fn apply_persisted_serial_settings(&mut self) {
        self.mobile_dns1 = self.ui_config.serial.mobile_dns1.clone();
        self.mobile_dns2 = self.ui_config.serial.mobile_dns2.clone();
        self.mobile_relay = self.ui_config.serial.mobile_relay.clone();
        self.link_host = self.ui_config.serial.link_host.clone();
        self.link_port = self.ui_config.serial.link_port.clone();

        self.serial_peripheral = match self.ui_config.serial.peripheral {
            SerialPeripheralKind::None => SerialPeripheral::None,
            SerialPeripheralKind::MobileAdapter => SerialPeripheral::MobileAdapter,
            SerialPeripheralKind::LinkCable => SerialPeripheral::LinkCable,
        };

        match self.serial_peripheral {
            SerialPeripheral::None => {}
            SerialPeripheral::MobileAdapter => self.connect_mobile_adapter(),
            SerialPeripheral::LinkCable => self.init_link_cable_network(),
        }
    }

    fn persist_serial_settings(&mut self) {
        self.ui_config.serial.peripheral = match self.serial_peripheral {
            SerialPeripheral::None => SerialPeripheralKind::None,
            SerialPeripheral::MobileAdapter => SerialPeripheralKind::MobileAdapter,
            SerialPeripheral::LinkCable => SerialPeripheralKind::LinkCable,
        };
        self.ui_config.serial.link_host = self.link_host.clone();
        self.ui_config.serial.link_port = self.link_port.clone();
        self.ui_config.serial.mobile_dns1 = self.mobile_dns1.clone();
        self.ui_config.serial.mobile_dns2 = self.mobile_dns2.clone();
        self.ui_config.serial.mobile_relay = self.mobile_relay.clone();
        self.save_ui_config();
    }

    fn apply_window_scale(&self, ctx: &egui::Context) {
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

    fn draw_emulation_mode_submenu(&mut self, ui: &mut egui::Ui) -> bool {
        let mut close_requested = false;
        let prev_mode = self.emulation_mode;

        if ui
            .radio_value(
                &mut self.emulation_mode,
                EmulationMode::Auto,
                "Auto (detect from ROM)",
            )
            .clicked()
        {
            close_requested = true;
        }
        if ui
            .radio_value(
                &mut self.emulation_mode,
                EmulationMode::ForceDmg,
                "Force DMG",
            )
            .clicked()
        {
            close_requested = true;
        }
        if ui
            .radio_value(
                &mut self.emulation_mode,
                EmulationMode::ForceCgb,
                "Force CGB",
            )
            .clicked()
        {
            close_requested = true;
        }

        if self.emulation_mode != prev_mode {
            self.persist_runtime_settings();
        }

        close_requested
    }

    fn draw_serial_peripheral_submenu(&mut self, ui: &mut egui::Ui) -> bool {
        let mut close_requested = false;
        let prev_peripheral = self.serial_peripheral;

        if ui
            .radio_value(&mut self.serial_peripheral, SerialPeripheral::None, "None")
            .clicked()
        {
            if prev_peripheral != SerialPeripheral::None {
                self.disconnect_serial_peripheral();
            }
            self.persist_serial_settings();
            close_requested = true;
        }
        if ui
            .radio_value(
                &mut self.serial_peripheral,
                SerialPeripheral::MobileAdapter,
                "Mobile Adapter",
            )
            .clicked()
        {
            if prev_peripheral != SerialPeripheral::MobileAdapter {
                self.disconnect_serial_peripheral();
                self.connect_mobile_adapter();
            }
            self.persist_serial_settings();
            close_requested = true;
        }
        if ui
            .radio_value(
                &mut self.serial_peripheral,
                SerialPeripheral::LinkCable,
                "Link Cable (Network)",
            )
            .clicked()
        {
            if prev_peripheral != SerialPeripheral::LinkCable {
                self.disconnect_serial_peripheral();
                self.init_link_cable_network();
            }
            self.persist_serial_settings();
        }

        if self.serial_peripheral == SerialPeripheral::LinkCable {
            ui.separator();
            let state_text = match self.link_cable_state {
                LinkCableState::Disconnected => "Disconnected",
                LinkCableState::Listening => "Listening...",
                LinkCableState::Connecting => "Connecting...",
                LinkCableState::Connected => "✓ Connected",
            };
            ui.label(format!("Status: {}", state_text));

            ui.horizontal(|ui| {
                ui.label("Host:");
                if ui
                    .add(egui::TextEdit::singleline(&mut self.link_host).desired_width(100.0))
                    .changed()
                {
                    self.persist_serial_settings();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Port:");
                if ui
                    .add(egui::TextEdit::singleline(&mut self.link_port).desired_width(60.0))
                    .changed()
                {
                    self.persist_serial_settings();
                }
            });

            ui.horizontal(|ui| {
                let can_act = matches!(self.link_cable_state, LinkCableState::Disconnected);
                if ui
                    .add_enabled(can_act, egui::Button::new("Listen"))
                    .clicked()
                    && let Ok(port) = self.link_port.parse::<u16>()
                    && let Some(ref tx) = self.link_cmd_tx
                {
                    let _ = tx.send(LinkCommand::Listen { port });
                    self.link_cable_state = LinkCableState::Listening;
                }
                if ui
                    .add_enabled(can_act, egui::Button::new("Connect"))
                    .clicked()
                    && let Ok(port) = self.link_port.parse::<u16>()
                    && let Some(ref tx) = self.link_cmd_tx
                {
                    let _ = tx.send(LinkCommand::Connect {
                        host: self.link_host.clone(),
                        port,
                    });
                    self.link_cable_state = LinkCableState::Connecting;
                }
            });

            let can_disconnect = !matches!(self.link_cable_state, LinkCableState::Disconnected);
            if ui
                .add_enabled(can_disconnect, egui::Button::new("Disconnect"))
                .clicked()
                && let Some(ref tx) = self.link_cmd_tx
            {
                let _ = tx.send(LinkCommand::Disconnect);
                self.link_cable_state = LinkCableState::Disconnected;
            }
        }

        if self.serial_peripheral == SerialPeripheral::MobileAdapter {
            ui.separator();
            ui.label("Mobile Adapter Settings:");
            ui.horizontal(|ui| {
                ui.label("DNS 1:");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.mobile_dns1)
                            .desired_width(120.0)
                            .hint_text("8.8.8.8"),
                    )
                    .changed()
                {
                    self.persist_serial_settings();
                }
            });
            ui.horizontal(|ui| {
                ui.label("DNS 2:");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.mobile_dns2)
                            .desired_width(120.0)
                            .hint_text("8.8.4.4"),
                    )
                    .changed()
                {
                    self.persist_serial_settings();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Relay:");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.mobile_relay)
                            .desired_width(180.0)
                            .hint_text("relay.example.com:port"),
                    )
                    .changed()
                {
                    self.persist_serial_settings();
                }
            });
        }

        close_requested
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }

        let mut new_state = 0xFFu8;
        let mut new_fast_forward = false;
        let mut capture_screenshot = false;

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
            capture_screenshot = i.key_pressed(self.keybinds.screenshot_key());
        });

        #[cfg(not(target_os = "android"))]
        if let Some(gamepad) = &mut self.gamepad {
            let (pad_state, pad_ff) = gamepad.sample();
            new_state &= pad_state;
            new_fast_forward |= pad_ff;
        }

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

        if capture_screenshot {
            self.capture_screenshot();
        }
    }

    fn poll_frames(&mut self) {
        while let Ok(evt) = self.frame_rx.try_recv() {
            match evt {
                EmuEvent::Frame {
                    mut frame,
                    frame_index: _,
                } => {
                    std::mem::swap(&mut self.framebuffer, &mut frame);
                    let _ = self.frame_pool_tx.try_send(frame);
                    self.frame_count_since_update += 1;
                }
                EmuEvent::BreakpointHit { bank, addr } => {
                    self.paused = true;
                    self.debugger_state.note_breakpoint_hit(bank, addr);
                    if let Ok(mut gb) = self.gb.lock() {
                        self.debugger_snapshot = Some(UiSnapshot::from_gb(&mut gb, true));
                    }
                    self.debugger_state.request_scroll_to_pc();
                }
            }
        }

        self.poll_link_events();

        let elapsed = self.last_fps_update.elapsed();
        if elapsed >= Duration::from_secs(1) {
            let instant_fps = self.frame_count_since_update as f64 / elapsed.as_secs_f64();
            // Exponential moving average for smoother display
            self.current_fps = self.current_fps * 0.7 + instant_fps * 0.3;
            self.frame_count_since_update = 0;
            self.last_fps_update = std::time::Instant::now();
        }
    }

    fn poll_link_events(&mut self) {
        if let Some(ref rx) = self.link_event_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    LinkEvent::Listening { port } => {
                        info!("Link cable listening on port {}", port);
                        self.link_cable_state = LinkCableState::Listening;
                    }
                    LinkEvent::Connected => {
                        info!("Link cable connected");
                        self.link_cable_state = LinkCableState::Connected;
                        if let Some(net_state) = &self.link_network_state {
                            let link_port = NetworkLinkPort::new_v2(
                                Arc::clone(net_state),
                                Arc::clone(&self.link_timestamp),
                                Arc::clone(&self.link_doublespeed),
                            );
                            if let Ok(mut gb) = self.gb.lock() {
                                gb.mmu.serial.connect(Box::new(link_port));
                            }
                        }
                    }
                    LinkEvent::Disconnected => {
                        info!("Link cable disconnected");
                        self.link_cable_state = LinkCableState::Disconnected;
                        if let Ok(mut gb) = self.gb.lock() {
                            gb.mmu.serial.connect(Box::new(NullLinkPort::default()));
                        }
                    }
                    LinkEvent::RemotePaused => {
                        if !self.paused {
                            info!("Remote emulator paused - pausing local");
                            self.paused = true;
                            let _ = self.emu_tx.send(EmuCommand::SetPaused(true));
                        }
                    }
                    LinkEvent::RemoteResumed => {
                        if self.paused {
                            info!("Remote emulator resumed - resuming local");
                            self.paused = false;
                            let _ = self.emu_tx.send(EmuCommand::SetPaused(false));
                        }
                    }
                    LinkEvent::SlaveTransferReady => {
                        // External clock pending is polled in the emu thread
                    }
                    LinkEvent::Error(msg) => {
                        warn!("Link cable error: {}", msg);
                        self.link_cable_state = LinkCableState::Disconnected;
                    }
                }
            }
        }
    }

    fn disconnect_serial_peripheral(&mut self) {
        if let Some(ref tx) = self.link_cmd_tx {
            let _ = tx.send(LinkCommand::Disconnect);
        }
        self.link_cable_state = LinkCableState::Disconnected;
        self.mobile_adapter = None;

        if let Ok(mut gb) = self.gb.lock() {
            gb.mmu.serial.connect(Box::new(NullLinkPort::default()));
        }
    }

    fn init_link_cable_network(&mut self) {
        if self.link_cmd_tx.is_none() {
            let (cmd_tx, cmd_rx) = mpsc::channel();
            let (event_tx, event_rx) = cb::bounded(16);
            let ext_clock = Arc::clone(&self.link_external_clock_pending);
            let pending_ts = Arc::clone(&self.link_pending_timestamp);
            let local_ts = Arc::clone(&self.link_timestamp);
            let slave_ready = Arc::clone(&self.link_slave_ready);
            let (net_state, condvar, _shared_ts) = network_link::spawn_network_thread(
                cmd_rx,
                event_tx,
                ext_clock,
                pending_ts,
                local_ts,
                slave_ready,
            );
            self.link_cmd_tx = Some(cmd_tx);
            self.link_event_rx = Some(event_rx);
            self.link_network_state = Some(net_state);
            self.link_transfer_condvar = Some(condvar);
        }
    }

    fn connect_mobile_adapter(&mut self) {
        let config_path = {
            #[cfg(target_os = "windows")]
            {
                if let Some(appdata) = std::env::var_os("APPDATA") {
                    std::path::PathBuf::from(appdata)
                        .join("vibeemu")
                        .join("mobile.config")
                } else {
                    std::path::PathBuf::from("mobile.config")
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
                    std::path::PathBuf::from(xdg)
                        .join("vibeemu")
                        .join("mobile.config")
                } else if let Some(home) = std::env::var_os("HOME") {
                    std::path::PathBuf::from(home)
                        .join(".local")
                        .join("share")
                        .join("vibeemu")
                        .join("mobile.config")
                } else {
                    std::path::PathBuf::from("mobile.config")
                }
            }
        };

        if let Some(parent) = config_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match MobileAdapter::new_std(config_path) {
            Ok(mut adapter) => {
                let dns1 = self
                    .mobile_dns1
                    .parse::<std::net::IpAddr>()
                    .ok()
                    .map(|ip| match ip {
                        std::net::IpAddr::V4(v4) => MobileAddr::V4 {
                            host: v4.octets(),
                            port: 53,
                        },
                        std::net::IpAddr::V6(v6) => MobileAddr::V6 {
                            host: v6.octets(),
                            port: 53,
                        },
                    });

                let dns2 = self
                    .mobile_dns2
                    .parse::<std::net::IpAddr>()
                    .ok()
                    .map(|ip| match ip {
                        std::net::IpAddr::V4(v4) => MobileAddr::V4 {
                            host: v4.octets(),
                            port: 53,
                        },
                        std::net::IpAddr::V6(v6) => MobileAddr::V6 {
                            host: v6.octets(),
                            port: 53,
                        },
                    });

                let config = MobileConfig {
                    device: MobileAdapterDevice::Blue,
                    unmetered: false,
                    dns1: dns1.unwrap_or_default(),
                    dns2: dns2.unwrap_or_default(),
                    p2p_port: None,
                    relay: MobileAddr::None,
                    relay_token: None,
                };

                if let Err(e) = adapter.apply_config(&config) {
                    warn!("Failed to apply mobile adapter config: {e}");
                }

                if let Err(e) = adapter.start() {
                    warn!("Failed to start mobile adapter: {e}");
                } else {
                    info!("Mobile Adapter connected");
                    let adapter = Arc::new(Mutex::new(adapter));
                    let link_port = MobileLinkPort::new(Arc::clone(&adapter));
                    self.mobile_adapter = Some(adapter);
                    if let Ok(mut gb) = self.gb.lock() {
                        gb.mmu.serial.connect(Box::new(link_port));
                    }
                }
            }
            Err(e) => {
                warn!("Failed to create mobile adapter: {e}");
            }
        }
    }

    fn update_texture(&mut self, ctx: &egui::Context) {
        const SRC_WIDTH: usize = 160;
        const SRC_HEIGHT: usize = 144;

        let scale = self
            .current_display_scale
            .round()
            .clamp(1.0, MAX_WINDOW_SCALE as f32) as usize;
        let out_width = SRC_WIDTH * scale;
        let out_height = SRC_HEIGHT * scale;

        let horizontal_linear = self.display_horizontal_filter == AxisFilter::Linear && scale > 1;
        let vertical_linear = self.display_vertical_filter == AxisFilter::Linear && scale > 1;

        let mut pixels = Vec::with_capacity(out_width * out_height);
        for out_y in 0..out_height {
            let src_y = out_y / scale;
            let sub_y = out_y % scale;
            let src_y_next = (src_y + 1).min(SRC_HEIGHT - 1);

            for out_x in 0..out_width {
                let src_x = out_x / scale;
                let sub_x = out_x % scale;
                let src_x_next = (src_x + 1).min(SRC_WIDTH - 1);

                let top_left = self.framebuffer[src_y * SRC_WIDTH + src_x];
                let top_right = self.framebuffer[src_y * SRC_WIDTH + src_x_next];
                let bottom_left = self.framebuffer[src_y_next * SRC_WIDTH + src_x];
                let bottom_right = self.framebuffer[src_y_next * SRC_WIDTH + src_x_next];

                let top = if horizontal_linear {
                    Self::lerp_rgb(top_left, top_right, sub_x, scale)
                } else {
                    top_left
                };
                let bottom = if horizontal_linear {
                    Self::lerp_rgb(bottom_left, bottom_right, sub_x, scale)
                } else {
                    bottom_left
                };

                let rgb = if vertical_linear {
                    Self::lerp_rgb(top, bottom, sub_y, scale)
                } else {
                    top
                };

                let rgb = Self::apply_display_effect(rgb, self.display_effect, out_x, out_y);
                let r = ((rgb >> 16) & 0xFF) as u8;
                let g = ((rgb >> 8) & 0xFF) as u8;
                let b = (rgb & 0xFF) as u8;
                pixels.push(egui::Color32::from_rgb(r, g, b));
            }
        }

        let image = egui::ColorImage::new([out_width, out_height], pixels);

        match &mut self.texture {
            Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.texture =
                    Some(ctx.load_texture("gb_framebuffer", image, egui::TextureOptions::NEAREST));
            }
        }
    }

    fn screenshot_output_dir(&self) -> std::path::PathBuf {
        if let Some(rom_path) = &self.current_rom_path
            && let Some(parent) = rom_path.parent()
        {
            return parent.join("screenshots");
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(profile) = std::env::var_os("USERPROFILE") {
                return std::path::PathBuf::from(profile)
                    .join("Pictures")
                    .join("vibeemu")
                    .join("screenshots");
            }
        }

        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join("Pictures")
                .join("vibeemu")
                .join("screenshots");
        }

        std::path::PathBuf::from("screenshots")
    }

    fn screenshot_prefix(&self) -> String {
        self.current_rom_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "vibeemu".to_string())
    }

    fn save_current_frame_screenshot(&self) -> std::io::Result<std::path::PathBuf> {
        const WIDTH: usize = 160;
        const HEIGHT: usize = 144;

        let output_dir = self.screenshot_output_dir();
        std::fs::create_dir_all(&output_dir)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let file_name = format!(
            "{}-{}-{:03}.png",
            self.screenshot_prefix(),
            now.as_secs(),
            now.subsec_millis()
        );
        let output_path = output_dir.join(file_name);

        let file = std::fs::File::create(&output_path)?;
        let writer = BufWriter::new(file);
        let mut encoder = png::Encoder::new(writer, WIDTH as u32, HEIGHT as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut png_writer = encoder
            .write_header()
            .map_err(|e| std::io::Error::other(format!("PNG header write failed: {e}")))?;

        let mut rgb_data = Vec::with_capacity(WIDTH * HEIGHT * 3);
        for &pixel in &self.framebuffer {
            rgb_data.push(((pixel >> 16) & 0xFF) as u8);
            rgb_data.push(((pixel >> 8) & 0xFF) as u8);
            rgb_data.push((pixel & 0xFF) as u8);
        }

        png_writer
            .write_image_data(&rgb_data)
            .map_err(|e| std::io::Error::other(format!("PNG data write failed: {e}")))?;

        Ok(output_path)
    }

    fn capture_screenshot(&mut self) {
        match self.save_current_frame_screenshot() {
            Ok(path) => {
                info!("Saved screenshot to {}", path.display());
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                self.status_notification = Some(StatusNotification::info(format!(
                    "📸 Screenshot saved: {file_name}"
                )));
            }
            Err(e) => {
                warn!("Failed to save screenshot: {e}");
                self.status_notification = Some(StatusNotification::error(format!(
                    "⚠ Failed to save screenshot: {e}"
                )));
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
                let bootrom_data = self.configured_bootrom_data(cgb_mode);
                info!(
                    "Loading ROM: {} (CGB header: {}, mode: {:?} → cgb_mode: {}, bootrom: {})",
                    cart.title,
                    cart.cgb,
                    self.emulation_mode,
                    cgb_mode,
                    if bootrom_data.is_some() {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
                if let Ok(mut gb) = self.gb.lock() {
                    gb.mmu.save_cart_ram();
                    let model = Model::from_cgb_flag(cgb_mode);
                    if bootrom_data.is_some() {
                        *gb = GameBoy::new_power_on(model);
                    } else {
                        *gb = GameBoy::new(model);
                    }
                    if let Some(data) = bootrom_data {
                        gb.mmu.load_boot_rom(data);
                    }
                    gb.mmu.load_cart(cart);
                    self._audio_stream =
                        audio::start_stream(&mut gb.mmu.apu, true, self.sound_enabled.clone());
                }
                self.current_rom_path = Some(path.clone());
                self.add_recent_rom(&path);
                self.debugger_state.load_symbols_for_rom_path(Some(&path));
                self.paused = false;
                let _ = self.emu_tx.send(EmuCommand::SetPaused(false));
                info!("ROM loaded successfully");
            }
            Err(e) => {
                error!("Failed to load ROM: {e}");
                self.status_notification = Some(StatusNotification::error(format!(
                    "⚠ Failed to load ROM: {e}"
                )));
            }
        }
    }

    fn handle_file_drop(&mut self, ctx: &egui::Context) {
        let dropped_paths: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });

        let Some(rom_path) = dropped_paths.into_iter().find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "gb" | "gbc"))
        }) else {
            return;
        };

        self.pending_rom_load = Some(rom_path);
    }
}

impl eframe::App for VibeEmuApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_input(ctx);
        self.handle_file_drop(ctx);
        self.poll_frames();
        self.update_texture(ctx);

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open ROM...").clicked() {
                        if let Some(path) = FileDialog::new()
                            .add_filter("Game Boy ROMs", &["gb", "gbc"])
                            .pick_file()
                        {
                            self.pending_rom_load = Some(path);
                        }
                        ui.close();
                    }

                    egui::containers::menu::SubMenuButton::new("Recent ROMs").ui(ui, |ui| {
                        if self.ui_config.recent_roms.is_empty() {
                            ui.add_enabled(false, egui::Button::new("(No recent ROMs)"));
                            return;
                        }

                        let recent_roms = self.ui_config.recent_roms.clone();
                        for path in recent_roms {
                            let label = path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .filter(|name| !name.is_empty())
                                .map(|name| name.to_string())
                                .unwrap_or_else(|| path.display().to_string());

                            let response =
                                ui.button(label).on_hover_text(path.display().to_string());
                            if response.clicked() {
                                self.pending_rom_load = Some(path);
                                ui.close();
                            }
                        }
                    });

                    if ui
                        .add_enabled(
                            self.current_rom_path.is_some(),
                            egui::Button::new(format!(
                                "Capture Screenshot ({:?})",
                                self.keybinds.screenshot_key()
                            )),
                        )
                        .clicked()
                    {
                        self.capture_screenshot();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        let _ = self.emu_tx.send(EmuCommand::Shutdown);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Emulation", |ui| {
                    let has_rom_loaded = self.current_rom_path.is_some();
                    if ui
                        .add_enabled(
                            has_rom_loaded,
                            egui::Button::new(if self.paused { "Resume" } else { "Pause" }),
                        )
                        .clicked()
                    {
                        self.paused = !self.paused;
                        let _ = self.emu_tx.send(EmuCommand::SetPaused(self.paused));
                        if let Some(ref tx) = self.link_cmd_tx {
                            let cmd = if self.paused {
                                LinkCommand::NotifyPause
                            } else {
                                LinkCommand::NotifyResume
                            };
                            let _ = tx.send(cmd);
                        }
                        ui.close();
                    }
                    if ui
                        .add_enabled(has_rom_loaded, egui::Button::new("Reset"))
                        .clicked()
                    {
                        info!("[reset] requested");
                        let _ = self.emu_tx.send(EmuCommand::Reset);
                        ui.close();
                    }
                    ui.separator();

                    egui::containers::menu::SubMenuButton::new("Mode").ui(ui, |ui| {
                        if self.draw_emulation_mode_submenu(ui) {
                            ui.close();
                        }
                    });
                    egui::containers::menu::SubMenuButton::new("Serial Peripheral").ui(ui, |ui| {
                        if self.draw_serial_peripheral_submenu(ui) {
                            ui.close();
                        }
                    });
                });

                ui.menu_button("Debug", |ui| {
                    #[cfg(debug_assertions)]
                    {
                        if ui.button("Debugger").clicked() {
                            self.show_debugger = !self.show_debugger;
                            if self.show_debugger {
                                self.debugger_state.request_scroll_to_pc();
                            }
                            ui.close();
                        }
                    }
                    if ui.button("VRAM Viewer").clicked() {
                        self.show_vram_viewer = !self.show_vram_viewer;
                        ui.close();
                    }
                    if ui.button("Watchpoints").clicked() {
                        self.show_watchpoints = !self.show_watchpoints;
                        ui.close();
                    }
                });

                ui.menu_button("Options", |ui| {
                    egui::containers::menu::SubMenuButton::new("Window Scale").ui(ui, |ui| {
                        let prev_scale = self.selected_window_scale;
                        for idx in 0..MAX_WINDOW_SCALE {
                            let label = format!("{}x", idx + 1);
                            if ui
                                .radio_value(&mut self.selected_window_scale, idx, label)
                                .clicked()
                            {
                                ui.close();
                            }
                        }
                        if self.selected_window_scale != prev_scale {
                            self.apply_window_scale(ctx);
                            self.persist_runtime_settings();
                        }
                    });

                    let mut enabled = self.sound_enabled.load(Ordering::Relaxed);
                    let response = ui.checkbox(&mut enabled, "Enable sound");
                    if response.changed() {
                        self.sound_enabled.store(enabled, Ordering::Relaxed);
                        self.persist_runtime_settings();
                        ui.close();
                    }

                    if ui.button("Settings...").clicked() {
                        self.show_options = !self.show_options;
                        ui.close();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.show_about = true;
                        ui.close();
                    }
                });
            });
        });

        if let Some(path) = self.pending_rom_load.take() {
            self.load_rom(path);
        }

        // Status bar at the bottom
        // Clear expired status notifications before rendering the status bar
        if self
            .status_notification
            .as_ref()
            .is_some_and(|n| n.is_expired())
        {
            self.status_notification = None;
        }
        let active_notification = self
            .status_notification
            .as_ref()
            .map(|n| (n.message.clone(), n.color));

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
                    match self.serial_peripheral {
                        SerialPeripheral::None => {}
                        SerialPeripheral::MobileAdapter => {
                            ui.separator();
                            ui.colored_label(egui::Color32::LIGHT_BLUE, "📱 Mobile");
                        }
                        SerialPeripheral::LinkCable => {
                            ui.separator();
                            let (text, color) = match self.link_cable_state {
                                LinkCableState::Disconnected => ("🔗 Link", egui::Color32::GRAY),
                                LinkCableState::Listening => {
                                    ("🔗 Listening", egui::Color32::YELLOW)
                                }
                                LinkCableState::Connecting => {
                                    ("🔗 Connecting", egui::Color32::YELLOW)
                                }
                                LinkCableState::Connected => ("🔗 Connected", egui::Color32::GREEN),
                            };
                            ui.colored_label(color, text);
                        }
                    }

                    // Calculate space needed for FPS (approximate)
                    let fps_text = format!("{:.1} FPS", self.current_fps);
                    let fps_reserve = 80.0;

                    let remaining = total_width - ui.min_rect().width() - fps_reserve - 30.0;

                    if remaining > 60.0 {
                        if let Some((msg, color)) = &active_notification {
                            // Timed notification takes precedence over the ROM name
                            ui.separator();
                            ui.add_sized(
                                [remaining.min(400.0), ui.available_height()],
                                egui::Label::new(egui::RichText::new(msg.as_str()).color(*color))
                                    .truncate()
                                    .wrap_mode(egui::TextWrapMode::Truncate),
                            );
                        } else if let Some(path) = &self.current_rom_path
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
                    }

                    // FPS counter (right-aligned)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(fps_text);
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let available = ui.available_size();
                let scale = (available.x / GB_WIDTH)
                    .min(available.y / GB_HEIGHT)
                    .floor()
                    .max(1.0);
                self.current_display_scale = scale;
                let menu_scale = (scale as usize).clamp(1, MAX_WINDOW_SCALE) - 1;
                if self.selected_window_scale != menu_scale {
                    self.selected_window_scale = menu_scale;
                    self.ui_config.window_size = self.selected_window_size();
                }

                if let Some(tex) = &self.texture {
                    let size = egui::vec2(GB_WIDTH * scale, GB_HEIGHT * scale);
                    let offset = (available - size) / 2.0;
                    let rect = egui::Rect::from_min_size(
                        ui.min_rect().min + egui::vec2(offset.x, offset.y),
                        size,
                    );
                    ui.put(rect, egui::Image::new(tex).fit_to_exact_size(size));
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("No ROM loaded. Use File → Open ROM... or drag and drop a ROM file here.");
                    });
                }
            });

        #[cfg(debug_assertions)]
        if self.show_debugger {
            self.draw_debugger_window(ctx);
        }

        #[cfg(not(debug_assertions))]
        {
            self.show_debugger = false;
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

        if self.show_about {
            self.draw_about_window(ctx);
        }

        if !self.paused {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_runtime_settings();
        if let Ok(mut gb) = self.gb.lock() {
            gb.mmu.save_cart_ram();
        }
        let _ = self.emu_tx.send(EmuCommand::Shutdown);
    }
}

impl VibeEmuApp {
    fn draw_about_window(&mut self, ctx: &egui::Context) {
        ctx.show_viewport_immediate(
            *VIEWPORT_ABOUT,
            egui::ViewportBuilder::default()
                .with_title(format!("About {APP_NAME}"))
                .with_inner_size([500.0, 320.0]),
            |ctx, class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.show_about = false;
                }

                match class {
                    egui::ViewportClass::Embedded => {
                        egui::Window::new(format!("About {APP_NAME}")).show(ctx, |ui| {
                            self.draw_about_content(ui);
                        });
                    }
                    _ => {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            self.draw_about_content(ui);
                        });
                    }
                }
            },
        );
    }

    fn draw_about_content(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(format!("{APP_NAME} {APP_VERSION}"));
                ui.label("Game Boy / Game Boy Color emulator");
            });

            ui.add_space(8.0);
            ui.label(APP_SHORT_DESCRIPTION);

            ui.add_space(12.0);
            egui::Grid::new("about_info_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.strong("GitHub:");
                    ui.hyperlink_to(APP_GITHUB_URL, APP_GITHUB_URL);
                    ui.end_row();

                    ui.strong("App license:");
                    ui.label(APP_LICENSE);
                    ui.end_row();

                    ui.strong("License files:");
                    ui.label("See LICENSE and THIRD_PARTY_LICENSES.md included with the app.");
                    ui.end_row();
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label("vibeEmu is distributed under the MIT license.");
            ui.label("Third-party dependency notices are collected in THIRD_PARTY_LICENSES.md.");

            if cfg!(feature = "mobile-bundled") {
                ui.label(
                    "Bundled builds also include vendored libmobile under LGPL-3.0-or-later. See THIRD_PARTY_LICENSES.md for details.",
                );
            }

            ui.add_space(12.0);
            if ui.button("Close").clicked() {
                self.show_about = false;
            }
        });
    }

    fn draw_options_window(&mut self, ctx: &egui::Context) {
        ctx.show_viewport_immediate(
            *VIEWPORT_OPTIONS,
            egui::ViewportBuilder::default()
                .with_title("Options")
                .with_inner_size([400.0, 380.0]),
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

                        ui.label("Screenshot");
                        ui.label(format!("{:?}", self.keybinds.screenshot_key()));
                        if ui.button("Rebind").clicked() {
                            self.rebinding = Some(RebindTarget::Screenshot);
                        }
                        ui.end_row();
                    });
            }
            OptionsTab::Emulation => {
                ui.horizontal(|ui| {
                    ui.label("DMG Boot ROM:");
                    let browse_button_width = 82.0;
                    let path_width =
                        (ui.available_width() - browse_button_width - ui.spacing().item_spacing.x)
                            .max(120.0);
                    if ui
                        .add_sized(
                            [path_width, ui.spacing().interact_size.y],
                            egui::TextEdit::singleline(&mut self.dmg_bootrom_path),
                        )
                        .changed()
                    {
                        self.persist_bootrom_paths();
                    }
                    if ui.button("Browse...").clicked()
                        && let Some(path) = FileDialog::new().pick_file()
                    {
                        self.dmg_bootrom_path = path.to_string_lossy().to_string();
                        self.persist_bootrom_paths();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("CGB Boot ROM:");
                    let browse_button_width = 82.0;
                    let path_width =
                        (ui.available_width() - browse_button_width - ui.spacing().item_spacing.x)
                            .max(120.0);
                    if ui
                        .add_sized(
                            [path_width, ui.spacing().interact_size.y],
                            egui::TextEdit::singleline(&mut self.cgb_bootrom_path),
                        )
                        .changed()
                    {
                        self.persist_bootrom_paths();
                    }
                    if ui.button("Browse...").clicked()
                        && let Some(path) = FileDialog::new().pick_file()
                    {
                        self.cgb_bootrom_path = path.to_string_lossy().to_string();
                        self.persist_bootrom_paths();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label("Display filter");

                let prev_filter_config = self.current_video_filter_config();
                let mut selected_preset = Self::video_filter_preset_for_config(prev_filter_config);

                egui::ComboBox::from_label("Preset")
                    .selected_text(
                        selected_preset
                            .map(Self::video_filter_preset_label)
                            .unwrap_or("Custom"),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut selected_preset,
                            Some(VideoFilterPreset::CurrentMethod),
                            Self::video_filter_preset_label(VideoFilterPreset::CurrentMethod),
                        );
                        ui.selectable_value(
                            &mut selected_preset,
                            Some(VideoFilterPreset::HorizontalBlur),
                            Self::video_filter_preset_label(VideoFilterPreset::HorizontalBlur),
                        );
                        ui.selectable_value(
                            &mut selected_preset,
                            Some(VideoFilterPreset::Bilinear),
                            Self::video_filter_preset_label(VideoFilterPreset::Bilinear),
                        );
                        ui.selectable_value(
                            &mut selected_preset,
                            Some(VideoFilterPreset::Scanlines),
                            Self::video_filter_preset_label(VideoFilterPreset::Scanlines),
                        );
                        ui.selectable_value(
                            &mut selected_preset,
                            Some(VideoFilterPreset::LcdGrid),
                            Self::video_filter_preset_label(VideoFilterPreset::LcdGrid),
                        );
                    });

                ui.small("\"30fps.net\" is the article source name, not the emulation frame rate.");

                if let Some(preset) = selected_preset {
                    self.apply_video_filter_config(Self::video_filter_preset_config(preset));
                }

                egui::ComboBox::from_label("Horizontal sampling")
                    .selected_text(Self::axis_filter_label(self.display_horizontal_filter))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.display_horizontal_filter,
                            AxisFilter::Nearest,
                            Self::axis_filter_label(AxisFilter::Nearest),
                        );
                        ui.selectable_value(
                            &mut self.display_horizontal_filter,
                            AxisFilter::Linear,
                            Self::axis_filter_label(AxisFilter::Linear),
                        );
                    });

                egui::ComboBox::from_label("Vertical sampling")
                    .selected_text(Self::axis_filter_label(self.display_vertical_filter))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.display_vertical_filter,
                            AxisFilter::Nearest,
                            Self::axis_filter_label(AxisFilter::Nearest),
                        );
                        ui.selectable_value(
                            &mut self.display_vertical_filter,
                            AxisFilter::Linear,
                            Self::axis_filter_label(AxisFilter::Linear),
                        );
                    });

                egui::ComboBox::from_label("Screen effect")
                    .selected_text(Self::display_effect_label(self.display_effect))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.display_effect,
                            DisplayEffect::None,
                            Self::display_effect_label(DisplayEffect::None),
                        );
                        ui.selectable_value(
                            &mut self.display_effect,
                            DisplayEffect::Scanlines,
                            Self::display_effect_label(DisplayEffect::Scanlines),
                        );
                        ui.selectable_value(
                            &mut self.display_effect,
                            DisplayEffect::LcdGrid,
                            Self::display_effect_label(DisplayEffect::LcdGrid),
                        );
                    });

                if self.current_video_filter_config() != prev_filter_config {
                    self.persist_runtime_settings();
                }
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
                .with_inner_size([750.0, 550.0]),
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
        // Process pending debugger actions first
        self.process_debugger_actions();

        let Some(snapshot) = self.debugger_snapshot.clone() else {
            ui.label("Unable to access emulator state");
            return;
        };

        self.draw_debugger_toolbar(ui, &snapshot);
        ui.separator();

        // Calculate space for top (disassembly/state) and bottom (memory viewer)
        let available = ui.available_height();
        let status_bar_height = 24.0;
        let mem_viewer_height = ((available - status_bar_height) * 0.35).clamp(120.0, 280.0);
        let top_height = available - mem_viewer_height - status_bar_height - 16.0;

        // Top portion: disassembly and state panes
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), top_height),
            egui::Layout::left_to_right(egui::Align::TOP),
            |ui| {
                ui.columns(2, |columns| {
                    self.draw_disassembly_pane(&mut columns[0], &snapshot);
                    self.draw_state_panes(&mut columns[1], &snapshot);
                });
            },
        );

        ui.separator();

        // Bottom portion: memory viewer
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), mem_viewer_height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                self.draw_memory_viewer(ui, &snapshot);
            },
        );

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

            if paused && ui.button("Run*").on_hover_text("Run (no break)").clicked() {
                self.debugger_state
                    .request_continue_no_break_and_focus_main();
                self.paused = false;
                let _ = self.emu_tx.send(EmuCommand::SetPaused(false));
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
                if ui.button("Run To").on_hover_text("Run to cursor").clicked() {
                    self.debugger_state.request_run_to_cursor();
                }
                if ui
                    .button("Run To*")
                    .on_hover_text("Run to cursor (no break)")
                    .clicked()
                {
                    self.debugger_state.request_run_to_cursor_no_break();
                }
                ui.separator();
                if ui.button("Jump").on_hover_text("Jump to cursor").clicked() {
                    self.debugger_state.request_jump_to_cursor();
                }
                if ui.button("Call").on_hover_text("Call cursor").clicked() {
                    self.debugger_state.request_call_cursor();
                }
                if ui
                    .button("Jump(SP)")
                    .on_hover_text("Jump to address on stack")
                    .clicked()
                {
                    self.debugger_state.request_jump_sp();
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
        self.debugger_state.request_scroll_to_pc();
    }

    fn process_debugger_actions(&mut self) {
        let Some(snapshot) = self.debugger_snapshot.clone() else {
            return;
        };

        // Process step over request
        if let Ok(mut gb) = self.gb.lock() {
            let pc = snapshot.cpu.pc;
            self.debugger_state.handle_step_over_request(
                self.paused,
                pc,
                |addr| gb.mmu.read_byte(addr),
                &snapshot,
            );
        }

        // Process run to cursor request
        self.debugger_state
            .handle_run_to_cursor_request(self.paused);

        // Process step out request
        if let Ok(mut gb) = self.gb.lock() {
            self.debugger_state.handle_step_out_request(
                self.paused,
                snapshot.cpu.sp,
                |addr| gb.mmu.read_byte(addr),
                &snapshot,
            );
        }

        // Process jump to cursor request
        self.debugger_state
            .handle_jump_to_cursor_request(self.paused);

        // Process call cursor request
        self.debugger_state.handle_call_cursor_request(self.paused);

        // Get pending actions (includes request_jump_sp)
        let actions = self.debugger_state.take_actions();

        // Handle run_to: run until we hit the target or a breakpoint
        if let Some(run_to) = actions.request_run_to {
            self.execute_run_to(run_to);
        }

        // Handle jump to addr (change PC directly)
        if let Some(addr) = actions.request_jump_to_cursor
            && let Ok(mut gb) = self.gb.lock()
        {
            gb.cpu.pc = addr;
            self.debugger_snapshot = Some(UiSnapshot::from_gb(&mut gb, true));
            self.debugger_state.request_scroll_to_pc();
        }

        // Handle call addr (push return, then jump)
        if let Some(addr) = actions.request_call_cursor
            && let Ok(mut gb) = self.gb.lock()
        {
            let pc = gb.cpu.pc;
            let sp = gb.cpu.sp.wrapping_sub(2);
            gb.cpu.sp = sp;
            gb.mmu.write_byte(sp, (pc & 0xFF) as u8);
            gb.mmu.write_byte(sp.wrapping_add(1), (pc >> 8) as u8);
            gb.cpu.pc = addr;
            self.debugger_snapshot = Some(UiSnapshot::from_gb(&mut gb, true));
            self.debugger_state.request_scroll_to_pc();
        }

        // Handle jump SP (pop return address)
        if actions.request_jump_sp
            && let Ok(mut gb) = self.gb.lock()
        {
            let sp = gb.cpu.sp;
            let lo = gb.mmu.read_byte(sp);
            let hi = gb.mmu.read_byte(sp.wrapping_add(1));
            let addr = (hi as u16) << 8 | lo as u16;
            gb.cpu.sp = sp.wrapping_add(2);
            gb.cpu.pc = addr;
            self.debugger_snapshot = Some(UiSnapshot::from_gb(&mut gb, true));
            self.debugger_state.request_scroll_to_pc();
        }

        // Sync breakpoints to emulator thread if changed
        if actions.breakpoints_updated {
            let _ = self
                .emu_tx
                .send(EmuCommand::UpdateBreakpoints(actions.breakpoints));
        }
    }

    fn execute_run_to(&mut self, run_to: ui::debugger::DebuggerRunToRequest) {
        let target_bank = run_to.target.bank;
        let target_addr = run_to.target.addr;
        let ignore_breakpoints = run_to.ignore_breakpoints;

        // Run until we hit target or breakpoint (max iterations to prevent infinite loop)
        const MAX_STEPS: u32 = 10_000_000;

        if let Ok(mut gb) = self.gb.lock() {
            for _ in 0..MAX_STEPS {
                let pc = gb.cpu.pc;
                let current_bank = if (0x4000..=0x7FFF).contains(&pc) {
                    gb.mmu
                        .cart
                        .as_ref()
                        .map(|c| c.current_rom_bank().min(0xFF) as u8)
                        .unwrap_or(1)
                } else if pc < 0x4000 {
                    0
                } else {
                    0xFF
                };

                // Check if we hit target
                if pc == target_addr && (current_bank == target_bank || target_bank == 0xFF) {
                    break;
                }

                // Check breakpoints (if not ignoring them)
                if !ignore_breakpoints {
                    let bp_spec = ui::debugger::BreakpointSpec {
                        bank: current_bank,
                        addr: pc,
                    };
                    if self.debugger_state.has_breakpoint(&bp_spec) == Some(true) {
                        self.debugger_state.note_breakpoint_hit(current_bank, pc);
                        break;
                    }
                }

                let GameBoy { cpu, mmu, .. } = &mut *gb;
                cpu.step(mmu);
            }

            self.debugger_snapshot = Some(UiSnapshot::from_gb(&mut gb, true));
        }

        self.debugger_state
            .set_pause_reason(DebuggerPauseReason::Step);
        self.debugger_state.request_scroll_to_pc();
    }

    fn draw_disassembly_pane(&mut self, ui: &mut egui::Ui, snapshot: &UiSnapshot) {
        // Fast instruction length lookup (avoids full disassembly for indexing)
        fn instruction_length(opcode: u8, _get_next: impl FnOnce() -> u8) -> u16 {
            match opcode {
                0xCB => 2,                             // CB prefix always 2 bytes
                0x01 | 0x08 | 0x11 | 0x21 | 0x31 => 3, // LD r16,nn / LD (nn),SP
                0xC2 | 0xC3 | 0xC4 | 0xCA | 0xCC | 0xCD | 0xD2 | 0xD4 | 0xDA | 0xDC => 3, // JP/CALL
                0xEA | 0xFA => 3,                      // LD (nn),A / LD A,(nn)
                0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => 2, // LD r,n
                0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => 2, // ALU A,n
                0x18 | 0x20 | 0x28 | 0x30 | 0x38 => 2, // JR
                0xE0 | 0xF0 => 2,                      // LDH
                0xE8 | 0xF8 => 2,                      // ADD SP,e / LD HL,SP+e
                _ => 1,
            }
        }

        let pc = snapshot.cpu.pc;
        let dbg = &snapshot.debugger;
        let active_bank = dbg.active_rom_bank.min(0xFF) as u8;

        let Some(mem_image) = &dbg.mem_image else {
            ui.label("Memory not available (emulator running)");
            return;
        };

        let mut bp_toggle: Option<BreakpointSpec> = None;
        let mut cursor_click: Option<BreakpointSpec> = None;

        // Build instruction address index with display row tracking
        // Each entry is (addr, display_row) where display_row accounts for labels
        let mut instr_addrs: Vec<u16> = Vec::with_capacity(32768);
        let mut instr_display_rows: Vec<usize> = Vec::with_capacity(32768);
        let mut addr: u16 = 0;
        let mut pc_display_row: Option<usize> = None;
        let mut current_display_row: usize = 0;

        loop {
            let bp_bank = if (0x4000..=0x7FFF).contains(&addr) {
                active_bank
            } else if addr < 0x4000 {
                0
            } else {
                0xFF
            };

            // Check if this address has a label (adds a row)
            if self.debugger_state.first_label_for(bp_bank, addr).is_some() {
                current_display_row += 1;
            }

            if addr == pc {
                pc_display_row = Some(current_display_row);
            }

            instr_addrs.push(addr);
            instr_display_rows.push(current_display_row);
            current_display_row += 1;

            let opcode = mem_image[addr as usize];
            let len = instruction_length(opcode, || {
                mem_image
                    .get(addr.wrapping_add(1) as usize)
                    .copied()
                    .unwrap_or(0)
            });

            let next_addr = addr.wrapping_add(len);
            if next_addr <= addr && addr != 0 {
                break;
            }
            addr = next_addr;
            if addr == 0 {
                break;
            }
        }

        let total_rows = current_display_row;
        let row_height = 16.0;

        // Check if we need to scroll to a specific address
        let scroll_target = self.debugger_state.take_pending_scroll();
        let scroll_to_display_row = scroll_target.and_then(|target| {
            if target == u16::MAX {
                // Scroll to PC
                pc_display_row
            } else {
                // Find display row for target address
                instr_addrs
                    .iter()
                    .position(|&a| a == target)
                    .map(|idx| instr_display_rows[idx])
            }
        });

        // Get available height to center the target row
        let available_height = ui.available_height();

        let mut scroll_area = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .id_salt("disasm_scroll");

        if let Some(display_row) = scroll_to_display_row {
            // Center the target row in the view
            let target_offset = (display_row as f32 * row_height - available_height / 2.0).max(0.0);
            scroll_area = scroll_area.vertical_scroll_offset(target_offset);
        }

        scroll_area.show(ui, |ui| {
            // Get current scroll position
            let scroll_offset = ui.clip_rect().top() - ui.min_rect().top();
            let visible_start_row = (scroll_offset / row_height).floor() as usize;
            let visible_rows = (available_height / row_height).ceil() as usize + 2;
            let visible_end_row = (visible_start_row + visible_rows).min(total_rows);

            // Add spacing for rows before visible area
            if visible_start_row > 0 {
                ui.add_space(visible_start_row as f32 * row_height);
            }

            // Find which instructions to render based on display rows
            let start_instr = instr_display_rows
                .iter()
                .position(|&r| r >= visible_start_row)
                .unwrap_or(0);
            let end_instr = instr_display_rows
                .iter()
                .position(|&r| r >= visible_end_row)
                .unwrap_or(instr_addrs.len());

            for instr_idx in start_instr..end_instr {
                let Some(&addr) = instr_addrs.get(instr_idx) else {
                    continue;
                };

                let bp_bank = if (0x4000..=0x7FFF).contains(&addr) {
                    active_bank
                } else if addr < 0x4000 {
                    0
                } else {
                    0xFF
                };

                // Show label on its own line if present
                if let Some(lbl) = self.debugger_state.first_label_for(bp_bank, addr) {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("{}:", lbl))
                                .color(egui::Color32::from_rgb(180, 180, 255))
                                .monospace(),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                }

                // Decode instruction
                let mem_slice: Vec<u8> = (0..4)
                    .map(|i| {
                        mem_image
                            .get(addr.wrapping_add(i) as usize)
                            .copied()
                            .unwrap_or(0)
                    })
                    .collect();

                let (mut mnemonic, _len, target_addr) = ui::disasm::decode_sm83(&mem_slice, addr);

                // Resolve target address to symbol name
                if let Some(target) = target_addr {
                    let target_bank = if target < 0x4000 {
                        0
                    } else if (0x4000..=0x7FFF).contains(&target) {
                        active_bank
                    } else {
                        0xFF
                    };

                    let sym_name = self
                        .debugger_state
                        .first_label_for(target_bank, target)
                        .or_else(|| self.debugger_state.first_label_for(0, target));

                    if let Some(sym_name) = sym_name {
                        let hex_target = format!("${target:04X}");
                        mnemonic = mnemonic.replace(&hex_target, sym_name);
                    }
                }

                let bp_spec = BreakpointSpec {
                    bank: bp_bank,
                    addr,
                };
                let bp_enabled = self.debugger_state.has_breakpoint(&bp_spec);
                let is_cursor = self.debugger_state.cursor() == Some(bp_spec);
                let is_pc = addr == pc;

                let bg_color = if is_pc {
                    Some(egui::Color32::from_rgb(60, 60, 100))
                } else if is_cursor {
                    Some(egui::Color32::from_rgb(40, 60, 80))
                } else {
                    None
                };

                let text_color = if is_pc {
                    egui::Color32::YELLOW
                } else if is_cursor {
                    egui::Color32::LIGHT_BLUE
                } else {
                    ui.style().visuals.text_color()
                };

                let display_bank = if addr < 0x4000 {
                    0
                } else if (0x4000..=0x7FFF).contains(&addr) {
                    active_bank
                } else {
                    0xFF
                };

                let addr_text = if display_bank == 0xFF {
                    format!("  {:04X}", addr)
                } else {
                    format!("{:02X}:{:04X}", display_bank, addr)
                };

                let pc_marker = if is_pc { "►" } else { " " };
                let line = format!("{} {}  {:<20}", pc_marker, addr_text, mnemonic);

                ui.horizontal(|ui| {
                    let bp_symbol = match bp_enabled {
                        Some(true) => "●",
                        Some(false) => "○",
                        None => " ",
                    };
                    let bp_color = match bp_enabled {
                        Some(true) => egui::Color32::RED,
                        Some(false) => egui::Color32::DARK_RED,
                        None => egui::Color32::TRANSPARENT,
                    };

                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(bp_symbol).color(bp_color))
                                .frame(false)
                                .min_size(egui::vec2(12.0, 0.0)),
                        )
                        .clicked()
                    {
                        bp_toggle = Some(bp_spec);
                    }

                    let label =
                        egui::Label::new(egui::RichText::new(&line).color(text_color).monospace())
                            .sense(egui::Sense::click());

                    let resp = if let Some(bg) = bg_color {
                        ui.scope(|ui| {
                            let rect = ui.available_rect_before_wrap();
                            ui.painter().rect_filled(rect, 0.0, bg);
                            ui.add(label)
                        })
                        .inner
                    } else {
                        ui.add(label)
                    };

                    if resp.clicked() {
                        cursor_click = Some(bp_spec);
                    }
                });
            }

            // Add spacing for rows after visible area
            let remaining_rows = total_rows.saturating_sub(visible_end_row);
            if remaining_rows > 0 {
                ui.add_space(remaining_rows as f32 * row_height);
            }
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
        let ppu = &snapshot.ppu;
        let dbg = &snapshot.debugger;

        let af = ((cpu.a as u16) << 8) | cpu.f as u16;
        let bc = ((cpu.b as u16) << 8) | cpu.c as u16;
        let de = ((cpu.d as u16) << 8) | cpu.e as u16;
        let hl = ((cpu.h as u16) << 8) | cpu.l as u16;

        // Helper macro for editable register labels
        let mut pending_edit: Option<(RegisterId, u16)> = None;

        // BGB-style compact register display with right-click editing
        egui::Grid::new("cpu_regs_bgb")
            .num_columns(4)
            .spacing([12.0, 2.0])
            .show(ui, |ui| {
                // AF register (right-click to edit)
                let af_resp = ui.add(
                    egui::Label::new(egui::RichText::new(format!("af= {:04X}", af)).monospace())
                        .sense(egui::Sense::click()),
                );
                if af_resp.secondary_clicked() {
                    self.reg_edit_popup = Some(RegisterId::AF);
                    self.reg_edit_value = format!("{:04X}", af);
                }
                ui.monospace(format!("lcdc={:02X}", ppu.lcdc));
                ui.end_row();

                // BC register
                let bc_resp = ui.add(
                    egui::Label::new(egui::RichText::new(format!("bc= {:04X}", bc)).monospace())
                        .sense(egui::Sense::click()),
                );
                if bc_resp.secondary_clicked() {
                    self.reg_edit_popup = Some(RegisterId::BC);
                    self.reg_edit_value = format!("{:04X}", bc);
                }
                ui.monospace(format!("stat={:02X}", ppu.stat));
                ui.end_row();

                // DE register
                let de_resp = ui.add(
                    egui::Label::new(egui::RichText::new(format!("de= {:04X}", de)).monospace())
                        .sense(egui::Sense::click()),
                );
                if de_resp.secondary_clicked() {
                    self.reg_edit_popup = Some(RegisterId::DE);
                    self.reg_edit_value = format!("{:04X}", de);
                }
                ui.monospace(format!("ly=  {:02X}", ppu.ly));
                ui.end_row();

                // HL register
                let hl_resp = ui.add(
                    egui::Label::new(egui::RichText::new(format!("hl= {:04X}", hl)).monospace())
                        .sense(egui::Sense::click()),
                );
                if hl_resp.secondary_clicked() {
                    self.reg_edit_popup = Some(RegisterId::HL);
                    self.reg_edit_value = format!("{:04X}", hl);
                }
                ui.monospace(format!("ie=  {:02X}", dbg.ie_reg));
                ui.end_row();

                // SP register
                let sp_resp = ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("sp= {:04X}", cpu.sp)).monospace(),
                    )
                    .sense(egui::Sense::click()),
                );
                if sp_resp.secondary_clicked() {
                    self.reg_edit_popup = Some(RegisterId::SP);
                    self.reg_edit_value = format!("{:04X}", cpu.sp);
                }
                ui.monospace(format!("if=  {:02X}", dbg.if_reg));
                ui.end_row();

                // PC register
                let pc_resp = ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("pc= {:04X}", cpu.pc)).monospace(),
                    )
                    .sense(egui::Sense::click()),
                );
                if pc_resp.secondary_clicked() {
                    self.reg_edit_popup = Some(RegisterId::PC);
                    self.reg_edit_value = format!("{:04X}", cpu.pc);
                }
                ui.monospace(format!("ime= {}", if cpu.ime { 1 } else { 0 }));
                ui.end_row();
            });

        // Register edit popup
        if let Some(reg) = self.reg_edit_popup {
            let popup_id = ui.make_persistent_id("reg_edit_popup");
            let reg_name = match reg {
                RegisterId::AF => "AF",
                RegisterId::BC => "BC",
                RegisterId::DE => "DE",
                RegisterId::HL => "HL",
                RegisterId::SP => "SP",
                RegisterId::PC => "PC",
            };

            egui::Window::new(format!("Edit {}", reg_name))
                .id(popup_id)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Value:");
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.reg_edit_value)
                                .desired_width(60.0)
                                .font(egui::TextStyle::Monospace),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if let Ok(val) = u16::from_str_radix(&self.reg_edit_value, 16) {
                                pending_edit = Some((reg, val));
                            }
                            self.reg_edit_popup = None;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            if let Ok(val) = u16::from_str_radix(&self.reg_edit_value, 16) {
                                pending_edit = Some((reg, val));
                            }
                            self.reg_edit_popup = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.reg_edit_popup = None;
                        }
                    });
                });
        }

        // Send register edit command if needed
        if let Some((reg, value)) = pending_edit {
            let _ = self.emu_tx.send(EmuCommand::SetRegister { reg, value });
        }

        ui.add_space(4.0);

        // Flags as checkboxes
        let f = cpu.f;
        let mut z_flag = (f & 0x80) != 0;
        let mut n_flag = (f & 0x40) != 0;
        let mut h_flag = (f & 0x20) != 0;
        let mut c_flag = (f & 0x10) != 0;

        ui.horizontal(|ui| {
            let mut flags_changed = false;
            if ui.checkbox(&mut z_flag, "Z").changed() {
                flags_changed = true;
            }
            if ui.checkbox(&mut n_flag, "N").changed() {
                flags_changed = true;
            }
            if ui.checkbox(&mut h_flag, "H").changed() {
                flags_changed = true;
            }
            if ui.checkbox(&mut c_flag, "C").changed() {
                flags_changed = true;
            }
            if flags_changed {
                let new_f = (if z_flag { 0x80 } else { 0 })
                    | (if n_flag { 0x40 } else { 0 })
                    | (if h_flag { 0x20 } else { 0 })
                    | (if c_flag { 0x10 } else { 0 });
                let new_af = ((cpu.a as u16) << 8) | (new_f as u16);
                let _ = self.emu_tx.send(EmuCommand::SetRegister {
                    reg: RegisterId::AF,
                    value: new_af,
                });
            }
            ui.monospace(format!("rom= {:02X}", dbg.active_rom_bank));
        });

        ui.separator();

        // Breakpoints section
        ui.strong("Breakpoints");

        let mut to_remove: Option<BreakpointSpec> = None;
        let entries: Vec<(BreakpointSpec, bool)> = self
            .debugger_state
            .all_breakpoints()
            .map(|(&bp, &en)| (bp, en))
            .collect();

        egui::ScrollArea::vertical()
            .id_salt("bp_list")
            .max_height(100.0)
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

                        if ui.small_button("×").clicked() {
                            to_remove = Some(bp);
                        }
                    });
                }
            });

        if let Some(bp) = to_remove {
            self.debugger_state.remove_breakpoint(&bp);
        }

        ui.separator();

        // Stack view
        ui.strong("Stack");

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
        let mut to_remove: Option<usize> = None;

        // Watchpoint list (top portion, scrollable)
        let list_height = ui.available_height() - 140.0;
        egui::ScrollArea::vertical()
            .id_salt("wp_list")
            .max_height(list_height.max(80.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, wp) in self.watchpoints.iter().enumerate() {
                    let is_selected = self.wp_selected_index == Some(i);
                    let start = *wp.range.start();

                    // Build type string (r/w/x/j)
                    let mut type_str = String::new();
                    if wp.on_read {
                        type_str.push('r');
                    }
                    if wp.on_write {
                        type_str.push('w');
                    }
                    if wp.on_execute {
                        type_str.push('x');
                    }
                    if wp.on_jump {
                        type_str.push('j');
                    }
                    if type_str.is_empty() {
                        type_str = "-".to_string();
                    }

                    // Get label for address
                    let label_str = self
                        .debugger_state
                        .first_label_for(0, start)
                        .map(|s| format!("{:04X}+{}", 0, s))
                        .unwrap_or_default();

                    let text = format!(
                        "{}:{:04X}    {}    {}",
                        if wp.enabled { "0" } else { "-" },
                        start,
                        type_str,
                        label_str
                    );

                    let resp =
                        ui.selectable_label(is_selected, egui::RichText::new(text).monospace());
                    if resp.clicked() {
                        self.wp_selected_index = Some(i);
                        // Populate edit fields with selected watchpoint
                        let start = *wp.range.start();
                        let end = *wp.range.end();
                        self.wp_edit_addr_range = if start == end {
                            format!("{:04X}", start)
                        } else {
                            format!("{:04X}-{:04X}", start, end)
                        };
                        self.wp_edit_value = wp
                            .value_match
                            .map(|v| format!("{:02X}", v))
                            .unwrap_or_default();
                        self.wp_edit_on_read = wp.on_read;
                        self.wp_edit_on_write = wp.on_write;
                        self.wp_edit_on_execute = wp.on_execute;
                        self.wp_edit_on_jump = wp.on_jump;
                        self.wp_edit_debug_msg = wp.message.clone().unwrap_or_default();
                    }
                }
            });

        ui.add_space(8.0);

        // Input fields row
        ui.horizontal(|ui| {
            ui.label("addr range");
            ui.add(
                egui::TextEdit::singleline(&mut self.wp_edit_addr_range)
                    .desired_width(80.0)
                    .font(egui::TextStyle::Monospace),
            );
            ui.label("value");
            ui.add(
                egui::TextEdit::singleline(&mut self.wp_edit_value)
                    .desired_width(40.0)
                    .font(egui::TextStyle::Monospace),
            );
            ui.checkbox(&mut self.wp_edit_on_read, "on read");
            ui.checkbox(&mut self.wp_edit_on_execute, "on execute");
        });

        ui.horizontal(|ui| {
            ui.add_space(148.0); // Align with value field
            ui.checkbox(&mut self.wp_edit_on_write, "on write");
            ui.checkbox(&mut self.wp_edit_on_jump, "on jump");
        });

        ui.horizontal(|ui| {
            ui.label("Debug msg:");
            ui.add(
                egui::TextEdit::singleline(&mut self.wp_edit_debug_msg)
                    .desired_width(ui.available_width() - 10.0)
                    .font(egui::TextStyle::Monospace),
            );
        });

        ui.add_space(4.0);

        // Button row
        ui.horizontal(|ui| {
            if ui.button("Add").clicked()
                && let Some((start, end)) =
                    self.parse_watchpoint_range(&self.wp_edit_addr_range.clone())
            {
                let value_match = if self.wp_edit_value.is_empty() {
                    None
                } else {
                    u8::from_str_radix(self.wp_edit_value.trim(), 16).ok()
                };
                let wp = vibe_emu_core::watchpoints::Watchpoint {
                    id: self.next_watchpoint_id,
                    enabled: true,
                    range: start..=end,
                    on_read: self.wp_edit_on_read,
                    on_write: self.wp_edit_on_write,
                    on_execute: self.wp_edit_on_execute,
                    on_jump: self.wp_edit_on_jump,
                    value_match,
                    message: if self.wp_edit_debug_msg.is_empty() {
                        None
                    } else {
                        Some(self.wp_edit_debug_msg.clone())
                    },
                };
                self.next_watchpoint_id += 1;
                self.watchpoints.push(wp);
            }

            if ui.button("Delete").clicked()
                && let Some(i) = self.wp_selected_index
            {
                to_remove = Some(i);
            }

            if ui.button("Replace").clicked()
                && let Some(i) = self.wp_selected_index
                && let Some((start, end)) =
                    self.parse_watchpoint_range(&self.wp_edit_addr_range.clone())
            {
                let value_match = if self.wp_edit_value.is_empty() {
                    None
                } else {
                    u8::from_str_radix(self.wp_edit_value.trim(), 16).ok()
                };
                if let Some(wp) = self.watchpoints.get_mut(i) {
                    wp.range = start..=end;
                    wp.on_read = self.wp_edit_on_read;
                    wp.on_write = self.wp_edit_on_write;
                    wp.on_execute = self.wp_edit_on_execute;
                    wp.on_jump = self.wp_edit_on_jump;
                    wp.value_match = value_match;
                    wp.message = if self.wp_edit_debug_msg.is_empty() {
                        None
                    } else {
                        Some(self.wp_edit_debug_msg.clone())
                    };
                }
            }

            ui.add_space(20.0);

            if ui.button("Disable").clicked()
                && let Some(i) = self.wp_selected_index
                && let Some(wp) = self.watchpoints.get_mut(i)
            {
                wp.enabled = false;
            }

            if ui.button("Enable").clicked()
                && let Some(i) = self.wp_selected_index
                && let Some(wp) = self.watchpoints.get_mut(i)
            {
                wp.enabled = true;
            }
        });

        // Handle deletion
        if let Some(i) = to_remove {
            self.watchpoints.remove(i);
            self.wp_selected_index = None;
        }
    }

    fn parse_watchpoint_range(&self, input: &str) -> Option<(u16, u16)> {
        let trimmed = input.trim();
        let trimmed = trimmed
            .strip_prefix("$")
            .or_else(|| trimmed.strip_prefix("0x"))
            .unwrap_or(trimmed);

        if let Some((start_str, end_str)) = trimmed.split_once('-') {
            let start = u16::from_str_radix(start_str.trim(), 16).ok()?;
            let end = u16::from_str_radix(end_str.trim(), 16).ok()?;
            Some((start, end))
        } else {
            let addr = u16::from_str_radix(trimmed, 16).ok()?;
            Some((addr, addr))
        }
    }

    fn draw_memory_viewer(&mut self, ui: &mut egui::Ui, snapshot: &UiSnapshot) {
        let Some(mem) = snapshot.debugger.mem_image.as_ref() else {
            ui.label("Memory not available (run paused to capture)");
            return;
        };

        // Top bar with go-to address
        ui.horizontal(|ui| {
            ui.label("Go:");
            let goto_resp = ui.add(
                egui::TextEdit::singleline(&mut self.mem_viewer_goto)
                    .desired_width(80.0)
                    .font(egui::TextStyle::Monospace),
            );
            let submitted = goto_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Go").clicked() || submitted {
                if let Some(addr) =
                    self.parse_mem_viewer_address(&self.mem_viewer_goto.clone(), snapshot)
                {
                    self.mem_viewer_addr = addr & 0xFFF0; // Align to 16-byte row
                    self.mem_viewer_cursor = addr;
                    self.mem_viewer_scroll_to = Some((addr as usize) / 16);
                }
                self.mem_viewer_goto.clear();
            }
        });

        ui.separator();

        // Render all rows - egui's ScrollArea handles virtualization
        // show_rows expects row_height_sans_spacing - it adds item_spacing.y internally
        let text_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let spacing = ui.spacing();
        let row_height_sans_spacing = text_height.max(spacing.interact_size.y);
        let row_height_with_spacing = row_height_sans_spacing + spacing.item_spacing.y;
        let bytes_per_row = 16usize;
        let total_rows = 0x10000usize.div_ceil(bytes_per_row);

        let scroll_to_row = self.mem_viewer_scroll_to.take();

        let mut scroll_area = egui::ScrollArea::vertical()
            .id_salt("mem_viewer_scroll")
            .auto_shrink([false, false]);

        // Set scroll offset using the same row height that show_rows uses internally
        if let Some(target_row) = scroll_to_row {
            let target_offset = target_row as f32 * row_height_with_spacing;
            scroll_area = scroll_area.vertical_scroll_offset(target_offset);
        }

        scroll_area.show_rows(ui, row_height_sans_spacing, total_rows, |ui, row_range| {
            for row_idx in row_range {
                let row_addr = (row_idx * bytes_per_row) as u16;
                let region = self.mem_region_prefix(row_addr, snapshot);

                ui.horizontal(|ui| {
                    ui.monospace(format!("{}:{:04X}", region, row_addr));
                    ui.add_space(8.0);

                    for col in 0..bytes_per_row {
                        let addr = row_addr.wrapping_add(col as u16);
                        let byte = mem[addr as usize];

                        let is_cursor = addr == self.mem_viewer_cursor;
                        let text = format!("{:02X}", byte);

                        let label = if is_cursor {
                            egui::RichText::new(text)
                                .monospace()
                                .background_color(egui::Color32::from_rgb(0, 80, 160))
                        } else {
                            egui::RichText::new(text).monospace()
                        };

                        if ui
                            .add(egui::Label::new(label).sense(egui::Sense::click()))
                            .clicked()
                        {
                            self.mem_viewer_cursor = addr;
                        }

                        if col == 7 {
                            ui.add_space(4.0);
                        }
                    }

                    ui.add_space(8.0);

                    let mut ascii = String::with_capacity(bytes_per_row);
                    for col in 0..bytes_per_row {
                        let addr = row_addr.wrapping_add(col as u16);
                        let byte = mem[addr as usize];
                        let c = if (0x20..=0x7E).contains(&byte) {
                            byte as char
                        } else {
                            '.'
                        };
                        ascii.push(c);
                    }
                    ui.monospace(ascii);
                });
            }
        });

        ui.separator();

        // Status bar showing label at cursor
        let cursor_addr = self.mem_viewer_cursor;
        let cursor_bank = self.bank_for_address(cursor_addr, snapshot);

        let label_info = if let Some(sym) = self.debugger_state.symbols() {
            if let Some((label, offset)) = sym.nearest_label_for(cursor_bank, cursor_addr) {
                if offset == 0 {
                    label.to_string()
                } else {
                    format!("{}+${:X}", label, offset)
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        ui.horizontal(|ui| {
            ui.monospace(format!(
                "{:04X}  {:02X}:{:04X}",
                cursor_addr, cursor_bank, cursor_addr
            ));
            if !label_info.is_empty() {
                ui.monospace(format!("  {}", label_info));
            }
        });
    }

    fn parse_mem_viewer_address(&mut self, input: &str, _snapshot: &UiSnapshot) -> Option<u16> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Try parsing as bank:address format (e.g., 00:c000 or 05:4200)
        if let Some((bank_str, addr_str)) = trimmed.split_once(':')
            && let (Ok(bank), Ok(addr)) = (
                u8::from_str_radix(bank_str.trim_start_matches('$'), 16),
                u16::from_str_radix(addr_str.trim_start_matches('$'), 16),
            )
        {
            // Store the display bank for switchable regions
            match addr {
                0x4000..=0x7FFF => self.mem_viewer_display_bank = Some(bank),
                _ => self.mem_viewer_display_bank = None,
            }
            return Some(addr);
        }

        // Clear display bank override for non-bank:address input
        self.mem_viewer_display_bank = None;

        // Try parsing as hex number
        if let Some(hex) = trimmed.strip_prefix("$").or(Some(trimmed))
            && let Ok(addr) = u16::from_str_radix(hex, 16)
        {
            return Some(addr);
        }

        // Try symbol lookup
        if let Some(sym) = self.debugger_state.symbols()
            && let Some((_, addr)) = sym.lookup_name(trimmed)
        {
            return Some(addr);
        }

        None
    }

    fn mem_region_prefix(&self, addr: u16, snapshot: &UiSnapshot) -> String {
        match addr {
            0x0000..=0x3FFF => "RO00".to_string(),
            0x4000..=0x7FFF => {
                let bank = self
                    .mem_viewer_display_bank
                    .unwrap_or(snapshot.debugger.active_rom_bank.min(0xFF) as u8);
                format!("RO{:02X}", bank)
            }
            0x8000..=0x9FFF => format!("VR{:02X}", snapshot.debugger.vram_bank),
            0xA000..=0xBFFF => format!("SR{:02X}", snapshot.debugger.sram_bank),
            0xC000..=0xCFFF => "WR00".to_string(),
            0xD000..=0xDFFF => {
                let bank = snapshot.debugger.wram_bank.max(1);
                format!("WR{:02X}", bank)
            }
            0xE000..=0xFDFF => "ECHO".to_string(),
            0xFE00..=0xFE9F => "OAM ".to_string(),
            0xFEA0..=0xFEFF => "----".to_string(),
            0xFF00..=0xFF7F => "I/O ".to_string(),
            0xFF80..=0xFFFE => "HRAM".to_string(),
            0xFFFF => "IE  ".to_string(),
        }
    }

    fn bank_for_address(&self, addr: u16, snapshot: &UiSnapshot) -> u8 {
        match addr {
            0x0000..=0x3FFF => 0,
            0x4000..=0x7FFF => snapshot.debugger.active_rom_bank.min(0xFF) as u8,
            0x8000..=0x9FFF => snapshot.debugger.vram_bank,
            0xA000..=0xBFFF => snapshot.debugger.sram_bank,
            0xC000..=0xCFFF => 0,
            0xD000..=0xDFFF => snapshot.debugger.wram_bank,
            _ => 0,
        }
    }

    fn draw_vram_viewer_window(&mut self, ctx: &egui::Context) {
        // During fast forward, use try_lock to avoid slowing down emulation.
        // During normal play, use blocking lock to ensure fresh data every frame.
        if self.fast_forward {
            if let Ok(mut gb) = self.gb.try_lock() {
                self.cached_ppu_snapshot = Some(UiSnapshot::from_gb(&mut gb, self.paused).ppu);
            }
        } else if let Ok(mut gb) = self.gb.lock() {
            self.cached_ppu_snapshot = Some(UiSnapshot::from_gb(&mut gb, self.paused).ppu);
        }

        // Clone so we can pass it into the closure without borrowing self
        let ppu_snapshot = self.cached_ppu_snapshot.clone();

        ctx.show_viewport_immediate(
            *VIEWPORT_VRAM_VIEWER,
            egui::ViewportBuilder::default()
                .with_title("VRAM Viewer")
                .with_inner_size([700.0, 500.0]),
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

        let lcdc = ppu.lcdc;
        let cgb = ppu.cgb;

        let map_base = match self.vram_viewer.bg_map_select {
            BgMapSelect::Auto => {
                if lcdc & 0x08 != 0 {
                    0x1C00
                } else {
                    0x1800
                }
            }
            BgMapSelect::Map9800 => 0x1800,
            BgMapSelect::Map9C00 => 0x1C00,
        };

        let signed_mode = match self.vram_viewer.bg_tile_data_select {
            TileDataSelect::Auto => lcdc & 0x10 == 0,
            TileDataSelect::Addr8800 => true,
            TileDataSelect::Addr8000 => false,
        };

        let frame = ppu.frame_counter;
        let map_select = self.vram_viewer.bg_map_select;
        let tile_select = self.vram_viewer.bg_tile_data_select;

        if frame != self.vram_viewer.bg_last_frame || self.vram_viewer.bg_map_tex.is_none() {
            self.vram_viewer.bg_last_frame = frame;
            let rgba = &mut self.vram_viewer.bg_map_buf;
            rgba.fill(0);

            let bgp = ppu.bgp;

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
            let image = egui::ColorImage::new([IMG_W, IMG_H], pixels);
            match &mut self.vram_viewer.bg_map_tex {
                Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
                None => {
                    self.vram_viewer.bg_map_tex =
                        Some(ctx.load_texture("bg_map", image, egui::TextureOptions::NEAREST));
                }
            }
        }

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                if let Some(tex) = &self.vram_viewer.bg_map_tex {
                    let scale = 1.5_f32;
                    let draw_size = egui::vec2(256.0 * scale, 256.0 * scale);

                    let (response, painter) = ui.allocate_painter(draw_size, egui::Sense::click());
                    let rect = response.rect;

                    painter.image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );

                    if self.vram_viewer.bg_show_grid {
                        let grid_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(80));
                        for i in 1..MAP_W {
                            let x = rect.min.x + (i as f32) * 8.0 * scale;
                            painter.line_segment(
                                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                                grid_stroke,
                            );
                        }
                        for i in 1..MAP_H {
                            let y = rect.min.y + (i as f32) * 8.0 * scale;
                            painter.line_segment(
                                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                                grid_stroke,
                            );
                        }
                    }

                    if self.vram_viewer.bg_show_viewport {
                        let scx = ppu.scx as f32;
                        let scy = ppu.scy as f32;
                        let vp_w = 160.0;
                        let vp_h = 144.0;
                        let map_size = 256.0;
                        let stroke = egui::Stroke::new(1.5, egui::Color32::RED);

                        let x_wraps = scx + vp_w > map_size;
                        let y_wraps = scy + vp_h > map_size;

                        if !x_wraps && !y_wraps {
                            let viewport_rect = egui::Rect::from_min_size(
                                rect.min + egui::vec2(scx * scale, scy * scale),
                                egui::vec2(vp_w * scale, vp_h * scale),
                            );
                            painter.rect_stroke(
                                viewport_rect,
                                0.0,
                                stroke,
                                egui::StrokeKind::Middle,
                            );
                        } else {
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

                            let r1 = egui::Rect::from_min_size(
                                rect.min + egui::vec2(x1_start * scale, y1_start * scale),
                                egui::vec2(x1_w * scale, y1_h * scale),
                            );
                            painter.rect_stroke(r1, 0.0, stroke, egui::StrokeKind::Middle);

                            if x_wraps {
                                let r2 = egui::Rect::from_min_size(
                                    rect.min + egui::vec2(x2_start * scale, y1_start * scale),
                                    egui::vec2(x2_w * scale, y1_h * scale),
                                );
                                painter.rect_stroke(r2, 0.0, stroke, egui::StrokeKind::Middle);
                            }
                            if y_wraps {
                                let r3 = egui::Rect::from_min_size(
                                    rect.min + egui::vec2(x1_start * scale, y2_start * scale),
                                    egui::vec2(x1_w * scale, y2_h * scale),
                                );
                                painter.rect_stroke(r3, 0.0, stroke, egui::StrokeKind::Middle);
                            }
                            if x_wraps && y_wraps {
                                let r4 = egui::Rect::from_min_size(
                                    rect.min + egui::vec2(x2_start * scale, y2_start * scale),
                                    egui::vec2(x2_w * scale, y2_h * scale),
                                );
                                painter.rect_stroke(r4, 0.0, stroke, egui::StrokeKind::Middle);
                            }
                        }
                    }

                    if let Some((sel_x, sel_y)) = self.vram_viewer.bg_selected_tile {
                        let sel_rect = egui::Rect::from_min_size(
                            rect.min
                                + egui::vec2(
                                    sel_x as f32 * 8.0 * scale,
                                    sel_y as f32 * 8.0 * scale,
                                ),
                            egui::vec2(8.0 * scale, 8.0 * scale),
                        );
                        painter.rect_stroke(
                            sel_rect,
                            0.0,
                            egui::Stroke::new(2.0, egui::Color32::YELLOW),
                            egui::StrokeKind::Middle,
                        );
                    }

                    if response.clicked()
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        let rel = pos - rect.min;
                        let tile_x = ((rel.x / scale) / 8.0) as u8;
                        let tile_y = ((rel.y / scale) / 8.0) as u8;
                        if tile_x < 32 && tile_y < 32 {
                            self.vram_viewer.bg_selected_tile = Some((tile_x, tile_y));
                        }
                    }

                    if response.hovered()
                        && let Some(pos) = ui.ctx().pointer_hover_pos()
                        && rect.contains(pos)
                    {
                        let rel = pos - rect.min;
                        let tile_x = ((rel.x / scale) / 8.0) as u8;
                        let tile_y = ((rel.y / scale) / 8.0) as u8;
                        if tile_x < 32 && tile_y < 32 {
                            response.on_hover_text(format!("Tile ({tile_x}, {tile_y})"));
                        }
                    }
                }
            });

            ui.add_space(8.0);

            ui.vertical(|ui| {
                let (
                    sel_x,
                    sel_y,
                    tile_idx,
                    attr,
                    bank,
                    x_flip,
                    y_flip,
                    priority,
                    pal_num,
                    tile_addr,
                ) = if let Some((sel_x, sel_y)) = self.vram_viewer.bg_selected_tile {
                    let tile_idx = ppu.vram0[map_base + (sel_y as usize) * MAP_W + sel_x as usize];
                    let attr = if cgb {
                        ppu.vram1[map_base + (sel_y as usize) * MAP_W + sel_x as usize]
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
                    let priority = cgb && attr & 0x80 != 0;
                    let pal_num = attr & 0x07;

                    (
                        Some(sel_x),
                        Some(sel_y),
                        Some(tile_idx),
                        Some(attr),
                        bank,
                        x_flip,
                        y_flip,
                        priority,
                        pal_num,
                        Some(tile_addr),
                    )
                } else {
                    (None, None, None, None, 0, false, false, false, 0, None)
                };

                if let Some(tile_addr) = tile_addr {
                    let preview_buf = &mut self.vram_viewer.bg_tile_preview_buf;
                    let vram = ppu.vram_bank(bank);
                    if tile_addr + 16 <= vram.len() {
                        for row in 0..8 {
                            let actual_row = if y_flip { 7 - row } else { row };
                            let lo = vram[tile_addr + actual_row * 2];
                            let hi = vram[tile_addr + actual_row * 2 + 1];
                            for col in 0..8 {
                                let actual_col = if x_flip { col } else { 7 - col };
                                let bit = actual_col;
                                let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
                                let color = if cgb {
                                    ppu.cgb_bg_colors[pal_num as usize][idx as usize]
                                } else {
                                    let shade = (ppu.bgp >> (idx * 2)) & 0x03;
                                    DMG_COLORS[shade as usize]
                                };
                                let off = (row * 8 + col) * 4;
                                preview_buf[off] = ((color >> 16) & 0xFF) as u8;
                                preview_buf[off + 1] = ((color >> 8) & 0xFF) as u8;
                                preview_buf[off + 2] = (color & 0xFF) as u8;
                                preview_buf[off + 3] = 0xFF;
                            }
                        }
                    }

                    let pixels: Vec<egui::Color32> = preview_buf
                        .chunks_exact(4)
                        .map(|c| egui::Color32::from_rgb(c[0], c[1], c[2]))
                        .collect();
                    let image = egui::ColorImage::new([8, 8], pixels);
                    match &mut self.vram_viewer.bg_tile_preview_tex {
                        Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
                        None => {
                            self.vram_viewer.bg_tile_preview_tex = Some(ctx.load_texture(
                                "bg_tile_preview",
                                image,
                                egui::TextureOptions::NEAREST,
                            ));
                        }
                    }

                    ui.horizontal(|ui| {
                        if let Some(tex) = &self.vram_viewer.bg_tile_preview_tex {
                            ui.image((tex.id(), egui::vec2(64.0, 64.0)));
                        }
                    });
                } else {
                    ui.allocate_space(egui::vec2(64.0, 64.0));
                }

                ui.add_space(8.0);
                ui.heading("Details");

                egui::Grid::new("bg_map_details")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("X");
                        ui.label(sel_x.map_or("--".to_string(), |x| format!("{:02X}", x)));
                        ui.end_row();

                        ui.label("Y");
                        ui.label(sel_y.map_or("--".to_string(), |y| format!("{:02X}", y)));
                        ui.end_row();

                        ui.label("Tile No.");
                        ui.label(tile_idx.map_or("--".to_string(), |t| format!("{:02X}", t)));
                        ui.end_row();

                        if cgb {
                            ui.label("Attribute");
                            ui.label(attr.map_or("--".to_string(), |a| format!("{:02X}", a)));
                            ui.end_row();
                        }

                        ui.label("Map address");
                        if let (Some(sel_x), Some(sel_y)) = (sel_x, sel_y) {
                            let map_addr =
                                0x8000 + map_base + (sel_y as usize) * MAP_W + sel_x as usize;
                            ui.label(format!("{:04X}", map_addr));
                        } else {
                            ui.label("----");
                        }
                        ui.end_row();

                        ui.label("Tile address");
                        if let Some(tile_addr) = tile_addr {
                            let vram_tile_addr = 0x8000 + tile_addr;
                            ui.label(format!("{}:{:04X}", bank, vram_tile_addr));
                        } else {
                            ui.label("-:----");
                        }
                        ui.end_row();

                        if cgb {
                            ui.label("X-flip");
                            ui.label(if sel_x.is_some() {
                                if x_flip { "Yes" } else { "No" }
                            } else {
                                "--"
                            });
                            ui.end_row();

                            ui.label("Y-flip");
                            ui.label(if sel_y.is_some() {
                                if y_flip { "Yes" } else { "No" }
                            } else {
                                "--"
                            });
                            ui.end_row();

                            ui.label("BG palette");
                            ui.label(if sel_x.is_some() {
                                format!("{}", pal_num)
                            } else {
                                "--".to_string()
                            });
                            ui.end_row();

                            ui.label("Priority");
                            ui.label(if sel_x.is_some() {
                                if priority { "Yes" } else { "No" }
                            } else {
                                "--"
                            });
                            ui.end_row();
                        }
                    });
            });

            ui.add_space(8.0);

            ui.vertical(|ui| {
                ui.checkbox(&mut self.vram_viewer.bg_show_grid, "Grid");
                ui.checkbox(&mut self.vram_viewer.bg_show_viewport, "SCX/SCY viewport");

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Map:");
                    ui.selectable_value(
                        &mut self.vram_viewer.bg_map_select,
                        BgMapSelect::Auto,
                        "Auto",
                    );
                    ui.selectable_value(
                        &mut self.vram_viewer.bg_map_select,
                        BgMapSelect::Map9800,
                        "9800",
                    );
                    ui.selectable_value(
                        &mut self.vram_viewer.bg_map_select,
                        BgMapSelect::Map9C00,
                        "9C00",
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Tiles:");
                    ui.selectable_value(
                        &mut self.vram_viewer.bg_tile_data_select,
                        TileDataSelect::Auto,
                        "Auto",
                    );
                    ui.selectable_value(
                        &mut self.vram_viewer.bg_tile_data_select,
                        TileDataSelect::Addr8800,
                        "8800",
                    );
                    ui.selectable_value(
                        &mut self.vram_viewer.bg_tile_data_select,
                        TileDataSelect::Addr8000,
                        "8000",
                    );
                });

                ui.add_space(8.0);
                let active_map = match map_select {
                    BgMapSelect::Auto => {
                        if lcdc & 0x08 != 0 {
                            "9C00"
                        } else {
                            "9800"
                        }
                    }
                    BgMapSelect::Map9800 => "9800",
                    BgMapSelect::Map9C00 => "9C00",
                };
                let active_tiles = match tile_select {
                    TileDataSelect::Auto => {
                        if lcdc & 0x10 != 0 {
                            "8000"
                        } else {
                            "8800"
                        }
                    }
                    TileDataSelect::Addr8800 => "8800",
                    TileDataSelect::Addr8000 => "8000",
                };
                ui.label(format!("Active: Map ${active_map}, Tiles ${active_tiles}"));
                ui.label(format!("SCX: {}, SCY: {}", ppu.scx, ppu.scy));
            });
        });
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
        let show_paletted = self.vram_viewer.tiles_show_paletted;

        if frame != self.vram_viewer.tiles_last_frame || self.vram_viewer.tiles_tex.is_none() {
            self.vram_viewer.tiles_last_frame = frame;
            let buf = &mut self.vram_viewer.tiles_buf[..img_w * img_h * 4];
            buf.fill(0);

            let bgp = ppu.bgp;

            for bank in 0..banks {
                for tile_idx in 0..384 {
                    let col = tile_idx % TILES_PER_ROW;
                    let row = tile_idx / TILES_PER_ROW;
                    let tile_addr = tile_idx * 16;

                    let pal_idx = if show_paletted && ppu.cgb {
                        Self::guess_tile_palette(ppu, bank, tile_idx)
                    } else {
                        None
                    };

                    for y in 0..TILE_H {
                        let vram = ppu.vram_bank(bank);
                        let lo = vram[tile_addr + y * 2];
                        let hi = vram[tile_addr + y * 2 + 1];

                        for x in 0..TILE_W {
                            let bit = 7 - x;
                            let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);

                            let rgb = if ppu.cgb {
                                match pal_idx {
                                    Some(GuessedPalette::Bg(pal)) => {
                                        ppu.cgb_bg_colors[pal][idx as usize]
                                    }
                                    Some(GuessedPalette::Obj(pal)) => {
                                        ppu.cgb_ob_colors[pal][idx as usize]
                                    }
                                    None => {
                                        // Grayscale for tiles without a guessed palette
                                        let gray = [0x00FFFFFF, 0x00AAAAAA, 0x00555555, 0x00000000];
                                        gray[idx as usize]
                                    }
                                }
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
            let image = egui::ColorImage::new([img_w, img_h], pixels);
            match &mut self.vram_viewer.tiles_tex {
                Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
                None => {
                    self.vram_viewer.tiles_tex =
                        Some(ctx.load_texture("tiles", image, egui::TextureOptions::NEAREST));
                }
            }
        }

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                if let Some(tex) = &self.vram_viewer.tiles_tex {
                    let scale = 2.0_f32;
                    let tex_w = img_w as f32;
                    let tex_h = img_h as f32;
                    let draw_size = egui::vec2(tex_w * scale, tex_h * scale);

                    let (response, painter) = ui.allocate_painter(draw_size, egui::Sense::click());
                    let rect = response.rect;

                    painter.image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );

                    if self.vram_viewer.tiles_show_grid {
                        let grid_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(80));
                        let total_cols = TILES_PER_ROW * banks;
                        for i in 1..total_cols {
                            let x = rect.min.x + (i as f32) * 8.0 * scale;
                            painter.line_segment(
                                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                                grid_stroke,
                            );
                        }
                        for i in 1..ROWS {
                            let y = rect.min.y + (i as f32) * 8.0 * scale;
                            painter.line_segment(
                                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                                grid_stroke,
                            );
                        }
                    }

                    if let Some((sel_bank, sel_idx)) = self.vram_viewer.tiles_selected {
                        let col = (sel_idx as usize) % TILES_PER_ROW;
                        let row = (sel_idx as usize) / TILES_PER_ROW;
                        let x_off = (sel_bank as usize) * TILES_PER_ROW + col;
                        let sel_rect = egui::Rect::from_min_size(
                            rect.min
                                + egui::vec2(x_off as f32 * 8.0 * scale, row as f32 * 8.0 * scale),
                            egui::vec2(8.0 * scale, 8.0 * scale),
                        );
                        painter.rect_stroke(
                            sel_rect,
                            0.0,
                            egui::Stroke::new(2.0, egui::Color32::YELLOW),
                            egui::StrokeKind::Middle,
                        );
                    }

                    if response.clicked()
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        let rel = pos - rect.min;
                        let tile_col = ((rel.x / scale) / 8.0) as usize;
                        let tile_row = ((rel.y / scale) / 8.0) as usize;
                        let bank = if tile_col >= TILES_PER_ROW { 1 } else { 0 };
                        let col_in_bank = tile_col % TILES_PER_ROW;
                        let tile_idx = tile_row * TILES_PER_ROW + col_in_bank;
                        if tile_idx < 384 {
                            self.vram_viewer.tiles_selected = Some((bank as u8, tile_idx as u16));
                        }
                    }

                    if response.hovered()
                        && let Some(pos) = ui.ctx().pointer_hover_pos()
                        && rect.contains(pos)
                    {
                        let rel = pos - rect.min;
                        let tile_col = ((rel.x / scale) / 8.0) as usize;
                        let tile_row = ((rel.y / scale) / 8.0) as usize;
                        response.on_hover_text(format!("Tile ({tile_col}, {tile_row})"));
                    }
                }
            });

            ui.add_space(8.0);

            ui.vertical(|ui| {
                if let Some((sel_bank, sel_idx)) = self.vram_viewer.tiles_selected {
                    let tile_addr = (sel_idx as usize) * 16;
                    let vram = ppu.vram_bank(sel_bank as usize);

                    let pal_idx = if ppu.cgb {
                        Self::guess_tile_palette(ppu, sel_bank as usize, sel_idx as usize)
                    } else {
                        None
                    };

                    let preview_buf = &mut self.vram_viewer.tiles_preview_buf;
                    if tile_addr + 16 <= vram.len() {
                        for row in 0..8 {
                            let lo = vram[tile_addr + row * 2];
                            let hi = vram[tile_addr + row * 2 + 1];
                            for col in 0..8 {
                                let bit = 7 - col;
                                let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
                                let color = if ppu.cgb {
                                    match pal_idx {
                                        Some(GuessedPalette::Bg(pal)) => {
                                            ppu.cgb_bg_colors[pal][idx as usize]
                                        }
                                        Some(GuessedPalette::Obj(pal)) => {
                                            ppu.cgb_ob_colors[pal][idx as usize]
                                        }
                                        None => {
                                            let gray =
                                                [0x00FFFFFF, 0x00AAAAAA, 0x00555555, 0x00000000];
                                            gray[idx as usize]
                                        }
                                    }
                                } else {
                                    let shade = (ppu.bgp >> (idx * 2)) & 0x03;
                                    DMG_COLORS[shade as usize]
                                };
                                let off = (row * 8 + col) * 4;
                                preview_buf[off] = ((color >> 16) & 0xFF) as u8;
                                preview_buf[off + 1] = ((color >> 8) & 0xFF) as u8;
                                preview_buf[off + 2] = (color & 0xFF) as u8;
                                preview_buf[off + 3] = 0xFF;
                            }
                        }
                    }

                    let pixels: Vec<egui::Color32> = preview_buf
                        .chunks_exact(4)
                        .map(|c| egui::Color32::from_rgb(c[0], c[1], c[2]))
                        .collect();
                    let image = egui::ColorImage::new([8, 8], pixels);
                    match &mut self.vram_viewer.tiles_preview_tex {
                        Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
                        None => {
                            self.vram_viewer.tiles_preview_tex = Some(ctx.load_texture(
                                "tiles_preview",
                                image,
                                egui::TextureOptions::NEAREST,
                            ));
                        }
                    }

                    if let Some(tex) = &self.vram_viewer.tiles_preview_tex {
                        ui.image((tex.id(), egui::vec2(64.0, 64.0)));
                    }
                } else {
                    ui.allocate_space(egui::vec2(64.0, 64.0));
                }

                ui.add_space(8.0);

                egui::Grid::new("tiles_details")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Tile Number");
                        if let Some((_, sel_idx)) = self.vram_viewer.tiles_selected {
                            ui.label(format!("{:02X}", sel_idx));
                        } else {
                            ui.label("--");
                        }
                        ui.end_row();

                        ui.label("Tile Address");
                        if let Some((sel_bank, sel_idx)) = self.vram_viewer.tiles_selected {
                            let addr = 0x8000 + (sel_idx as usize) * 16;
                            ui.label(format!("{}:{:04X}", sel_bank, addr));
                        } else {
                            ui.label("-:----");
                        }
                        ui.end_row();

                        if ppu.cgb {
                            ui.label("Guessed palette");
                            if let Some((sel_bank, sel_idx)) = self.vram_viewer.tiles_selected {
                                match Self::guess_tile_palette(
                                    ppu,
                                    sel_bank as usize,
                                    sel_idx as usize,
                                ) {
                                    Some(GuessedPalette::Bg(pal)) => {
                                        ui.label(format!("BG {}", pal));
                                    }
                                    Some(GuessedPalette::Obj(pal)) => {
                                        ui.label(format!("OBJ {}", pal));
                                    }
                                    None => {
                                        ui.label("");
                                    }
                                }
                            } else {
                                ui.label("--");
                            }
                            ui.end_row();
                        }
                    });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.checkbox(&mut self.vram_viewer.tiles_show_paletted, "Show paletted");
                ui.checkbox(&mut self.vram_viewer.tiles_show_grid, "Grid");
            });
        });
    }

    fn guess_tile_palette(
        ppu: &ui::snapshot::PpuSnapshot,
        bank: usize,
        tile_idx: usize,
    ) -> Option<GuessedPalette> {
        // Check BG maps first
        let map_bases = [0x1800_usize, 0x1C00_usize];
        for &map_base in &map_bases {
            for map_offset in 0..(32 * 32) {
                let map_tile = ppu.vram0[map_base + map_offset];
                let attr = ppu.vram1[map_base + map_offset];
                let attr_bank = if attr & 0x08 != 0 { 1 } else { 0 };

                let lcdc = ppu.lcdc;
                let signed_mode = lcdc & 0x10 == 0;
                let effective_tile = if signed_mode {
                    // 8800 mode: base $9000 with signed offset
                    // map value 0 → tile 256, value -128 (0x80) → tile 128
                    ((map_tile as i8 as i32) + 256) as usize
                } else {
                    // 8000 mode: direct mapping to tiles 0-255
                    map_tile as usize
                };

                if effective_tile == tile_idx && attr_bank == bank {
                    return Some(GuessedPalette::Bg((attr & 0x07) as usize));
                }
            }
        }

        // Check OAM entries
        let tall_sprites = ppu.lcdc & 0x04 != 0;
        for i in 0..40 {
            let base = i * 4;
            let oam_tile = ppu.oam[base + 2] as usize;
            let attr = ppu.oam[base + 3];
            let oam_bank = if ppu.cgb && attr & 0x08 != 0 { 1 } else { 0 };

            // In 8x16 mode, the tile index has bit 0 ignored
            let (tile_match, tile_match_bottom) = if tall_sprites {
                let top_tile = oam_tile & 0xFE;
                let bottom_tile = oam_tile | 0x01;
                (tile_idx == top_tile, tile_idx == bottom_tile)
            } else {
                (tile_idx == oam_tile, false)
            };

            if (tile_match || tile_match_bottom) && oam_bank == bank {
                let pal = if ppu.cgb {
                    (attr & 0x07) as usize
                } else {
                    // DMG: bit 4 selects OBP0 or OBP1
                    if attr & 0x10 != 0 { 1 } else { 0 }
                };
                return Some(GuessedPalette::Obj(pal));
            }
        }

        None
    }

    fn draw_oam_tab(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        ppu: &ui::snapshot::PpuSnapshot,
    ) {
        let sprite_h: u8 = if ppu.lcdc & 0x04 != 0 { 16 } else { 8 };
        let is_8x16 = sprite_h == 16;

        if sprite_h != self.vram_viewer.oam_sprite_h {
            self.vram_viewer.oam_sprite_h = sprite_h;
        }

        const DMG_COLORS: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        let render_sprite_to_buf =
            |buf: &mut [u8], sprite_idx: usize, ppu: &ui::snapshot::PpuSnapshot| {
                let base = sprite_idx * 4;
                let tile_num = ppu.oam[base + 2];
                let attr = ppu.oam[base + 3];
                let x_flip = attr & 0x20 != 0;
                let y_flip = attr & 0x40 != 0;
                let bank = if ppu.cgb && (attr & 0x08 != 0) { 1 } else { 0 };

                let tile_num_base = if is_8x16 { tile_num & 0xFE } else { tile_num };

                for ty in 0..sprite_h as usize {
                    let actual_ty = if y_flip {
                        sprite_h as usize - 1 - ty
                    } else {
                        ty
                    };
                    let tile_offset = if is_8x16 && actual_ty >= 8 { 1 } else { 0 };
                    let current_tile = tile_num_base.wrapping_add(tile_offset);
                    let row_in_tile = (actual_ty % 8) as u16;

                    let tile_addr = (current_tile as u16) * 16;
                    let vram = if bank == 1 { &ppu.vram1 } else { &ppu.vram0 };
                    let lo = vram
                        .get(tile_addr as usize + row_in_tile as usize * 2)
                        .copied()
                        .unwrap_or(0);
                    let hi = vram
                        .get(tile_addr as usize + row_in_tile as usize * 2 + 1)
                        .copied()
                        .unwrap_or(0);

                    for tx in 0..8usize {
                        let actual_tx = if x_flip { 7 - tx } else { tx };
                        let bit = 7 - actual_tx;
                        let color_idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);

                        let rgb = if ppu.cgb {
                            let pal_num = (attr & 0x07) as usize;
                            if color_idx == 0 {
                                0x00FFFFFF
                            } else {
                                ppu.cgb_ob_colors[pal_num][color_idx as usize]
                            }
                        } else {
                            let obp = if attr & 0x10 != 0 { ppu.obp1 } else { ppu.obp0 };
                            let shade = (obp >> (color_idx * 2)) & 0x03;
                            if color_idx == 0 {
                                0x00FFFFFF
                            } else {
                                DMG_COLORS[shade as usize]
                            }
                        };

                        let px = ty * 8 + tx;
                        buf[px * 4] = ((rgb >> 16) & 0xFF) as u8;
                        buf[px * 4 + 1] = ((rgb >> 8) & 0xFF) as u8;
                        buf[px * 4 + 2] = (rgb & 0xFF) as u8;
                        buf[px * 4 + 3] = if color_idx == 0 { 0 } else { 255 };
                    }
                }
            };

        let frame = ppu.frame_counter;
        let needs_update =
            frame != self.vram_viewer.oam_last_frame || self.vram_viewer.oam_screen_tex.is_none();

        if needs_update {
            self.vram_viewer.oam_last_frame = frame;

            for i in 0..40 {
                render_sprite_to_buf(&mut self.vram_viewer.oam_sprite_bufs[i], i, ppu);
            }

            for (px_idx, &px) in ppu.framebuffer.iter().enumerate() {
                let r = ((px >> 16) & 0xFF) as u8;
                let g = ((px >> 8) & 0xFF) as u8;
                let b = (px & 0xFF) as u8;
                self.vram_viewer.oam_screen_buf[px_idx * 4] = r;
                self.vram_viewer.oam_screen_buf[px_idx * 4 + 1] = g;
                self.vram_viewer.oam_screen_buf[px_idx * 4 + 2] = b;
                self.vram_viewer.oam_screen_buf[px_idx * 4 + 3] = 255;
            }
        }

        for i in 0..40 {
            let buf_len = 8 * sprite_h as usize * 4;
            let tex = self.vram_viewer.oam_sprite_textures[i].get_or_insert_with(|| {
                ctx.load_texture(
                    format!("oam_sprite_{}", i),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [8, sprite_h as usize],
                        &self.vram_viewer.oam_sprite_bufs[i][..buf_len],
                    ),
                    egui::TextureOptions::NEAREST,
                )
            });
            if needs_update {
                tex.set(
                    egui::ColorImage::from_rgba_unmultiplied(
                        [8, sprite_h as usize],
                        &self.vram_viewer.oam_sprite_bufs[i][..buf_len],
                    ),
                    egui::TextureOptions::NEAREST,
                );
            }
        }

        let screen_tex = self.vram_viewer.oam_screen_tex.get_or_insert_with(|| {
            ctx.load_texture(
                "oam_screen",
                egui::ColorImage::from_rgba_unmultiplied(
                    [160, 144],
                    &self.vram_viewer.oam_screen_buf,
                ),
                egui::TextureOptions::NEAREST,
            )
        });
        if needs_update {
            screen_tex.set(
                egui::ColorImage::from_rgba_unmultiplied(
                    [160, 144],
                    &self.vram_viewer.oam_screen_buf,
                ),
                egui::TextureOptions::NEAREST,
            );
        }

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Sprites");

                let cell_w = 24.0;
                let cell_h = if is_8x16 { 48.0 } else { 28.0 };
                let cols = 10;
                let rows = 4;
                let grid_w = cols as f32 * cell_w;
                let grid_h = rows as f32 * cell_h;

                let (response, painter) =
                    ui.allocate_painter(egui::vec2(grid_w, grid_h), egui::Sense::click());
                let rect = response.rect;

                painter.rect_filled(rect, 0.0, egui::Color32::WHITE);

                for i in 0..40 {
                    let col = i % cols;
                    let row = i / cols;
                    let cell_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(col as f32 * cell_w, row as f32 * cell_h),
                        egui::vec2(cell_w, cell_h),
                    );

                    let base = i * 4;
                    let y_pos = ppu.oam[base] as i16;
                    let x_pos = ppu.oam[base + 1] as i16;

                    let screen_y = y_pos - 16;
                    let screen_x = x_pos - 8;
                    let is_offscreen = screen_x <= -8
                        || screen_x >= 160
                        || screen_y <= -(sprite_h as i16)
                        || screen_y >= 144;

                    if i == self.vram_viewer.oam_selected {
                        painter.rect_filled(cell_rect, 0.0, egui::Color32::from_rgb(180, 210, 255));
                    }

                    if let Some(tex) = &self.vram_viewer.oam_sprite_textures[i] {
                        let sprite_w = 8.0 * 2.0;
                        let sprite_h_scaled = sprite_h as f32 * 2.0;
                        let sprite_rect = egui::Rect::from_center_size(
                            cell_rect.center() + egui::vec2(0.0, 4.0),
                            egui::vec2(sprite_w, sprite_h_scaled),
                        );
                        painter.image(
                            tex.id(),
                            sprite_rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );

                        if is_offscreen {
                            let center = sprite_rect.center();
                            let size = 6.0;
                            painter.line_segment(
                                [
                                    center - egui::vec2(size, size),
                                    center + egui::vec2(size, size),
                                ],
                                egui::Stroke::new(2.0, egui::Color32::RED),
                            );
                            painter.line_segment(
                                [
                                    center + egui::vec2(-size, size),
                                    center + egui::vec2(size, -size),
                                ],
                                egui::Stroke::new(2.0, egui::Color32::RED),
                            );
                        }
                    }

                    let label_pos = egui::pos2(cell_rect.center().x, cell_rect.top() + 6.0);
                    painter.text(
                        label_pos,
                        egui::Align2::CENTER_CENTER,
                        format!("{:02}", i),
                        egui::FontId::monospace(8.0),
                        egui::Color32::DARK_GRAY,
                    );
                }

                for col in 0..=cols {
                    let x = rect.left() + col as f32 * cell_w;
                    painter.line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
                    );
                }
                for row in 0..=rows {
                    let y = rect.top() + row as f32 * cell_h;
                    painter.line_segment(
                        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                        egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
                    );
                }

                if let Some(pos) = response.interact_pointer_pos() {
                    let rel = pos - rect.min;
                    let col = (rel.x / cell_w) as usize;
                    let row = (rel.y / cell_h) as usize;
                    let idx = row * cols + col;
                    if idx < 40 {
                        self.vram_viewer.oam_selected = idx;
                    }
                }
            });

            ui.add_space(16.0);

            ui.vertical(|ui| {
                ui.heading("Screen Position");
                let scale = 1.0;
                let (screen_response, screen_painter) = ui.allocate_painter(
                    egui::vec2(160.0 * scale, 144.0 * scale),
                    egui::Sense::hover(),
                );
                let screen_rect = screen_response.rect;

                screen_painter.image(
                    screen_tex.id(),
                    screen_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );

                let sel = self.vram_viewer.oam_selected;
                let base = sel * 4;
                let y_pos = ppu.oam[base] as i16;
                let x_pos = ppu.oam[base + 1] as i16;
                let screen_y = (y_pos - 16) as f32 * scale;
                let screen_x = (x_pos - 8) as f32 * scale;
                let sprite_w = 8.0 * scale;
                let sprite_h_scaled = sprite_h as f32 * scale;

                let sprite_screen_rect = egui::Rect::from_min_size(
                    screen_rect.min + egui::vec2(screen_x, screen_y),
                    egui::vec2(sprite_w, sprite_h_scaled),
                );
                screen_painter.rect_stroke(
                    sprite_screen_rect,
                    0.0,
                    egui::Stroke::new(1.0, egui::Color32::YELLOW),
                    egui::StrokeKind::Middle,
                );

                ui.add_space(8.0);

                ui.heading("Details");
                ui.separator();

                if sel < 40 {
                    let base = sel * 4;
                    let y_pos = ppu.oam[base];
                    let x_pos = ppu.oam[base + 1];
                    let tile_num = ppu.oam[base + 2];
                    let attr = ppu.oam[base + 3];

                    let x_flip = attr & 0x20 != 0;
                    let y_flip = attr & 0x40 != 0;
                    let priority = attr & 0x80 != 0;

                    let screen_y = y_pos as i16 - 16;
                    let screen_x = x_pos as i16 - 8;

                    let oam_addr = 0xFE00 + (sel as u16) * 4;
                    let tile_addr = if is_8x16 {
                        (tile_num & 0xFE) as u16 * 16
                    } else {
                        tile_num as u16 * 16
                    };

                    egui::Grid::new("oam_details_grid")
                        .num_columns(2)
                        .spacing([8.0, 2.0])
                        .show(ui, |ui| {
                            ui.label("X coord:");
                            ui.monospace(format!("{} (${:02X})", screen_x, x_pos));
                            ui.end_row();

                            ui.label("Y coord:");
                            ui.monospace(format!("{} (${:02X})", screen_y, y_pos));
                            ui.end_row();

                            ui.label("Tile No:");
                            ui.monospace(format!("${:02X}", tile_num));
                            ui.end_row();

                            ui.label("Attribute:");
                            ui.monospace(format!("${:02X}", attr));
                            ui.end_row();

                            ui.label("   X-flip:");
                            ui.checkbox(&mut x_flip.clone(), "");
                            ui.end_row();

                            ui.label("   Y-flip:");
                            ui.checkbox(&mut y_flip.clone(), "");
                            ui.end_row();

                            ui.label("   Palette:");
                            if ppu.cgb {
                                ui.monospace(format!("OBJ {}", attr & 0x07));
                            } else {
                                ui.monospace(format!(
                                    "OBP{}",
                                    if attr & 0x10 != 0 { 1 } else { 0 }
                                ));
                            }
                            ui.end_row();

                            if ppu.cgb {
                                ui.label("   Bank:");
                                ui.monospace(format!("{}", if attr & 0x08 != 0 { 1 } else { 0 }));
                                ui.end_row();
                            }

                            ui.label("   Priority:");
                            ui.checkbox(&mut priority.clone(), "BG over OBJ");
                            ui.end_row();

                            ui.label("OAM addr:");
                            ui.monospace(format!("${:04X}", oam_addr));
                            ui.end_row();

                            ui.label("Tile addr:");
                            ui.monospace(format!("${:04X}", tile_addr));
                            ui.end_row();
                        });
                }
            });
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
                        ui.painter().rect_stroke(
                            rect,
                            0.0,
                            egui::Stroke::new(1.0, stroke_color),
                            egui::StrokeKind::Middle,
                        );
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
                        ui.painter().rect_stroke(
                            rect,
                            0.0,
                            egui::Stroke::new(1.0, stroke_color),
                            egui::StrokeKind::Middle,
                        );
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
                    egui::StrokeKind::Middle,
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

    let ui_config_path = ui_config::default_ui_config_path();
    let ui_config = ui_config::load_from_file(&ui_config_path);

    let emulation_mode = if args.dmg {
        EmulationMode::ForceDmg
    } else if args.cgb {
        EmulationMode::ForceCgb
    } else {
        ui_config.emulation_mode
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
        dmg_bootrom_path: ui_config.dmg_bootrom_path.clone(),
        cgb_bootrom_path: ui_config.cgb_bootrom_path.clone(),
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

    let bootrom_data = configured_bootrom_data(&load_config, cgb_mode);

    let model = if cgb_mode {
        Model::Cgb(CgbRevision::default())
    } else {
        Model::Dmg(DmgRevision::default())
    };

    let mut gb = if bootrom_data.is_some() {
        GameBoy::new_power_on(model)
    } else {
        GameBoy::new(model)
    };
    if let Some(data) = bootrom_data {
        gb.mmu.load_boot_rom(data);
    }
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
    let external_clock_pending = Arc::new(network_link::ExternalClockPending::default());
    let pending_timestamp = Arc::new(network_link::PendingTimestamp::default());
    let local_timestamp = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let link_doublespeed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let slave_ready = Arc::new(network_link::SlaveReadyState::default());

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
    let emu_ext_clock = Arc::clone(&external_clock_pending);
    let emu_slave_ready = Arc::clone(&slave_ready);
    let emu_link_timestamp = Arc::clone(&local_timestamp);
    let emu_link_doublespeed = Arc::clone(&link_doublespeed);

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
            emu_ext_clock,
            emu_slave_ready,
            emu_link_timestamp,
            emu_link_doublespeed,
        );
    });

    let scale = ui_config
        .window_size
        .scale_factor_px()
        .unwrap_or(DEFAULT_WINDOW_SCALE) as f32;
    let initial_size = [
        GB_WIDTH * scale,
        GB_HEIGHT * scale + MENU_BAR_HEIGHT + STATUS_BAR_HEIGHT,
    ];
    let mut wgpu_setup = egui_wgpu::WgpuSetupCreateNew::default();

    // Lavapipe (software Vulkan on Linux VMs) doesn't fully implement
    // VK_EXT_debug_utils, which wgpu requests when the DEBUG flag is set.
    wgpu_setup
        .instance_descriptor
        .flags
        .remove(wgpu::InstanceFlags::DEBUG);

    // Mesa < 22.0 lavapipe returns empty surface formats on Wayland, causing a
    // black screen. Prefer the GL (llvmpipe) adapter when the only Vulkan
    // adapter is a software/CPU device, since llvmpipe handles surfaces
    // correctly via EGL even on old Mesa. Real GPUs still get Vulkan.
    wgpu_setup.native_adapter_selector = Some(Arc::new(|adapters, surface| {
        let mut ranked: Vec<_> = adapters.iter().collect();

        ranked.sort_by_key(|a| {
            let info = a.get_info();
            let is_software = info.device_type == wgpu::DeviceType::Cpu;

            let has_surface_formats = surface
                .map(|s| !s.get_capabilities(a).formats.is_empty())
                .unwrap_or(true);

            // (primary key, secondary key) — lower is better
            let backend_rank = match (info.backend, is_software) {
                (wgpu::Backend::Vulkan, false) => 0,
                (wgpu::Backend::Metal, _) => 0,
                (wgpu::Backend::Dx12, false) => 0,
                (wgpu::Backend::Gl, _) => 1,
                (wgpu::Backend::Vulkan, true) => 2,
                (wgpu::Backend::Dx12, true) => 2,
                _ => 3,
            };

            // Adapters without surface formats are unusable for presentation
            let format_penalty: u8 = if has_surface_formats { 0 } else { 10 };
            backend_rank + format_penalty
        });

        for a in &ranked {
            let info = a.get_info();
            log::debug!(
                "wgpu adapter candidate: {:?} backend={:?} type={:?}",
                info.name,
                info.backend,
                info.device_type
            );
        }

        let selected = ranked
            .first()
            .map(|a| (*a).clone())
            .ok_or_else(|| "No suitable wgpu adapter found".to_owned());

        if let Ok(ref adapter) = selected {
            let info = adapter.get_info();
            log::info!(
                "Selected wgpu adapter: {:?} ({:?})",
                info.name,
                info.backend
            );
        }

        selected
    }));

    // Probe whether wgpu would land on a GL-backend software/virtual GPU.
    // In that scenario (e.g. VMware SVGA3D, llvmpipe) the wgpu GL present
    // path (offscreen renderbuffer + glBlitFramebuffer) often produces a
    // black screen on old Mesa. The glow renderer drives OpenGL directly
    // through glutin and works reliably in these environments.
    let renderer = {
        let probe_instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu_setup.instance_descriptor.backends,
            ..Default::default()
        });
        let adapters = probe_instance.enumerate_adapters(wgpu_setup.instance_descriptor.backends);

        let best_is_gl_software = adapters
            .iter()
            .min_by_key(|a| {
                let info = a.get_info();
                match (info.backend, info.device_type) {
                    (
                        wgpu::Backend::Vulkan,
                        wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu,
                    ) => 0,
                    (wgpu::Backend::Metal, _) | (wgpu::Backend::Dx12, _) => 0,
                    _ => 1,
                }
            })
            .map(|a| {
                let info = a.get_info();
                let is_gl = info.backend == wgpu::Backend::Gl;
                let is_software = matches!(
                    info.device_type,
                    wgpu::DeviceType::Cpu | wgpu::DeviceType::Other
                );
                is_gl || is_software
            })
            .unwrap_or(true);

        if best_is_gl_software {
            log::info!("Best wgpu adapter is GL/software — using glow renderer for reliability");
            eframe::Renderer::Glow
        } else {
            eframe::Renderer::Wgpu
        }
    };

    let native_options = eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title("vibeEmu")
            .with_inner_size(initial_size)
            .with_icon(load_window_icon().unwrap_or_default()),
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(wgpu_setup),
            ..Default::default()
        },
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
                load_config.clone(),
                ui_config_path.clone(),
                ui_config.clone(),
                external_clock_pending,
                pending_timestamp,
                local_timestamp,
                link_doublespeed,
                slave_ready,
            )))
        }),
    ) {
        error!("eframe error: {e}");
    }
}
