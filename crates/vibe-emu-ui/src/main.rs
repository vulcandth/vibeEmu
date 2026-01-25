#![allow(dead_code)]

mod audio;
mod keybinds;
mod scaler;
mod ui;
mod ui_config;

use clap::{Parser, ValueEnum};
use cpal::traits::StreamTrait;
use log::{debug, error, info, warn};
use pixels::{Pixels, SurfaceTexture};
use rfd::FileDialog;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex, Once, RwLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use vibe_emu_core::serial::{LinkPort, NullLinkPort};
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::CgbRevision, mmu::Mmu};
use vibe_emu_mobile::{
    MobileAdapter, MobileAdapterDevice, MobileAddr, MobileConfig, MobileHost, MobileLinkPort,
    MobileNumber, MobileSockType, StdMobileHost,
};
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Icon, UserAttentionType, Window, WindowAttributes};

use crossbeam_channel as cb;
use keybinds::KeyBindings;
pub use scaler::GameScaler;
use ui::snapshot::UiSnapshot;
use ui_config::{EmulationMode, UiConfig, WindowSize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum LogLevelArg {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum WgpuBackendArg {
    /// Let wgpu pick the best available backend.
    Auto,
    /// Direct3D 12 (Windows).
    Dx12,
    /// Direct3D 11 (Windows).
    Dx11,
    /// Vulkan.
    Vulkan,
    /// Metal (macOS).
    Metal,
    /// OpenGL.
    Gl,
}

impl WgpuBackendArg {
    fn as_env_value(self) -> Option<&'static str> {
        match self {
            WgpuBackendArg::Auto => None,
            WgpuBackendArg::Dx12 => Some("dx12"),
            WgpuBackendArg::Dx11 => Some("dx11"),
            WgpuBackendArg::Vulkan => Some("vulkan"),
            WgpuBackendArg::Metal => Some("metal"),
            WgpuBackendArg::Gl => Some("gl"),
        }
    }
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

    // Reduce noisy GPU backend logs. Users can override via `RUST_LOG`.
    logger.filter_module("wgpu", log::LevelFilter::Warn);
    logger.filter_module("wgpu_core", log::LevelFilter::Warn);
    logger.filter_module("wgpu_hal", log::LevelFilter::Warn);
    logger.filter_module("naga", log::LevelFilter::Warn);
    logger.format_timestamp_millis().init();
}

fn format_serial_bytes(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len());
    for &b in data {
        if b.is_ascii_graphic() || b == b' ' {
            out.push(b as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(&mut out, "\\x{b:02X}");
        }
    }
    out
}

fn load_window_icon() -> Option<Icon> {
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
    Icon::from_rgba(rgba, info.width, info.height).ok()
}

fn default_keybinds_path() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return std::path::PathBuf::from(appdata)
                .join("vibeemu")
                .join("keybinds.cfg");
        }
    }

    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return std::path::PathBuf::from(xdg)
            .join("vibeemu")
            .join("keybinds.cfg");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return std::path::PathBuf::from(home)
            .join(".config")
            .join("vibeemu")
            .join("keybinds.cfg");
    }

    std::path::PathBuf::from("keybinds.cfg")
}

const DEFAULT_WINDOW_SCALE: u32 = 2;
// Tool windows use a fixed (non-configurable) default scale.
const SCALE: u32 = DEFAULT_WINDOW_SCALE;
// The top menu bar consumes vertical space; ensure the default window is tall enough
// to still fit a 2x (or SCALEx) Game Boy frame below it.
const DEFAULT_MENU_BAR_HEIGHT_PX: u32 = 32;
const GB_FPS: f64 = 59.7275;
const FRAME_TIME: Duration = Duration::from_nanos((1e9_f64 / GB_FPS) as u64);
const FF_MULT: f32 = 4.0;
const AUDIO_WARMUP_TARGET_RATIO: f32 = 0.9;
const AUDIO_WARMUP_CHECK_INTERVAL: u32 = 1024;
const AUDIO_WARMUP_TIMEOUT_MS: u64 = 200;

#[derive(Default)]
struct UiState {
    paused: bool,
    spawn_debugger: bool,
    spawn_vram: bool,
    spawn_watchpoints: bool,
    spawn_options: bool,
    pending_exit: bool,
    pending_action: Option<UiAction>,
    pending_pause: Option<bool>,
    pending_load_config_update: bool,
    pending_save_ui_config: bool,
    pending_window_size: Option<WindowSize>,
    current_rom_path: Option<std::path::PathBuf>,
    bootrom_edit_initialized: bool,
    dmg_bootrom_edit: String,
    cgb_bootrom_edit: String,
    menu_pause_active: bool,
    menu_resume_armed: bool,
    rebinding: Option<RebindTarget>,
    serial_peripheral: SerialPeripheral,
    pending_serial_peripheral: Option<SerialPeripheral>,
    last_main_inner_size: Option<(u32, u32)>,

    debugger: ui::debugger::DebuggerState,
    watchpoints: ui::watchpoints::WatchpointsState,
    debugger_focus_pause_active: bool,
    debugger_focus_resume_armed: bool,
    debugger_pending_focus: bool,

    key_modifiers: winit::keyboard::ModifiersState,

    debugger_animate_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RebindTarget {
    Joypad(u8),
    Pause,
    FastForward,
    Quit,
}

enum UiAction {
    Reset,
    LoadPath(std::path::PathBuf),
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum SerialPeripheral {
    #[default]
    None,
    MobileAdapter,
}

#[derive(Clone)]
enum EmuCommand {
    SetPaused(bool),
    Resume {
        ignore_breakpoints: bool,
    },
    ResumeIgnoreOnce {
        breakpoint: ui::debugger::BreakpointSpec,
    },
    SetAnimate(bool),
    Step {
        count: u32,
        cmd_id: Option<u64>,
        guarantee_snapshot: bool,
    },
    RunTo {
        target: ui::debugger::BreakpointSpec,
        ignore_breakpoints: bool,
    },
    JumpTo {
        addr: u16,
    },
    CallCursor {
        addr: u16,
    },
    JumpSp,
    SetBreakpoints(Vec<ui::debugger::BreakpointSpec>),
    SetWatchpoints(Vec<vibe_emu_core::watchpoints::Watchpoint>),
    SetSpeed(Speed),
    UpdateInput(u8),
    SetSerialPeripheral(SerialPeripheral),
    UpdateLoadConfig(LoadConfig),
    Shutdown,
}

enum UiToEmu {
    Command(EmuCommand),
    Action(UiAction),
}

#[derive(Clone, Debug)]
enum UserEvent {
    EmuWake,
    DebuggerWake,
    DebuggerBreak {
        bank: u8,
        addr: u16,
    },
    DebuggerWatchpoint {
        hit: vibe_emu_core::watchpoints::WatchpointHit,
    },
    DebuggerAck {
        cmd_id: u64,
    },
}

struct EmuThreadChannels {
    rx: mpsc::Receiver<UiToEmu>,
    frame_tx: cb::Sender<EmuEvent>,
    serial_tx: cb::Sender<EmuEvent>,
    frame_pool_tx: cb::Sender<Vec<u32>>,
    frame_pool_rx: cb::Receiver<Vec<u32>>,
    wake_proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    exec_trace: Arc<Mutex<Vec<ui::code_data::ExecutedInstruction>>>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
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

enum EmuEvent {
    Frame { frame: Vec<u32>, frame_index: u64 },
    Serial { data: Vec<u8>, frame_index: u64 },
}

use ui::window::{UiWindow, WindowKind};

#[derive(Parser)]
struct Args {
    /// Path to ROM file
    rom: Option<std::path::PathBuf>,

    /// Force DMG mode
    #[arg(long, conflicts_with = "cgb")]
    dmg: bool,

    /// Use a neutral (non-green) DMG palette
    #[arg(long)]
    dmg_neutral: bool,

    /// Force CGB mode
    #[arg(long, conflicts_with = "dmg")]
    cgb: bool,

    /// Run in serial test mode
    #[arg(long)]
    serial: bool,

    /// Path to boot ROM file
    #[arg(long)]
    bootrom: Option<std::path::PathBuf>,

    /// Enable debug logging of CPU state and serial output
    #[arg(long)]
    debug: bool,

    /// Logging verbosity (release builds default to `off`)
    #[arg(long, value_enum)]
    log_level: Option<LogLevelArg>,

    /// Select wgpu backend.
    ///
    /// When omitted, the UI defaults to a stable backend per-platform.
    /// On Windows this currently prefers D3D12; pass `--wgpu-backend auto` to disable.
    #[arg(long, value_enum)]
    wgpu_backend: Option<WgpuBackendArg>,

    /// Run without opening a window
    #[arg(long)]
    headless: bool,

    /// Number of frames to run in headless mode
    #[arg(long)]
    frames: Option<usize>,

    /// Number of seconds to run in headless mode
    #[arg(long)]
    seconds: Option<u64>,

    /// Number of CPU cycles to run in headless mode
    #[arg(long)]
    cycles: Option<u64>,

    /// Enable Mobile Adapter GB emulation via libmobile
    #[arg(long)]
    mobile: bool,

    /// Path to the persisted MOBILE_CONFIG_SIZE blob (defaults next to ROM)
    #[arg(long)]
    mobile_config: Option<std::path::PathBuf>,

    /// Adapter model to emulate
    #[arg(long, value_enum, default_value_t = MobileDeviceArg::Blue)]
    mobile_device: MobileDeviceArg,

    /// Mark the connection as unmetered (used by some games)
    #[arg(long)]
    mobile_unmetered: bool,

    /// Override DNS server 1 as ip:port (e.g. `8.8.8.8:53` or `[2001:4860:4860::8888]:53`)
    #[arg(long)]
    mobile_dns1: Option<String>,

    /// Override DNS server 2 as ip:port
    #[arg(long)]
    mobile_dns2: Option<String>,

    /// Override relay server as ip:port
    #[arg(long)]
    mobile_relay: Option<String>,

    /// Override P2P port (defaults to libmobile's default)
    #[arg(long)]
    mobile_p2p_port: Option<u16>,

    /// Emit Mobile Adapter diagnostics (raw serial bytes + libmobile debug + socket events)
    #[arg(long)]
    mobile_diag: bool,

    /// Path to a keybind configuration file (see README/UI_TODO for format)
    #[arg(long)]
    keybinds: Option<std::path::PathBuf>,
}

struct DiagMobileHost {
    inner: Box<dyn MobileHost>,
}

impl DiagMobileHost {
    fn new(inner: Box<dyn MobileHost>) -> Self {
        Self { inner }
    }
}

impl MobileHost for DiagMobileHost {
    fn debug_log(&mut self, line: &str) {
        info!("[MOBILE] {line}");
        self.inner.debug_log(line);
    }

    fn update_number(&mut self, which: MobileNumber, number: Option<&str>) {
        info!("[MOBILE] update_number {:?} -> {:?}", which, number);
        self.inner.update_number(which, number);
    }

    fn config_read(&mut self, dest: &mut [u8], offset: usize) -> bool {
        self.inner.config_read(dest, offset)
    }

    fn config_write(&mut self, src: &[u8], offset: usize) -> bool {
        self.inner.config_write(src, offset)
    }

    fn sock_open(
        &mut self,
        conn: u32,
        socktype: MobileSockType,
        addr: &MobileAddr,
        bind_port: u16,
    ) -> bool {
        let ok = self.inner.sock_open(conn, socktype, addr, bind_port);
        info!(
            "[MOBILE] sock_open conn={} type={:?} addr={:?} bind_port={} -> {}",
            conn, socktype, addr, bind_port, ok
        );
        ok
    }

    fn sock_close(&mut self, conn: u32) {
        info!("[MOBILE] sock_close conn={conn}");
        self.inner.sock_close(conn);
    }

    fn sock_connect(&mut self, conn: u32, addr: &MobileAddr) -> i32 {
        let rc = self.inner.sock_connect(conn, addr);
        info!(
            "[MOBILE] sock_connect conn={} addr={:?} -> {}",
            conn, addr, rc
        );
        rc
    }

    fn sock_listen(&mut self, conn: u32) -> bool {
        let ok = self.inner.sock_listen(conn);
        info!("[MOBILE] sock_listen conn={} -> {}", conn, ok);
        ok
    }

    fn sock_accept(&mut self, conn: u32) -> bool {
        let ok = self.inner.sock_accept(conn);
        info!("[MOBILE] sock_accept conn={} -> {}", conn, ok);
        ok
    }

    fn sock_send(&mut self, conn: u32, data: &[u8], addr: Option<&MobileAddr>) -> i32 {
        let rc = self.inner.sock_send(conn, data, addr);
        info!(
            "[MOBILE] sock_send conn={} len={} addr={:?} -> {}",
            conn,
            data.len(),
            addr,
            rc
        );
        rc
    }

    fn sock_recv(
        &mut self,
        conn: u32,
        mut data: Option<&mut [u8]>,
        mut addr_out: Option<&mut MobileAddr>,
    ) -> i32 {
        let rc = self
            .inner
            .sock_recv(conn, data.as_deref_mut(), addr_out.as_deref_mut());

        if rc > 0 {
            let n = rc as usize;
            let preview_len = n.min(32);
            match (data.as_deref(), addr_out.as_deref()) {
                (Some(buf), Some(addr)) => {
                    info!(
                        "[MOBILE] sock_recv conn={} -> {} bytes from {:?} (first {:02X?}{})",
                        conn,
                        rc,
                        addr,
                        &buf[..preview_len],
                        if n > preview_len { "…" } else { "" }
                    );
                }
                (Some(buf), None) => {
                    info!(
                        "[MOBILE] sock_recv conn={} -> {} bytes (first {:02X?}{})",
                        conn,
                        rc,
                        &buf[..preview_len],
                        if n > preview_len { "…" } else { "" }
                    );
                }
                _ => {
                    info!("[MOBILE] sock_recv conn={} -> {} bytes", conn, rc);
                }
            }
        } else {
            info!("[MOBILE] sock_recv conn={} -> {}", conn, rc);
        }

        rc
    }
}

struct DiagMobileLinkPort {
    adapter: Arc<Mutex<MobileAdapter>>,
}

impl DiagMobileLinkPort {
    fn new(adapter: Arc<Mutex<MobileAdapter>>) -> Self {
        Self { adapter }
    }
}

impl LinkPort for DiagMobileLinkPort {
    fn transfer(&mut self, byte: u8) -> u8 {
        let mut adapter = match self.adapter.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("mobile adapter mutex poisoned; recovering");
                poisoned.into_inner()
            }
        };
        let rx = adapter.transfer_byte(byte).unwrap_or(0xFF);
        debug!("[MOBILE][SIO] tx={:02X} rx={:02X}", byte, rx);
        rx
    }
}

fn parse_mobile_addr(s: &str) -> Result<MobileAddr, String> {
    let sock: std::net::SocketAddr = s
        .parse()
        .map_err(|e| format!("invalid socket address '{s}': {e}"))?;

    Ok(match sock {
        std::net::SocketAddr::V4(v4) => MobileAddr::V4 {
            host: v4.ip().octets(),
            port: v4.port(),
        },
        std::net::SocketAddr::V6(v6) => MobileAddr::V6 {
            host: v6.ip().octets(),
            port: v6.port(),
        },
    })
}

#[cfg(target_os = "windows")]
fn enforce_square_corners(attrs: WindowAttributes) -> WindowAttributes {
    use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};

    attrs.with_corner_preference(CornerPreference::DoNotRound)
}

#[cfg(not(target_os = "windows"))]
fn enforce_square_corners(attrs: WindowAttributes) -> WindowAttributes {
    attrs
}

fn desired_main_inner_size(scale: u32, top_padding_px: u32) -> winit::dpi::PhysicalSize<u32> {
    winit::dpi::PhysicalSize::new(160 * scale, top_padding_px + 144 * scale)
}

fn enforce_main_window_inner_size(
    ui_state: &mut UiState,
    window: &winit::window::Window,
    scale: u32,
    top_padding_px: u32,
) -> bool {
    let desired = desired_main_inner_size(scale, top_padding_px);
    let desired_pair = (desired.width, desired.height);
    if ui_state.last_main_inner_size == Some(desired_pair) {
        return false;
    }

    ui_state.last_main_inner_size = Some(desired_pair);
    let _ = window.request_inner_size(desired);
    true
}

fn request_attention_and_focus(window: &winit::window::Window) {
    window.set_minimized(false);
    window.request_user_attention(Some(UserAttentionType::Critical));
    window.focus_window();
}

fn spawn_debugger_window(
    event_loop: &ActiveEventLoop,
    windows: &mut HashMap<winit::window::WindowId, UiWindow>,
) {
    use winit::dpi::LogicalSize;
    let attrs = enforce_square_corners(
        Window::default_attributes()
            .with_title("vibeEmu \u{2013} Debugger")
            .with_window_icon(load_window_icon())
            .with_inner_size(LogicalSize::new(900.0, 700.0)),
    );
    let w = match event_loop.create_window(attrs) {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create debugger window: {e}");
            return;
        }
    };

    let size = w.inner_size();
    let surface = pixels::SurfaceTexture::new(size.width, size.height, &w);
    // Tool windows don't render a dedicated pixel framebuffer; Pixels is used as the wgpu surface
    // carrier for ImGui rendering, so a 1×1 buffer is sufficient.
    const IMGUI_CARRIER_BUFFER: (u32, u32) = (1, 1);
    let pixels = match pixels::Pixels::new(IMGUI_CARRIER_BUFFER.0, IMGUI_CARRIER_BUFFER.1, surface)
    {
        Ok(p) => p,
        Err(e) => {
            error!("Pixels init failed (debugger window): {e}");
            return;
        }
    };

    let ui_win = UiWindow::new(WindowKind::Debugger, w, pixels, IMGUI_CARRIER_BUFFER);
    let id = ui_win.win.id();
    windows.insert(id, ui_win);
    if let Some(win) = windows.get_mut(&id) {
        win.resize(win.win.inner_size());
    }
}

fn spawn_vram_window(
    event_loop: &ActiveEventLoop,
    windows: &mut HashMap<winit::window::WindowId, UiWindow>,
) {
    use winit::dpi::LogicalSize;
    let attrs = enforce_square_corners(
        Window::default_attributes()
            .with_title("vibeEmu \u{2013} VRAM")
            .with_window_icon(load_window_icon())
            .with_inner_size(LogicalSize::new(640.0, 600.0)),
    );
    let w = match event_loop.create_window(attrs) {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create VRAM window: {e}");
            return;
        }
    };

    let size = w.inner_size();
    let surface = pixels::SurfaceTexture::new(size.width, size.height, &w);
    // Tool windows don't render a dedicated pixel framebuffer; Pixels is used as the wgpu surface
    // carrier for ImGui rendering, so a 1×1 buffer is sufficient.
    const IMGUI_CARRIER_BUFFER: (u32, u32) = (1, 1);
    let pixels = match pixels::Pixels::new(IMGUI_CARRIER_BUFFER.0, IMGUI_CARRIER_BUFFER.1, surface)
    {
        Ok(p) => p,
        Err(e) => {
            error!("Pixels init failed (VRAM window): {e}");
            return;
        }
    };

    let ui_win = UiWindow::new(WindowKind::VramViewer, w, pixels, IMGUI_CARRIER_BUFFER);
    let id = ui_win.win.id();
    windows.insert(id, ui_win);
    if let Some(win) = windows.get_mut(&id) {
        win.resize(win.win.inner_size());
    }
}

fn spawn_options_window(
    event_loop: &ActiveEventLoop,
    windows: &mut HashMap<winit::window::WindowId, UiWindow>,
) {
    use winit::dpi::LogicalSize;
    let attrs = enforce_square_corners(
        Window::default_attributes()
            .with_title("vibeEmu \u{2013} Options")
            .with_window_icon(load_window_icon())
            .with_inner_size(LogicalSize::new(520.0, 420.0)),
    );
    let w = match event_loop.create_window(attrs) {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create Options window: {e}");
            return;
        }
    };

    let size = w.inner_size();
    let surface = pixels::SurfaceTexture::new(size.width, size.height, &w);
    // Tool windows don't render a dedicated pixel framebuffer; Pixels is used as the wgpu surface
    // carrier for ImGui rendering, so a 1×1 buffer is sufficient.
    const IMGUI_CARRIER_BUFFER: (u32, u32) = (1, 1);
    let pixels = match pixels::Pixels::new(IMGUI_CARRIER_BUFFER.0, IMGUI_CARRIER_BUFFER.1, surface)
    {
        Ok(p) => p,
        Err(e) => {
            error!("Pixels init failed (Options window): {e}");
            return;
        }
    };

    let ui_win = UiWindow::new(WindowKind::Options, w, pixels, IMGUI_CARRIER_BUFFER);
    let id = ui_win.win.id();
    windows.insert(id, ui_win);
    if let Some(win) = windows.get_mut(&id) {
        win.resize(win.win.inner_size());
    }
}

fn spawn_watchpoints_window(
    event_loop: &ActiveEventLoop,
    windows: &mut HashMap<winit::window::WindowId, UiWindow>,
) {
    use winit::dpi::LogicalSize;
    let attrs = enforce_square_corners(
        Window::default_attributes()
            .with_title("vibeEmu \u{2013} Watchpoints")
            .with_window_icon(load_window_icon())
            .with_inner_size(LogicalSize::new(520.0, 520.0)),
    );
    let w = match event_loop.create_window(attrs) {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create Watchpoints window: {e}");
            return;
        }
    };

    let size = w.inner_size();
    let surface = pixels::SurfaceTexture::new(size.width, size.height, &w);
    // Tool windows don't render a dedicated pixel framebuffer; Pixels is used as the wgpu surface
    // carrier for ImGui rendering, so a 1×1 buffer is sufficient.
    const IMGUI_CARRIER_BUFFER: (u32, u32) = (1, 1);
    let pixels = match pixels::Pixels::new(IMGUI_CARRIER_BUFFER.0, IMGUI_CARRIER_BUFFER.1, surface)
    {
        Ok(p) => p,
        Err(e) => {
            error!("Pixels init failed (Watchpoints window): {e}");
            return;
        }
    };

    let ui_win = UiWindow::new(WindowKind::Watchpoints, w, pixels, IMGUI_CARRIER_BUFFER);
    let id = ui_win.win.id();
    windows.insert(id, ui_win);
    if let Some(win) = windows.get_mut(&id) {
        win.resize(win.win.inner_size());
    }
}

#[allow(clippy::too_many_arguments)]
fn run_emulator_thread(
    gb: Arc<Mutex<GameBoy>>,
    ui_snapshot: Arc<RwLock<UiSnapshot>>,
    mut speed: Speed,
    initial_paused: bool,
    debug: bool,
    mobile: Option<Arc<Mutex<MobileAdapter>>>,
    mut serial_peripheral: SerialPeripheral,
    mobile_diag: bool,
    load_config: LoadConfig,
    channels: EmuThreadChannels,
) {
    let EmuThreadChannels {
        rx,
        frame_tx,
        serial_tx,
        frame_pool_tx,
        frame_pool_rx,
        wake_proxy,
        exec_trace,
    } = channels;

    let mut exec_seen_rom0 = vec![false; 0x4000];
    let mut exec_seen_romx: Vec<Option<Vec<bool>>> = vec![None; 256];
    let mut pending_exec_trace: Vec<ui::code_data::ExecutedInstruction> = Vec::new();

    let flush_exec_trace = |pending: &mut Vec<ui::code_data::ExecutedInstruction>| {
        if pending.is_empty() {
            return;
        }

        if let Ok(mut buf) = exec_trace.lock() {
            buf.extend(pending.drain(..));
        }
    };

    let note_execute_pc =
        |pc: u16,
         bank: u8,
         mmu: &mut Mmu,
         seen_rom0: &mut [bool],
         seen_romx: &mut [Option<Vec<bool>>],
         pending: &mut Vec<ui::code_data::ExecutedInstruction>| {
            if pc >= 0x8000 {
                return;
            }

            let (bank, idx) = if pc < 0x4000 {
                (0u8, pc as usize)
            } else {
                (bank, (pc - 0x4000) as usize)
            };

            if pc < 0x4000 {
                if seen_rom0.get(idx).copied().unwrap_or(false) {
                    return;
                }
                if let Some(slot) = seen_rom0.get_mut(idx) {
                    *slot = true;
                }
            } else {
                let bank_slot = seen_romx
                    .get_mut(bank as usize)
                    .expect("ROMX bank index out of range");
                if bank_slot.is_none() {
                    *bank_slot = Some(vec![false; 0x4000]);
                }
                let bank_map = bank_slot.as_mut().expect("ROMX bank map missing");
                if bank_map.get(idx).copied().unwrap_or(false) {
                    return;
                }
                if let Some(slot) = bank_map.get_mut(idx) {
                    *slot = true;
                }
            }

            let opcode = mmu.read_byte(pc);
            let len = ui::code_data::sm83_instr_len(opcode);
            pending.push(ui::code_data::ExecutedInstruction {
                bank,
                addr: pc,
                len,
            });
        };

    fn exec_break_key_for_pc(pc: u16, mmu: &Mmu) -> ui::debugger::BreakpointSpec {
        let active_rom_bank = mmu.cart.as_ref().map(|c| c.current_rom_bank()).unwrap_or(1);
        let vram_bank = mmu.ppu.vram_bank as u8;
        let wram_bank = mmu.wram_bank as u8;
        let sram_bank = mmu.cart.as_ref().map(|c| c.current_ram_bank()).unwrap_or(0);

        let bank = match pc {
            0x0000..=0x3FFF => 0,
            0x4000..=0x7FFF => active_rom_bank.min(0xFF) as u8,
            0x8000..=0x9FFF => vram_bank,
            0xA000..=0xBFFF => sram_bank,
            0xC000..=0xCFFF => 0,
            0xD000..=0xDFFF => wram_bank,
            0xE000..=0xEFFF => 0,
            0xF000..=0xFDFF => wram_bank,
            _ => 0,
        };

        ui::debugger::BreakpointSpec { bank, addr: pc }
    }

    fn apply_serial_peripheral(
        gb: &mut GameBoy,
        mobile: &Option<Arc<Mutex<MobileAdapter>>>,
        desired: SerialPeripheral,
        mobile_diag: bool,
        serial_peripheral: &mut SerialPeripheral,
        mobile_active: &mut bool,
        mobile_time_accum_ns: &mut u128,
    ) {
        match desired {
            SerialPeripheral::None => {
                if let Some(mobile) = mobile.as_ref()
                    && let Ok(mut adapter) = mobile.lock()
                {
                    let _ = adapter.stop();
                }
                gb.mmu.serial.connect(Box::new(NullLinkPort::default()));
                *mobile_active = false;
                *serial_peripheral = SerialPeripheral::None;
            }
            SerialPeripheral::MobileAdapter => {
                let Some(mobile) = mobile.as_ref() else {
                    warn!("Mobile Adapter requested but unavailable");
                    gb.mmu.serial.connect(Box::new(NullLinkPort::default()));
                    *mobile_active = false;
                    *serial_peripheral = SerialPeripheral::None;
                    *mobile_time_accum_ns = 0;
                    return;
                };

                if let Ok(mut adapter) = mobile.lock() {
                    let _ = adapter.stop();
                    if let Err(e) = adapter.start() {
                        warn!("Failed to start Mobile Adapter: {e}");
                        gb.mmu.serial.connect(Box::new(NullLinkPort::default()));
                        *mobile_active = false;
                        *serial_peripheral = SerialPeripheral::None;
                        *mobile_time_accum_ns = 0;
                        return;
                    }
                }

                if mobile_diag {
                    gb.mmu
                        .serial
                        .connect(Box::new(DiagMobileLinkPort::new(Arc::clone(mobile))));
                } else {
                    gb.mmu
                        .serial
                        .connect(Box::new(MobileLinkPort::new(Arc::clone(mobile))));
                }

                *mobile_active = true;
                *serial_peripheral = SerialPeripheral::MobileAdapter;
            }
        }

        *mobile_time_accum_ns = 0;
    }

    let mut load_config = load_config;
    let mut paused = initial_paused;
    let mut step_budget: u32 = 0;
    let mut pending_debug_ack: Option<u64> = None;
    let mut breakpoints: std::collections::HashSet<ui::debugger::BreakpointSpec> =
        std::collections::HashSet::new();
    let mut watchpoints: Vec<vibe_emu_core::watchpoints::Watchpoint> = Vec::new();
    let mut temp_exec_break: Option<ui::debugger::BreakpointSpec> = None;
    let mut ignore_breakpoints = false;
    let mut ignore_once_breakpoint: Option<ui::debugger::BreakpointSpec> = None;
    let mut watchpoints_suspended = false;
    let mut animate = false;
    let mut frame_count = 0u64;
    let mut next_frame = Instant::now() + FRAME_TIME;
    let mut audio_stream = None;
    let mut mobile_time_accum_ns: u128 = 0;
    let mut mobile_active = serial_peripheral == SerialPeripheral::MobileAdapter;

    if let Ok(mut gb) = gb.lock() {
        apply_serial_peripheral(
            &mut gb,
            &mobile,
            serial_peripheral,
            mobile_diag,
            &mut serial_peripheral,
            &mut mobile_active,
            &mut mobile_time_accum_ns,
        );
        gb.mmu.watchpoints.set_watchpoints(watchpoints.clone());
        gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
        if !cfg!(test) && gb.mmu.cart.is_some() {
            rebuild_audio_stream(&mut gb, speed, &mut audio_stream);
        }
    }

    loop {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                UiToEmu::Command(cmd) => match cmd {
                    EmuCommand::SetPaused(p) => {
                        paused = p;
                        next_frame = Instant::now() + FRAME_TIME;

                        ignore_breakpoints = false;
                        ignore_once_breakpoint = None;
                        watchpoints_suspended = false;
                        animate = false;

                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                        }

                        if paused {
                            temp_exec_break = None;
                        }

                        if paused {
                            if let Ok(mut gb) = gb.lock()
                                && let Ok(mut snap) = ui_snapshot.write()
                            {
                                *snap = UiSnapshot::from_gb(&mut gb, true);
                            }
                            let _ = wake_proxy.send_event(UserEvent::DebuggerWake);
                        }
                    }
                    EmuCommand::Resume {
                        ignore_breakpoints: ignore,
                    } => {
                        paused = false;
                        ignore_breakpoints = ignore;
                        watchpoints_suspended = ignore;
                        next_frame = Instant::now() + FRAME_TIME;
                        ignore_once_breakpoint = None;

                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                        }
                    }
                    EmuCommand::ResumeIgnoreOnce { breakpoint } => {
                        paused = false;
                        ignore_breakpoints = false;
                        ignore_once_breakpoint = Some(breakpoint);
                        watchpoints_suspended = false;
                        next_frame = Instant::now() + FRAME_TIME;

                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                        }
                    }
                    EmuCommand::SetAnimate(value) => {
                        animate = value;
                    }
                    EmuCommand::Step {
                        count,
                        cmd_id,
                        guarantee_snapshot,
                    } => {
                        step_budget = step_budget.saturating_add(count);
                        paused = true;
                        ignore_breakpoints = false;
                        watchpoints_suspended = false;
                        next_frame = Instant::now() + FRAME_TIME;
                        ignore_once_breakpoint = None;
                        animate = false;
                        temp_exec_break = None;

                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                        }

                        if guarantee_snapshot {
                            pending_debug_ack = cmd_id;
                        }
                    }
                    EmuCommand::RunTo {
                        target,
                        ignore_breakpoints: ignore,
                    } => {
                        temp_exec_break = Some(target);
                        paused = false;
                        ignore_breakpoints = ignore;
                        watchpoints_suspended = ignore;
                        ignore_once_breakpoint = None;
                        next_frame = Instant::now() + FRAME_TIME;

                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                        }
                    }
                    EmuCommand::JumpTo { addr } => {
                        paused = true;
                        ignore_breakpoints = false;
                        ignore_once_breakpoint = None;
                        watchpoints_suspended = false;
                        animate = false;
                        next_frame = Instant::now() + FRAME_TIME;
                        temp_exec_break = None;
                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                            gb.cpu.pc = addr;
                            gb.cpu.halted = false;
                            if let Ok(mut snap) = ui_snapshot.write() {
                                *snap = UiSnapshot::from_gb(&mut gb, true);
                            }
                        }
                        let _ = wake_proxy.send_event(UserEvent::DebuggerWake);
                    }
                    EmuCommand::CallCursor { addr } => {
                        paused = true;
                        ignore_breakpoints = false;
                        ignore_once_breakpoint = None;
                        watchpoints_suspended = false;
                        animate = false;
                        next_frame = Instant::now() + FRAME_TIME;
                        temp_exec_break = None;
                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                            let ret = gb.cpu.pc;
                            let sp_hi = gb.cpu.sp.wrapping_sub(1);
                            let sp_lo = gb.cpu.sp.wrapping_sub(2);
                            gb.mmu.write_byte(sp_hi, (ret >> 8) as u8);
                            gb.mmu.write_byte(sp_lo, (ret & 0xFF) as u8);
                            gb.cpu.sp = sp_lo;
                            gb.cpu.pc = addr;
                            gb.cpu.halted = false;
                            if let Ok(mut snap) = ui_snapshot.write() {
                                *snap = UiSnapshot::from_gb(&mut gb, true);
                            }
                        }
                        let _ = wake_proxy.send_event(UserEvent::DebuggerWake);
                    }
                    EmuCommand::JumpSp => {
                        paused = true;
                        ignore_breakpoints = false;
                        ignore_once_breakpoint = None;
                        watchpoints_suspended = false;
                        animate = false;
                        next_frame = Instant::now() + FRAME_TIME;
                        temp_exec_break = None;
                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                            let sp = gb.cpu.sp;
                            let lo = gb.mmu.read_byte(sp);
                            let hi = gb.mmu.read_byte(sp.wrapping_add(1));
                            let target = u16::from_le_bytes([lo, hi]);
                            gb.cpu.sp = sp.wrapping_add(2);
                            gb.cpu.pc = target;
                            gb.cpu.halted = false;
                            if let Ok(mut snap) = ui_snapshot.write() {
                                *snap = UiSnapshot::from_gb(&mut gb, true);
                            }
                        }
                        let _ = wake_proxy.send_event(UserEvent::DebuggerWake);
                    }
                    EmuCommand::SetBreakpoints(list) => {
                        breakpoints.clear();
                        breakpoints.extend(list);
                    }
                    EmuCommand::SetWatchpoints(list) => {
                        watchpoints = list;
                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.watchpoints.set_watchpoints(watchpoints.clone());
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                        }
                    }
                    EmuCommand::SetSpeed(new_speed) => {
                        speed = new_speed;
                        next_frame = Instant::now() + FRAME_TIME;
                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.apu.set_speed(speed.factor);
                        }
                    }
                    EmuCommand::UpdateInput(state) => {
                        if let Ok(mut gb) = gb.lock() {
                            let mmu = &mut gb.mmu;
                            let if_reg = &mut mmu.if_reg;
                            mmu.input.update_state(state, if_reg);
                        }
                    }
                    EmuCommand::SetSerialPeripheral(peripheral) => {
                        if let Ok(mut gb) = gb.lock() {
                            apply_serial_peripheral(
                                &mut gb,
                                &mobile,
                                peripheral,
                                mobile_diag,
                                &mut serial_peripheral,
                                &mut mobile_active,
                                &mut mobile_time_accum_ns,
                            );
                        }
                    }
                    EmuCommand::UpdateLoadConfig(new_cfg) => {
                        load_config = new_cfg;
                    }
                    EmuCommand::Shutdown => {
                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.save_cart_ram();
                        }
                        if let Some(mobile) = mobile.as_ref()
                            && let Ok(mut adapter) = mobile.lock()
                        {
                            let _ = adapter.stop();
                        }
                        return;
                    }
                },
                UiToEmu::Action(action) => {
                    if let Ok(mut gb) = gb.lock() {
                        apply_ui_action(action, &mut gb, &mut audio_stream, speed, &load_config);
                        // GameBoy::reset rebuilds the MMU (including Serial), so restore the
                        // currently selected serial peripheral after any reset/load.
                        apply_serial_peripheral(
                            &mut gb,
                            &mobile,
                            serial_peripheral,
                            mobile_diag,
                            &mut serial_peripheral,
                            &mut mobile_active,
                            &mut mobile_time_accum_ns,
                        );
                        gb.mmu.watchpoints.set_watchpoints(watchpoints.clone());
                        gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                        gb.mmu.ppu.clear_frame_flag();
                        frame_count = 0;
                        next_frame = Instant::now() + FRAME_TIME;
                    }
                }
            }
        }

        if paused {
            if step_budget == 0 {
                thread::sleep(Duration::from_millis(1));
                continue;
            }

            let want_guaranteed = pending_debug_ack.is_some() && step_budget == 1;

            let mut step_watch_hit: Option<vibe_emu_core::watchpoints::WatchpointHit> = None;

            if let Ok(mut gb) = gb.lock() {
                let (cpu, mmu) = {
                    let GameBoy { cpu, mmu, .. } = &mut *gb;
                    (cpu, mmu)
                };

                let pre_pc = cpu.pc;
                let pre_exec_hit = if !watchpoints_suspended {
                    watchpoints.iter().find(|wp| {
                        wp.enabled
                            && wp.on_execute
                            && wp.matches_addr(pre_pc)
                            && wp.matches_value(None)
                    })
                } else {
                    None
                };

                let pre_fallthrough = {
                    let was = mmu.watchpoints.suspended();
                    if !was {
                        mmu.watchpoints.set_suspended(true);
                    }
                    let opcode = mmu.read_byte(pre_pc);
                    if !was {
                        mmu.watchpoints.set_suspended(false);
                    }
                    let len = ui::code_data::sm83_instr_len(opcode) as u16;
                    pre_pc.wrapping_add(len)
                };

                let romx_bank = mmu.cart.as_ref().map(|c| c.current_rom_bank()).unwrap_or(1);
                let bank = romx_bank.min(0xFF) as u8;
                let was = mmu.watchpoints.suspended();
                if !was {
                    mmu.watchpoints.set_suspended(true);
                }
                note_execute_pc(
                    pre_pc,
                    bank,
                    mmu,
                    &mut exec_seen_rom0,
                    &mut exec_seen_romx,
                    &mut pending_exec_trace,
                );
                if !was {
                    mmu.watchpoints.set_suspended(false);
                }
                cpu.step(mmu);

                if let Some(hit) = mmu.watchpoints.take_hit() {
                    step_watch_hit = Some(hit);
                } else if !watchpoints_suspended && cpu.pc != pre_fallthrough {
                    let dest = cpu.pc;
                    if let Some(wp) = watchpoints.iter().find(|wp| {
                        wp.enabled && wp.on_jump && wp.matches_addr(dest) && wp.matches_value(None)
                    }) {
                        step_watch_hit = Some(vibe_emu_core::watchpoints::WatchpointHit {
                            id: wp.id,
                            trigger: vibe_emu_core::watchpoints::WatchpointTrigger::Jump,
                            addr: dest,
                            value: None,
                            pc: Some(pre_pc),
                        });
                    }
                } else if let Some(wp) = pre_exec_hit {
                    step_watch_hit = Some(vibe_emu_core::watchpoints::WatchpointHit {
                        id: wp.id,
                        trigger: vibe_emu_core::watchpoints::WatchpointTrigger::Execute,
                        addr: pre_pc,
                        value: None,
                        pc: Some(pre_pc),
                    });
                }

                if want_guaranteed {
                    if let Ok(mut snap) = ui_snapshot.write() {
                        *snap = UiSnapshot::from_gb(&mut gb, true);
                    }
                } else if let Ok(mut snap) = ui_snapshot.try_write() {
                    *snap = UiSnapshot::from_gb(&mut gb, true);
                }
            }

            step_budget = step_budget.saturating_sub(1);

            flush_exec_trace(&mut pending_exec_trace);

            if want_guaranteed && let Some(cmd_id) = pending_debug_ack.take() {
                let _ = wake_proxy.send_event(UserEvent::DebuggerAck { cmd_id });
            }
            if let Some(hit) = step_watch_hit {
                let _ = wake_proxy.send_event(UserEvent::DebuggerWatchpoint { hit });
            }
            let _ = wake_proxy.send_event(UserEvent::DebuggerWake);
            continue;
        }

        let frame_start = Instant::now();
        let mut frame_buf: Option<Vec<u32>> = None;
        let mut serial = None;

        if let Ok(mut gb) = gb.lock() {
            gb.mmu.apu.set_speed(speed.factor);

            let mut break_hit: Option<(u8, u16)> = None;
            let mut watch_hit: Option<vibe_emu_core::watchpoints::WatchpointHit> = None;
            let mut yield_for_messages = false;
            let mut pause_wake_needed = false;
            let mut debugger_wake_needed = false;
            let mut shutdown_requested = false;
            let mut deferred: Vec<UiToEmu> = Vec::new();

            {
                let (cpu, mmu) = {
                    let GameBoy { cpu, mmu, .. } = &mut *gb;
                    (cpu, mmu)
                };

                if temp_exec_break.is_some() || (!ignore_breakpoints && !breakpoints.is_empty()) {
                    let key = exec_break_key_for_pc(cpu.pc, mmu);
                    let ignored_once = ignore_once_breakpoint == Some(key);
                    if ignored_once {
                        ignore_once_breakpoint = None;
                    }

                    let hit = temp_exec_break == Some(key)
                        || (!ignored_once && !ignore_breakpoints && breakpoints.contains(&key));
                    if hit {
                        temp_exec_break = None;
                        break_hit = Some((key.bank, key.addr));
                    }
                }

                if !watchpoints_suspended {
                    for wp in &watchpoints {
                        if !wp.enabled || !wp.on_execute || !wp.matches_addr(cpu.pc) {
                            continue;
                        }
                        if !wp.matches_value(None) {
                            continue;
                        }
                        watch_hit = Some(vibe_emu_core::watchpoints::WatchpointHit {
                            id: wp.id,
                            trigger: vibe_emu_core::watchpoints::WatchpointTrigger::Execute,
                            addr: cpu.pc,
                            value: None,
                            pc: Some(cpu.pc),
                        });
                        break;
                    }
                }

                let mut instrs_since_poll: u32 = 0;
                while !mmu.ppu.frame_ready() {
                    if yield_for_messages || break_hit.is_some() || watch_hit.is_some() {
                        break;
                    }

                    if temp_exec_break.is_some() || (!ignore_breakpoints && !breakpoints.is_empty())
                    {
                        let key = exec_break_key_for_pc(cpu.pc, mmu);
                        if temp_exec_break == Some(key) {
                            temp_exec_break = None;
                            break_hit = Some((key.bank, key.addr));
                            break;
                        }

                        if ignore_once_breakpoint == Some(key) {
                            ignore_once_breakpoint = None;
                        } else if !ignore_breakpoints && breakpoints.contains(&key) {
                            temp_exec_break = None;
                            break_hit = Some((key.bank, key.addr));
                            break;
                        }
                    }

                    if !watchpoints_suspended {
                        for wp in &watchpoints {
                            if !wp.enabled || !wp.on_execute || !wp.matches_addr(cpu.pc) {
                                continue;
                            }
                            if !wp.matches_value(None) {
                                continue;
                            }
                            watch_hit = Some(vibe_emu_core::watchpoints::WatchpointHit {
                                id: wp.id,
                                trigger: vibe_emu_core::watchpoints::WatchpointTrigger::Execute,
                                addr: cpu.pc,
                                value: None,
                                pc: Some(cpu.pc),
                            });
                            break;
                        }
                        if watch_hit.is_some() {
                            break;
                        }
                    }

                    let pre_pc = cpu.pc;
                    let pre_fallthrough = {
                        let was = mmu.watchpoints.suspended();
                        if !was {
                            mmu.watchpoints.set_suspended(true);
                        }
                        let opcode = mmu.read_byte(pre_pc);
                        if !was {
                            mmu.watchpoints.set_suspended(false);
                        }
                        let len = ui::code_data::sm83_instr_len(opcode) as u16;
                        pre_pc.wrapping_add(len)
                    };

                    let romx_bank = mmu.cart.as_ref().map(|c| c.current_rom_bank()).unwrap_or(1);
                    let bank = romx_bank.min(0xFF) as u8;
                    let was = mmu.watchpoints.suspended();
                    if !was {
                        mmu.watchpoints.set_suspended(true);
                    }
                    note_execute_pc(
                        pre_pc,
                        bank,
                        mmu,
                        &mut exec_seen_rom0,
                        &mut exec_seen_romx,
                        &mut pending_exec_trace,
                    );
                    if !was {
                        mmu.watchpoints.set_suspended(false);
                    }

                    cpu.step(mmu);

                    if let Some(hit) = mmu.watchpoints.take_hit() {
                        watch_hit = Some(hit);
                        break;
                    }

                    if !watchpoints_suspended && cpu.pc != pre_fallthrough {
                        let dest = cpu.pc;
                        for wp in &watchpoints {
                            if !wp.enabled || !wp.on_jump || !wp.matches_addr(dest) {
                                continue;
                            }
                            if !wp.matches_value(None) {
                                continue;
                            }
                            watch_hit = Some(vibe_emu_core::watchpoints::WatchpointHit {
                                id: wp.id,
                                trigger: vibe_emu_core::watchpoints::WatchpointTrigger::Jump,
                                addr: dest,
                                value: None,
                                pc: Some(pre_pc),
                            });
                            break;
                        }
                        if watch_hit.is_some() {
                            break;
                        }
                    }

                    instrs_since_poll = instrs_since_poll.wrapping_add(1);
                    if instrs_since_poll >= 256 {
                        instrs_since_poll = 0;

                        while let Ok(msg) = rx.try_recv() {
                            match msg {
                                UiToEmu::Command(cmd) => match cmd {
                                    EmuCommand::SetPaused(p) => {
                                        paused = p;
                                        next_frame = Instant::now() + FRAME_TIME;
                                        ignore_breakpoints = false;
                                        ignore_once_breakpoint = None;
                                        watchpoints_suspended = false;
                                        mmu.watchpoints.set_suspended(watchpoints_suspended);
                                        animate = false;
                                        if paused {
                                            temp_exec_break = None;
                                            pause_wake_needed = true;
                                            yield_for_messages = true;
                                            break;
                                        }
                                    }
                                    EmuCommand::Resume {
                                        ignore_breakpoints: ignore,
                                    } => {
                                        paused = false;
                                        ignore_breakpoints = ignore;
                                        watchpoints_suspended = ignore;
                                        mmu.watchpoints.set_suspended(watchpoints_suspended);
                                        next_frame = Instant::now() + FRAME_TIME;
                                        ignore_once_breakpoint = None;
                                    }
                                    EmuCommand::ResumeIgnoreOnce { breakpoint } => {
                                        paused = false;
                                        ignore_breakpoints = false;
                                        ignore_once_breakpoint = Some(breakpoint);
                                        watchpoints_suspended = false;
                                        mmu.watchpoints.set_suspended(watchpoints_suspended);
                                        next_frame = Instant::now() + FRAME_TIME;
                                    }
                                    EmuCommand::SetAnimate(value) => {
                                        animate = value;
                                    }
                                    EmuCommand::Step {
                                        count,
                                        cmd_id,
                                        guarantee_snapshot,
                                    } => {
                                        step_budget = step_budget.saturating_add(count);
                                        paused = true;
                                        ignore_breakpoints = false;
                                        watchpoints_suspended = false;
                                        mmu.watchpoints.set_suspended(watchpoints_suspended);
                                        next_frame = Instant::now() + FRAME_TIME;
                                        ignore_once_breakpoint = None;
                                        animate = false;
                                        temp_exec_break = None;
                                        if guarantee_snapshot {
                                            pending_debug_ack = cmd_id;
                                        }
                                        pause_wake_needed = true;
                                        yield_for_messages = true;
                                        break;
                                    }
                                    EmuCommand::RunTo {
                                        target,
                                        ignore_breakpoints: ignore,
                                    } => {
                                        temp_exec_break = Some(target);
                                        paused = false;
                                        ignore_breakpoints = ignore;
                                        watchpoints_suspended = ignore;
                                        mmu.watchpoints.set_suspended(watchpoints_suspended);
                                        ignore_once_breakpoint = None;
                                        next_frame = Instant::now() + FRAME_TIME;
                                    }
                                    cmd @ (EmuCommand::JumpTo { .. }
                                    | EmuCommand::CallCursor { .. }
                                    | EmuCommand::JumpSp) => {
                                        deferred.push(UiToEmu::Command(cmd));
                                        pause_wake_needed = true;
                                        yield_for_messages = true;
                                        break;
                                    }
                                    EmuCommand::SetBreakpoints(list) => {
                                        breakpoints.clear();
                                        breakpoints.extend(list);
                                    }
                                    EmuCommand::SetWatchpoints(list) => {
                                        watchpoints = list;
                                        mmu.watchpoints.set_watchpoints(watchpoints.clone());
                                        mmu.watchpoints.set_suspended(watchpoints_suspended);
                                    }
                                    EmuCommand::SetSpeed(new_speed) => {
                                        speed = new_speed;
                                        next_frame = Instant::now() + FRAME_TIME;
                                        mmu.apu.set_speed(speed.factor);
                                    }
                                    EmuCommand::UpdateInput(state) => {
                                        let if_reg = &mut mmu.if_reg;
                                        mmu.input.update_state(state, if_reg);
                                    }
                                    EmuCommand::UpdateLoadConfig(new_cfg) => {
                                        load_config = new_cfg;
                                    }
                                    EmuCommand::SetSerialPeripheral(peripheral) => {
                                        deferred.push(UiToEmu::Command(
                                            EmuCommand::SetSerialPeripheral(peripheral),
                                        ));
                                        yield_for_messages = true;
                                        break;
                                    }
                                    EmuCommand::Shutdown => {
                                        shutdown_requested = true;
                                        yield_for_messages = true;
                                        break;
                                    }
                                },
                                UiToEmu::Action(action) => {
                                    deferred.push(UiToEmu::Action(action));
                                    yield_for_messages = true;
                                    break;
                                }
                            }
                        }

                        if animate && !yield_for_messages {
                            debugger_wake_needed = true;
                            yield_for_messages = true;
                            break;
                        }
                    }
                }

                if !yield_for_messages
                    && break_hit.is_none()
                    && watch_hit.is_none()
                    && (temp_exec_break.is_some()
                        || (!ignore_breakpoints && !breakpoints.is_empty()))
                {
                    let key = exec_break_key_for_pc(cpu.pc, mmu);
                    let ignored_once = ignore_once_breakpoint == Some(key);
                    if ignored_once {
                        ignore_once_breakpoint = None;
                    }

                    let hit = temp_exec_break == Some(key)
                        || (!ignored_once && !ignore_breakpoints && breakpoints.contains(&key));
                    if hit {
                        temp_exec_break = None;
                        break_hit = Some((key.bank, key.addr));
                    }
                }

                if !yield_for_messages
                    && break_hit.is_none()
                    && watch_hit.is_none()
                    && !watchpoints_suspended
                {
                    for wp in &watchpoints {
                        if !wp.enabled || !wp.on_execute || !wp.matches_addr(cpu.pc) {
                            continue;
                        }
                        if !wp.matches_value(None) {
                            continue;
                        }
                        watch_hit = Some(vibe_emu_core::watchpoints::WatchpointHit {
                            id: wp.id,
                            trigger: vibe_emu_core::watchpoints::WatchpointTrigger::Execute,
                            addr: cpu.pc,
                            value: None,
                            pc: Some(cpu.pc),
                        });
                        break;
                    }
                }

                if break_hit.is_some() || watch_hit.is_some() {
                    // Breakpoint hit; do not attempt to complete or present a video frame.
                    // We'll publish a snapshot after we drop the CPU/MMU borrows.
                } else if yield_for_messages {
                    // Defer any actions until we drop the CPU/MMU borrows.
                } else {
                    // Avoid allocating every frame. If no free buffers are
                    // available, drop this frame rather than allocating.
                    if let Ok(mut buf) = frame_pool_rx.try_recv() {
                        // Core framebuffer is 0x00RRGGBB; convert to a u32 layout whose
                        // *in-memory bytes* match Pixels RGBA8 on little-endian: [R,G,B,A].
                        for (dst, &src) in buf.iter_mut().zip(mmu.ppu.framebuffer().iter()) {
                            // 0x00RRGGBB -> 0xFFBBGGRR (bytes RR GG BB FF)
                            *dst = 0xFF00_0000
                                | ((src & 0x0000_00FF) << 16)
                                | (src & 0x0000_FF00)
                                | ((src & 0x00FF_0000) >> 16);
                        }
                        frame_buf = Some(buf);
                    }
                    mmu.ppu.clear_frame_flag();
                }
            }

            if shutdown_requested {
                gb.mmu.save_cart_ram();
                if let Some(mobile) = mobile.as_ref()
                    && let Ok(mut adapter) = mobile.lock()
                {
                    let _ = adapter.stop();
                }
                return;
            }

            if yield_for_messages {
                for msg in deferred {
                    match msg {
                        UiToEmu::Command(EmuCommand::SetSerialPeripheral(peripheral)) => {
                            apply_serial_peripheral(
                                &mut gb,
                                &mobile,
                                peripheral,
                                mobile_diag,
                                &mut serial_peripheral,
                                &mut mobile_active,
                                &mut mobile_time_accum_ns,
                            );
                        }
                        UiToEmu::Action(action) => {
                            apply_ui_action(
                                action,
                                &mut gb,
                                &mut audio_stream,
                                speed,
                                &load_config,
                            );
                            apply_serial_peripheral(
                                &mut gb,
                                &mobile,
                                serial_peripheral,
                                mobile_diag,
                                &mut serial_peripheral,
                                &mut mobile_active,
                                &mut mobile_time_accum_ns,
                            );
                            gb.mmu.watchpoints.set_watchpoints(watchpoints.clone());
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                            gb.mmu.ppu.clear_frame_flag();
                            frame_count = 0;
                            next_frame = Instant::now() + FRAME_TIME;
                        }
                        UiToEmu::Command(EmuCommand::JumpTo { addr }) => {
                            paused = true;
                            ignore_breakpoints = false;
                            ignore_once_breakpoint = None;
                            watchpoints_suspended = false;
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                            animate = false;
                            next_frame = Instant::now() + FRAME_TIME;
                            temp_exec_break = None;
                            gb.cpu.pc = addr;
                            gb.cpu.halted = false;
                        }
                        UiToEmu::Command(EmuCommand::CallCursor { addr }) => {
                            paused = true;
                            ignore_breakpoints = false;
                            ignore_once_breakpoint = None;
                            watchpoints_suspended = false;
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                            animate = false;
                            next_frame = Instant::now() + FRAME_TIME;
                            temp_exec_break = None;

                            let ret = gb.cpu.pc;
                            let sp_hi = gb.cpu.sp.wrapping_sub(1);
                            let sp_lo = gb.cpu.sp.wrapping_sub(2);
                            gb.mmu.write_byte(sp_hi, (ret >> 8) as u8);
                            gb.mmu.write_byte(sp_lo, (ret & 0xFF) as u8);
                            gb.cpu.sp = sp_lo;
                            gb.cpu.pc = addr;
                            gb.cpu.halted = false;
                        }
                        UiToEmu::Command(EmuCommand::JumpSp) => {
                            paused = true;
                            ignore_breakpoints = false;
                            ignore_once_breakpoint = None;
                            watchpoints_suspended = false;
                            gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                            animate = false;
                            next_frame = Instant::now() + FRAME_TIME;
                            temp_exec_break = None;

                            let sp = gb.cpu.sp;
                            let lo = gb.mmu.read_byte(sp);
                            let hi = gb.mmu.read_byte(sp.wrapping_add(1));
                            let target = u16::from_le_bytes([lo, hi]);
                            gb.cpu.sp = sp.wrapping_add(2);
                            gb.cpu.pc = target;
                            gb.cpu.halted = false;
                        }
                        _ => {}
                    }
                }

                if pause_wake_needed {
                    if let Ok(mut snap) = ui_snapshot.write() {
                        *snap = UiSnapshot::from_gb(&mut gb, true);
                    }
                    let _ = wake_proxy.send_event(UserEvent::DebuggerWake);
                } else if debugger_wake_needed {
                    if let Ok(mut snap) = ui_snapshot.try_write() {
                        *snap = UiSnapshot::from_gb(&mut gb, false);
                    }
                    let _ = wake_proxy.send_event(UserEvent::DebuggerWake);
                }

                continue;
            }

            if let Some((bank, pc)) = break_hit {
                paused = true;
                ignore_breakpoints = false;
                ignore_once_breakpoint = None;
                watchpoints_suspended = false;
                gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                animate = false;
                temp_exec_break = None;
                if let Ok(mut snap) = ui_snapshot.write() {
                    *snap = UiSnapshot::from_gb(&mut gb, true);
                }
                let _ = wake_proxy.send_event(UserEvent::DebuggerBreak { bank, addr: pc });
                continue;
            }

            if let Some(hit) = watch_hit {
                paused = true;
                ignore_breakpoints = false;
                ignore_once_breakpoint = None;
                watchpoints_suspended = false;
                gb.mmu.watchpoints.set_suspended(watchpoints_suspended);
                animate = false;
                temp_exec_break = None;
                if let Ok(mut snap) = ui_snapshot.write() {
                    *snap = UiSnapshot::from_gb(&mut gb, true);
                }
                let _ = wake_proxy.send_event(UserEvent::DebuggerWatchpoint { hit });
                continue;
            }

            // Publish a UI snapshot while we already hold the emulation lock.
            // Use try_write to avoid stalling emulation if the UI is mid-draw.
            if let Ok(mut snap) = ui_snapshot.try_write() {
                *snap = UiSnapshot::from_gb(&mut gb, paused);
            }

            flush_exec_trace(&mut pending_exec_trace);

            if !speed.fast {
                let elapsed = frame_start.elapsed();
                let warn_threshold = FRAME_TIME + FRAME_TIME / 2;
                if elapsed > warn_threshold {
                    warn!(
                        "Frame emulation exceeded budget: {:?} vs {:?} (audio queue {} / {})",
                        elapsed,
                        FRAME_TIME,
                        gb.mmu.apu.queued_frames(),
                        gb.mmu.apu.max_queue_capacity()
                    );
                }
            }

            if debug && frame_count.is_multiple_of(60) {
                let out = gb.mmu.take_serial();
                if !out.is_empty() {
                    serial = Some(out);
                }
                debug!(target: "vibe_emu_ui::cpu", "{}", gb.cpu.debug_state());
            }
        }

        if let Some(frame) = frame_buf {
            // If the UI is behind, drop frames instead of queueing unbounded.
            let evt = EmuEvent::Frame {
                frame,
                frame_index: frame_count,
            };

            match frame_tx.send_timeout(evt, Duration::ZERO) {
                Ok(()) => {
                    let _ = wake_proxy.send_event(UserEvent::EmuWake);
                }
                Err(
                    cb::SendTimeoutError::Timeout(evt) | cb::SendTimeoutError::Disconnected(evt),
                ) => {
                    // Return buffer to the pool if we couldn't send.
                    if let EmuEvent::Frame { frame, .. } = evt {
                        let _ = frame_pool_tx.send_timeout(frame, Duration::ZERO);
                    }
                }
            }
        }

        if let Some(serial) = serial {
            let _ = serial_tx.send_timeout(
                EmuEvent::Serial {
                    data: serial,
                    frame_index: frame_count,
                },
                Duration::ZERO,
            );
            let _ = wake_proxy.send_event(UserEvent::EmuWake);
        }

        frame_count = frame_count.wrapping_add(1);

        if mobile_active && let Some(mobile) = mobile.as_ref() {
            // Advance emulated time by one frame and drive libmobile.
            mobile_time_accum_ns += FRAME_TIME.as_nanos();
            let delta_ms = (mobile_time_accum_ns / 1_000_000) as u32;
            mobile_time_accum_ns %= 1_000_000;

            if delta_ms != 0
                && let Ok(mut adapter) = mobile.lock()
            {
                let _ = adapter.poll(delta_ms);
            }
        }

        if !speed.fast {
            let now = Instant::now();
            if now < next_frame {
                thread::sleep(next_frame - now);
            }
            next_frame += FRAME_TIME;
        } else {
            next_frame = Instant::now();
        }
    }
}

fn draw_vram(
    viewer: Option<&mut ui::vram_viewer::VramViewerWindow>,
    renderer: &mut imgui_wgpu::Renderer,
    pixels: &mut Pixels,
    snapshot: &UiSnapshot,
    ui: &imgui::Ui,
) {
    let _ = pixels.frame_mut();
    if let Some(viewer) = viewer {
        viewer.ui(ui, &snapshot.ppu, renderer, pixels.device(), pixels.queue());
    } else {
        ui.text("VRAM viewer not initialized");
    }
}

fn draw_game_screen(pixels: &mut Pixels, frame: &[u32]) {
    // Frames are pre-converted to a u32 layout that matches Pixels' RGBA8
    // byte buffer on little-endian platforms: 0xAABBGGRR in u32.
    let dst = pixels.frame_mut();
    if dst.len() == frame.len() * 4 {
        // SAFETY: `frame` is a `[u32]` stored contiguously; we only view its bytes.
        // The emulator thread writes pixels as 0xAABBGGRR so little-endian memory
        // order matches RGBA8 ([R, G, B, A]) expected by Pixels.
        let src =
            unsafe { std::slice::from_raw_parts(frame.as_ptr().cast::<u8>(), frame.len() * 4) };
        dst.copy_from_slice(src);
    }
}

fn build_ui(state: &mut UiState, cfg: &mut UiConfig, ui: &imgui::Ui, mobile_available: bool) {
    let mut any_menu_open = false;

    // Top menu bar (replaces the old right-click context menu).
    if let Some(_bar) = ui.begin_main_menu_bar() {
        if let Some(_menu) = ui.begin_menu("File") {
            any_menu_open = true;
            if ui.menu_item("Load ROM...")
                && let Some(path) = FileDialog::new()
                    .add_filter("Game Boy ROM", &["gb", "gbc"])
                    .pick_file()
            {
                state.current_rom_path = Some(path.clone());
                state.debugger.load_symbols_for_rom_path(Some(&path));
                state.pending_action = Some(UiAction::LoadPath(path));
            }
            if ui.menu_item("Reset") {
                state.pending_action = Some(UiAction::Reset);
            }
            if ui.menu_item("Exit") {
                state.pending_exit = true;
            }
        }

        if let Some(_menu) = ui.begin_menu("Emulation") {
            any_menu_open = true;

            let pause_label = if state.paused { "Resume" } else { "Pause" };
            if ui.menu_item(pause_label) {
                let new_paused = !state.paused;
                state.paused = new_paused;
                state.pending_pause = Some(new_paused);
                // Manual pause overrides menu auto-resume.
                state.menu_resume_armed = false;
            }

            if let Some(_serial_menu) = ui.begin_menu("Serial Peripheral") {
                any_menu_open = true;
                if mobile_available {
                    let none_selected = state.serial_peripheral == SerialPeripheral::None;
                    if ui.menu_item_config("None").selected(none_selected).build() && !none_selected
                    {
                        state.serial_peripheral = SerialPeripheral::None;
                        state.pending_serial_peripheral = Some(SerialPeripheral::None);
                    }

                    let mob_selected = state.serial_peripheral == SerialPeripheral::MobileAdapter;
                    if ui
                        .menu_item_config("Mobile Adapter GB")
                        .selected(mob_selected)
                        .build()
                        && !mob_selected
                    {
                        state.serial_peripheral = SerialPeripheral::MobileAdapter;
                        state.pending_serial_peripheral = Some(SerialPeripheral::MobileAdapter);
                    }
                } else {
                    state.serial_peripheral = SerialPeripheral::None;
                    ui.text_disabled("Mobile Adapter GB (unavailable)");
                }
            }

            if let Some(_mode_menu) = ui.begin_menu("Hardware Mode") {
                any_menu_open = true;

                let auto_selected = cfg.emulation_mode == EmulationMode::Auto;
                if ui.menu_item_config("Auto").selected(auto_selected).build() && !auto_selected {
                    cfg.emulation_mode = EmulationMode::Auto;
                    state.pending_load_config_update = true;
                    state.pending_save_ui_config = true;
                    if let Some(path) = state.current_rom_path.clone() {
                        state.pending_action = Some(UiAction::LoadPath(path));
                    }
                }

                let dmg_selected = cfg.emulation_mode == EmulationMode::ForceDmg;
                if ui
                    .menu_item_config("Force DMG")
                    .selected(dmg_selected)
                    .build()
                    && !dmg_selected
                {
                    cfg.emulation_mode = EmulationMode::ForceDmg;
                    state.pending_load_config_update = true;
                    state.pending_save_ui_config = true;
                    if let Some(path) = state.current_rom_path.clone() {
                        state.pending_action = Some(UiAction::LoadPath(path));
                    }
                }

                let cgb_selected = cfg.emulation_mode == EmulationMode::ForceCgb;
                if ui
                    .menu_item_config("Force CGB")
                    .selected(cgb_selected)
                    .build()
                    && !cgb_selected
                {
                    cfg.emulation_mode = EmulationMode::ForceCgb;
                    state.pending_load_config_update = true;
                    state.pending_save_ui_config = true;
                    if let Some(path) = state.current_rom_path.clone() {
                        state.pending_action = Some(UiAction::LoadPath(path));
                    }
                }
            }
        }

        if let Some(_menu) = ui.begin_menu("Tools") {
            any_menu_open = true;
            if ui.menu_item("Debugger") {
                state.spawn_debugger = true;
                state.paused = true;
                state.pending_pause = Some(true);
                state.menu_resume_armed = false;
            }
            if ui.menu_item("VRAM Viewer") {
                state.spawn_vram = true;
                state.paused = true;
                state.pending_pause = Some(true);
                state.menu_resume_armed = false;
            }
            if ui.menu_item("Watchpoints") {
                state.spawn_watchpoints = true;
                state.paused = true;
                state.pending_pause = Some(true);
                state.menu_resume_armed = false;
            }
        }

        if let Some(_menu) = ui.begin_menu("Options") {
            any_menu_open = true;
            if ui.menu_item("Options...") {
                state.spawn_options = true;
            }
        }
    }

    // Auto-pause while the top menu is open, and resume when it closes.
    if any_menu_open {
        if !state.menu_pause_active {
            state.menu_pause_active = true;
            state.menu_resume_armed = !state.paused;
            if !state.paused {
                state.paused = true;
                state.pending_pause = Some(true);
            }
        }
    } else if state.menu_pause_active {
        state.menu_pause_active = false;
        if state.menu_resume_armed {
            state.menu_resume_armed = false;
            state.paused = false;
            state.pending_pause = Some(false);
        }
    }
}

fn draw_options_window(
    state: &mut UiState,
    cfg: &mut UiConfig,
    keybinds: &KeyBindings,
    ui: &imgui::Ui,
) {
    let display = ui.io().display_size;
    let flags = imgui::WindowFlags::NO_MOVE
        | imgui::WindowFlags::NO_RESIZE
        | imgui::WindowFlags::NO_COLLAPSE;

    ui.window("Options")
        .position([0.0, 0.0], imgui::Condition::Always)
        .size(display, imgui::Condition::Always)
        .flags(flags)
        .build(|| {
            if !state.bootrom_edit_initialized {
                state.bootrom_edit_initialized = true;
                state.dmg_bootrom_edit = cfg
                    .dmg_bootrom_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                state.cgb_bootrom_edit = cfg
                    .cgb_bootrom_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
            }

            if let Some(_tabs) = imgui::TabBar::new("OptionsTabs").begin(ui) {
                if let Some(_tab) = imgui::TabItem::new("Keybinds").begin(ui) {
                    ui.text("Click Rebind, then press a key.");

                    if state.rebinding.is_some() {
                        ui.text_colored([1.0, 0.8, 0.2, 1.0], "Waiting for key...");
                        ui.same_line();
                        if ui.button("Cancel") {
                            state.rebinding = None;
                        }
                        ui.separator();
                    }

                    let mut row = |label: &str, current: String, target: RebindTarget| {
                        ui.text(label);
                        ui.same_line();
                        ui.text_disabled(current);
                        ui.same_line();
                        let btn = format!("Rebind##{label}");
                        if ui.button(btn) {
                            state.rebinding = Some(target);
                        }
                    };

                    let fmt_joy = |mask: u8| {
                        keybinds
                            .key_for_joypad_mask(mask)
                            .map(|c| format!("{c:?}"))
                            .unwrap_or_else(|| "<unbound>".to_string())
                    };

                    row("Up", fmt_joy(0x04), RebindTarget::Joypad(0x04));
                    row("Down", fmt_joy(0x08), RebindTarget::Joypad(0x08));
                    row("Left", fmt_joy(0x02), RebindTarget::Joypad(0x02));
                    row("Right", fmt_joy(0x01), RebindTarget::Joypad(0x01));

                    ui.separator();
                    row("A", fmt_joy(0x10), RebindTarget::Joypad(0x10));
                    row("B", fmt_joy(0x20), RebindTarget::Joypad(0x20));
                    row("Select", fmt_joy(0x40), RebindTarget::Joypad(0x40));
                    row("Start", fmt_joy(0x80), RebindTarget::Joypad(0x80));

                    ui.separator();
                    row(
                        "Pause",
                        format!("{:?}", keybinds.pause_key()),
                        RebindTarget::Pause,
                    );
                    row(
                        "Fast Forward",
                        format!("{:?}", keybinds.fast_forward_key()),
                        RebindTarget::FastForward,
                    );
                    row(
                        "Quit",
                        format!("{:?}", keybinds.quit_key()),
                        RebindTarget::Quit,
                    );
                }

                if let Some(_tab) = imgui::TabItem::new("Emulation").begin(ui) {
                    ui.text("Boot ROMs");
                    ui.separator();

                    ui.text("DMG boot ROM");
                    let dmg_changed =
                        imgui::InputText::new(ui, "##dmg_bootrom", &mut state.dmg_bootrom_edit)
                            .build();
                    ui.same_line();
                    let mut dmg_browsed = false;
                    if ui.button("Browse##dmg_bootrom")
                        && let Some(path) = FileDialog::new().pick_file()
                    {
                        state.dmg_bootrom_edit = path.to_string_lossy().to_string();
                        dmg_browsed = true;
                    }
                    if dmg_changed || dmg_browsed {
                        let trimmed = state.dmg_bootrom_edit.trim();
                        cfg.dmg_bootrom_path = if trimmed.is_empty() {
                            None
                        } else {
                            Some(std::path::PathBuf::from(trimmed))
                        };
                        state.pending_load_config_update = true;
                        state.pending_save_ui_config = true;
                    }

                    ui.text("CGB boot ROM");
                    let cgb_changed =
                        imgui::InputText::new(ui, "##cgb_bootrom", &mut state.cgb_bootrom_edit)
                            .build();
                    ui.same_line();
                    let mut cgb_browsed = false;
                    if ui.button("Browse##cgb_bootrom")
                        && let Some(path) = FileDialog::new().pick_file()
                    {
                        state.cgb_bootrom_edit = path.to_string_lossy().to_string();
                        cgb_browsed = true;
                    }
                    if cgb_changed || cgb_browsed {
                        let trimmed = state.cgb_bootrom_edit.trim();
                        cfg.cgb_bootrom_path = if trimmed.is_empty() {
                            None
                        } else {
                            Some(std::path::PathBuf::from(trimmed))
                        };
                        state.pending_load_config_update = true;
                        state.pending_save_ui_config = true;
                    }

                    ui.separator();
                    let items = [
                        "1x",
                        "2x",
                        "3x",
                        "4x",
                        "5x",
                        "6x",
                        "Fullscreen",
                        "Fullscreen (stretched)",
                    ];
                    let mut current_idx = match cfg.window_size {
                        WindowSize::X1 => 0,
                        WindowSize::X2 => 1,
                        WindowSize::X3 => 2,
                        WindowSize::X4 => 3,
                        WindowSize::X5 => 4,
                        WindowSize::X6 => 5,
                        WindowSize::Fullscreen => 6,
                        WindowSize::FullscreenStretched => 7,
                    };

                    if ui.combo_simple_string("Window size", &mut current_idx, &items) {
                        cfg.window_size = match current_idx {
                            0 => WindowSize::X1,
                            1 => WindowSize::X2,
                            2 => WindowSize::X3,
                            3 => WindowSize::X4,
                            4 => WindowSize::X5,
                            5 => WindowSize::X6,
                            6 => WindowSize::Fullscreen,
                            _ => WindowSize::FullscreenStretched,
                        };
                        state.pending_window_size = Some(cfg.window_size);
                        state.pending_save_ui_config = true;
                    }
                }
            }
        });
}

fn configure_wgpu_backend(args: &Args) {
    if let Some(choice) = args.wgpu_backend {
        if let Some(value) = choice.as_env_value() {
            // Ensure the backend is configured before wgpu initializes.
            unsafe {
                std::env::set_var("WGPU_BACKEND", value);
            }
        }
    } else if std::env::var_os("WGPU_BACKEND").is_none() {
        #[cfg(target_os = "windows")]
        {
            // Prefer DirectX on Windows to avoid Vulkan/ANGLE present-mode quirks on some setups.
            unsafe {
                std::env::set_var("WGPU_BACKEND", "dx12");
            }
        }
    }
}

fn log_wgpu_adapter_once(pixels: &Pixels) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let adapter_info = pixels.adapter().get_info();
        info!(
            "WGPU adapter: {} (type={:?}, backend={:?}, vendor=0x{:X}, device=0x{:X}); driver='{}' driver_info='{}'",
            adapter_info.name,
            adapter_info.device_type,
            adapter_info.backend,
            adapter_info.vendor,
            adapter_info.device,
            adapter_info.driver,
            adapter_info.driver_info
        );

        if let Some(forced) = std::env::var_os("WGPU_BACKEND") {
            info!("WGPU_BACKEND={}", forced.to_string_lossy());
        }
    });
}

fn prime_audio_queue(gb: &mut GameBoy) -> (usize, usize) {
    let capacity = gb.mmu.apu.max_queue_capacity().max(1);
    let target_frames = ((capacity as f32) * AUDIO_WARMUP_TARGET_RATIO).ceil() as usize;

    let mut queued = gb.mmu.apu.queued_frames();
    if queued >= target_frames {
        return (queued, target_frames);
    }

    let deadline = Instant::now() + Duration::from_millis(AUDIO_WARMUP_TIMEOUT_MS);
    let mut steps = 0u32;
    loop {
        gb.cpu.step(&mut gb.mmu);
        steps += 1;

        if steps >= AUDIO_WARMUP_CHECK_INTERVAL {
            steps = 0;
            let now = Instant::now();
            queued = gb.mmu.apu.queued_frames();
            if queued >= target_frames || now >= deadline {
                break;
            }
        }
    }

    if queued < target_frames {
        queued = gb.mmu.apu.queued_frames();
    }

    (queued, target_frames)
}

fn prime_and_play_audio(gb: &mut GameBoy, audio_stream: &mut Option<cpal::Stream>) {
    let primed = audio_stream.as_ref().map(|_| prime_audio_queue(gb));

    if let Some((queued, target)) = &primed {
        if *queued >= *target {
            info!("Primed audio queue with {queued} / {target} frames before starting playback");
        } else {
            warn!(
                "Audio warmup timed out after priming {queued} / {target} frames; startup may glitch"
            );
        }
    }

    if let Some(stream) = audio_stream.as_ref() {
        if let Err(e) = stream.play() {
            warn!("Failed to start audio stream: {e}");
            *audio_stream = None;
        } else if let Some((queued, target)) = &primed {
            info!("Audio playback started with {queued} primed frames (capacity {target})");
        } else {
            info!("Audio playback started");
        }
    }
}

fn rebuild_audio_stream(gb: &mut GameBoy, speed: Speed, audio_stream: &mut Option<cpal::Stream>) {
    if let Some(stream) = audio_stream.take() {
        drop(stream);
    }

    gb.mmu.apu.set_speed(speed.factor);

    *audio_stream = audio::start_stream(&mut gb.mmu.apu, false);
    if audio_stream.is_none() {
        warn!("Audio output disabled; continuing without sound");
        return;
    }

    prime_and_play_audio(gb, audio_stream);
}

fn apply_ui_action(
    action: UiAction,
    gb: &mut GameBoy,
    audio_stream: &mut Option<cpal::Stream>,
    speed: Speed,
    load_config: &LoadConfig,
) {
    match action {
        UiAction::Reset => {
            info!("Resetting Game Boy");
            gb.reset();
        }
        UiAction::LoadPath(path) => {
            info!("Loading new ROM");
            match Cartridge::from_file(&path) {
                Ok(cart) => {
                    *gb = build_gameboy_for_cart(Some(cart), load_config);
                }
                Err(e) => {
                    warn!("Failed to load ROM {}: {e}", path.display());
                }
            }
        }
    }

    if gb.mmu.cart.is_some() {
        rebuild_audio_stream(gb, speed, audio_stream);
    }
}

fn build_gameboy_for_cart(cart: Option<Cartridge>, load_config: &LoadConfig) -> GameBoy {
    let cart_is_cgb = cart.as_ref().is_some_and(|c| c.cgb);
    let cgb_mode = match load_config.emulation_mode {
        EmulationMode::ForceDmg => false,
        EmulationMode::ForceCgb => true,
        EmulationMode::Auto => cart_is_cgb,
    };

    let bootrom_data = if let Some(data) = load_config.bootrom_override.clone() {
        Some(data)
    } else {
        let path = if cgb_mode {
            load_config.cgb_bootrom_path.as_ref()
        } else {
            load_config.dmg_bootrom_path.as_ref()
        };
        match path {
            Some(p) if !p.as_os_str().is_empty() => match std::fs::read(p) {
                Ok(data) => Some(data),
                Err(e) => {
                    warn!("Failed to load boot ROM {}: {e}", p.display());
                    None
                }
            },
            _ => None,
        }
    };

    let mut gb = if bootrom_data.is_some() {
        GameBoy::new_power_on_with_revision(cgb_mode, CgbRevision::default())
    } else {
        GameBoy::new_with_revision(cgb_mode, CgbRevision::default())
    };

    if let Some(cart) = cart {
        gb.mmu.load_cart(cart);
    }

    if !cgb_mode && load_config.dmg_neutral {
        const NEUTRAL_DMG_PALETTE: [u32; 4] = [0x00E0F8D0, 0x0088C070, 0x00346856, 0x00081820];
        gb.mmu.ppu.set_dmg_palette(NEUTRAL_DMG_PALETTE);
    }

    if let Some(data) = bootrom_data {
        gb.mmu.load_boot_rom(data);
        gb.cpu.pc = 0x0000;
    }

    gb
}

fn build_load_config(
    cfg: &UiConfig,
    dmg_neutral: bool,
    bootrom_override: Option<Vec<u8>>,
) -> LoadConfig {
    LoadConfig {
        emulation_mode: cfg.emulation_mode,
        dmg_neutral,
        bootrom_override,
        dmg_bootrom_path: cfg.dmg_bootrom_path.clone(),
        cgb_bootrom_path: cfg.cgb_bootrom_path.clone(),
    }
}

fn apply_window_size_setting(window: &winit::window::Window, mode: WindowSize) {
    use winit::dpi::PhysicalSize;
    use winit::window::Fullscreen;

    if mode.is_fullscreen() {
        window.set_fullscreen(Some(Fullscreen::Borderless(None)));
        return;
    }

    window.set_fullscreen(None);
    if let Some(scale) = mode.scale_factor_px() {
        let size = PhysicalSize::new(160 * scale, 144 * scale + DEFAULT_MENU_BAR_HEIGHT_PX);
        let _ = window.request_inner_size(size);
    }
}

fn main() {
    let args = Args::parse();

    configure_wgpu_backend(&args);

    init_logging(&args);

    info!("Starting emulator");

    let headless = args.headless;
    let rom_path = args.rom.clone();

    let ui_config_path = ui_config::default_ui_config_path();
    let mut ui_config = if ui_config_path.exists() {
        ui_config::load_from_file(&ui_config_path)
    } else {
        UiConfig::default()
    };

    let bootrom_data = match args.bootrom.as_ref() {
        Some(path) => match std::fs::read(path) {
            Ok(data) => Some(data),
            Err(e) => {
                error!("Failed to load boot ROM {}: {e}", path.display());
                return;
            }
        },
        None => None,
    };

    let mut load_config = build_load_config(&ui_config, args.dmg_neutral, bootrom_data);
    if args.dmg {
        load_config.emulation_mode = EmulationMode::ForceDmg;
    } else if args.cgb {
        load_config.emulation_mode = EmulationMode::ForceCgb;
    }

    let cart = match rom_path.as_ref() {
        Some(path) => match Cartridge::from_file(path) {
            Ok(c) => Some(c),
            Err(e) => {
                error!("Failed to load ROM: {e}");
                return;
            }
        },
        None => None,
    };

    if headless && cart.is_none() {
        error!("No ROM supplied (required for --headless)");
        return;
    }

    let cart_is_cgb = cart.as_ref().is_some_and(|c| c.cgb);
    let cgb_mode = match load_config.emulation_mode {
        EmulationMode::ForceDmg => false,
        EmulationMode::ForceCgb => true,
        EmulationMode::Auto => cart_is_cgb,
    };

    let mut gb = build_gameboy_for_cart(cart, &load_config);

    info!(
        "Emulator initialized in {} mode",
        if cgb_mode { "CGB" } else { "DMG" }
    );
    let debug_enabled = args.debug;
    let frame_limit = args.frames;
    let cycle_limit = args.cycles;
    let second_limit = args.seconds.map(Duration::from_secs);

    // UI frames are stored as u32 in a byte layout matching Pixels' RGBA8 buffer.
    let mut frame = vec![0u32; 160 * 144];

    let keybinds_path = {
        let from_args = args.keybinds.clone();
        let from_env = std::env::var_os("VIBEEMU_KEYBINDS").map(std::path::PathBuf::from);
        from_args.or(from_env).unwrap_or_else(default_keybinds_path)
    };

    let mut keybinds = if keybinds_path.exists() {
        KeyBindings::load_from_file(&keybinds_path)
    } else {
        KeyBindings::defaults()
    };

    if !keybinds_path.exists()
        && let Err(e) = keybinds.save_to_file(&keybinds_path)
    {
        warn!(
            "Failed to write default keybinds file {}: {e}",
            keybinds_path.display()
        );
    }

    let mut serial_peripheral = if args.mobile {
        SerialPeripheral::MobileAdapter
    } else {
        SerialPeripheral::None
    };

    let config_path = args
        .mobile_config
        .clone()
        .or_else(|| rom_path.as_ref().map(|p| p.with_extension("mobile")))
        .unwrap_or_else(|| std::path::PathBuf::from("vibeemu.mobile"));

    let mobile = {
        let base_host: Box<dyn MobileHost> = Box::new(StdMobileHost::new(config_path));
        let host: Box<dyn MobileHost> = if args.mobile_diag {
            Box::new(DiagMobileHost::new(base_host))
        } else {
            base_host
        };

        match MobileAdapter::new(host) {
            Ok(mut adapter) => {
                let mut cfg = MobileConfig {
                    device: args.mobile_device.into(),
                    unmetered: args.mobile_unmetered,
                    p2p_port: args.mobile_p2p_port,
                    ..Default::default()
                };

                if let Some(s) = args.mobile_dns1.as_deref() {
                    match parse_mobile_addr(s) {
                        Ok(addr) => cfg.dns1 = addr,
                        Err(e) => warn!("Ignoring --mobile-dns1: {e}"),
                    }
                }
                if let Some(s) = args.mobile_dns2.as_deref() {
                    match parse_mobile_addr(s) {
                        Ok(addr) => cfg.dns2 = addr,
                        Err(e) => warn!("Ignoring --mobile-dns2: {e}"),
                    }
                }
                if let Some(s) = args.mobile_relay.as_deref() {
                    match parse_mobile_addr(s) {
                        Ok(addr) => cfg.relay = addr,
                        Err(e) => warn!("Ignoring --mobile-relay: {e}"),
                    }
                }

                let _ = adapter.apply_config(&cfg);
                Some(Arc::new(Mutex::new(adapter)))
            }
            Err(e) => {
                warn!("Mobile Adapter backend unavailable: {e}");
                None
            }
        }
    };

    match serial_peripheral {
        SerialPeripheral::None => {
            gb.mmu.serial.connect(Box::new(NullLinkPort::default()));
        }
        SerialPeripheral::MobileAdapter => {
            if let Some(mobile) = mobile.as_ref() {
                if let Ok(mut adapter) = mobile.lock()
                    && let Err(e) = adapter.start()
                {
                    warn!("Failed to start Mobile Adapter: {e}");
                    gb.mmu.serial.connect(Box::new(NullLinkPort::default()));
                    serial_peripheral = SerialPeripheral::None;
                }

                if serial_peripheral == SerialPeripheral::MobileAdapter {
                    if args.mobile_diag {
                        gb.mmu
                            .serial
                            .connect(Box::new(DiagMobileLinkPort::new(Arc::clone(mobile))));
                    } else {
                        gb.mmu
                            .serial
                            .connect(Box::new(MobileLinkPort::new(Arc::clone(mobile))));
                    }
                }
            } else {
                warn!("Mobile Adapter selected but backend not available");
                gb.mmu.serial.connect(Box::new(NullLinkPort::default()));
                serial_peripheral = SerialPeripheral::None;
            }
        }
    }

    if !headless {
        let initial_paused = rom_path.is_none();

        let gb = Arc::new(Mutex::new(gb));
        let ui_snapshot = Arc::new(RwLock::new(UiSnapshot::default()));
        let mut speed = Speed {
            factor: 1.0,
            fast: false,
        };
        let mut ui_state = UiState {
            serial_peripheral,
            ..Default::default()
        };
        ui_state.current_rom_path = rom_path.clone();
        ui_state.paused = initial_paused;
        ui_state
            .debugger
            .load_symbols_for_rom_path(ui_state.current_rom_path.as_deref());

        let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
            Ok(el) => el,
            Err(e) => {
                error!("Failed to initialize winit event loop: {e}");
                return;
            }
        };
        let wake_proxy = event_loop.create_proxy();

        let (to_emu_tx, to_emu_rx) = mpsc::channel();
        // Bounded channels so UI stalls can't grow memory without bound.
        let (from_emu_frame_tx, from_emu_frame_rx) = cb::bounded(1);
        let (from_emu_serial_tx, from_emu_serial_rx) = cb::bounded(64);
        // Buffer pool to avoid per-frame allocations in the emulator thread.
        let (frame_pool_tx, frame_pool_rx) = cb::bounded::<Vec<u32>>(2);
        for _ in 0..2 {
            let _ = frame_pool_tx.send(vec![0u32; 160 * 144]);
        }

        let exec_trace = Arc::new(Mutex::new(Vec::<ui::code_data::ExecutedInstruction>::new()));
        let emu_gb = Arc::clone(&gb);
        let emu_snapshot = Arc::clone(&ui_snapshot);
        let emu_mobile = mobile.clone();
        let emu_serial_peripheral = serial_peripheral;
        let emu_mobile_diag = args.mobile_diag;
        let emu_load_config = load_config.clone();
        let emu_frame_pool_tx = frame_pool_tx.clone();
        let emu_exec_trace = Arc::clone(&exec_trace);
        let emu_handle = thread::spawn(move || {
            run_emulator_thread(
                emu_gb,
                emu_snapshot,
                speed,
                initial_paused,
                debug_enabled,
                emu_mobile,
                emu_serial_peripheral,
                emu_mobile_diag,
                emu_load_config,
                EmuThreadChannels {
                    rx: to_emu_rx,
                    frame_tx: from_emu_frame_tx,
                    serial_tx: from_emu_serial_tx,
                    frame_pool_tx: emu_frame_pool_tx,
                    frame_pool_rx,
                    wake_proxy,
                    exec_trace: emu_exec_trace,
                },
            );
        });
        let mut emu_handle = Some(emu_handle);
        let mut sent_shutdown = false;

        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::UpdateInput(0xFF)));

        let initial_scale = ui_config
            .window_size
            .scale_factor_px()
            .unwrap_or(DEFAULT_WINDOW_SCALE);
        let attrs = enforce_square_corners(
            Window::default_attributes()
                .with_title("vibeEmu")
                .with_window_icon(load_window_icon())
                .with_resizable(false)
                .with_inner_size(winit::dpi::LogicalSize::new(
                    (160 * initial_scale) as f64,
                    (144 * initial_scale + DEFAULT_MENU_BAR_HEIGHT_PX) as f64,
                )),
        );
        #[allow(deprecated)]
        let window = match event_loop.create_window(attrs) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create main window: {e}");
                return;
            }
        };

        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, &window);
        let pixels = match Pixels::new(160, 144, surface) {
            Ok(p) => p,
            Err(e) => {
                error!("Pixels init failed (main window): {e}");
                return;
            }
        };

        log_wgpu_adapter_once(&pixels);

        let mut windows = HashMap::new();
        let main_win = UiWindow::new(WindowKind::Main, window, pixels, (160, 144));
        let main_id = main_win.win.id();
        windows.insert(main_id, main_win);
        if let Some(win) = windows.get_mut(&main_id) {
            win.resize(win.win.inner_size());
        }

        if let Some(main) = windows.get(&main_id) {
            apply_window_size_setting(&main.win, ui_config.window_size);
        }
        ui_state.last_main_inner_size = None;

        let mut state = 0xFFu8;

        if initial_paused {
            let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetPaused(true)));
        }

        #[allow(deprecated)]
        let _ = event_loop.run(move |event, target| {
            target.set_control_flow(if ui_state.paused {
                ControlFlow::Wait
            } else {
                ControlFlow::WaitUntil(Instant::now() + FRAME_TIME)
            });
            match &event {
                Event::UserEvent(UserEvent::EmuWake) => {
                    if let Ok(mut trace) = exec_trace.lock()
                        && !trace.is_empty()
                    {
                        let drained = std::mem::take(&mut *trace);
                        ui_state
                            .debugger
                            .note_executed_instructions(drained.as_slice());
                    }

                    let mut got_frame = false;

                    while let Ok(evt) = from_emu_frame_rx.try_recv() {
                        if let EmuEvent::Frame {
                            frame: mut incoming,
                            frame_index: _,
                        } = evt
                            && incoming.len() == frame.len()
                        {
                            std::mem::swap(&mut frame, &mut incoming);
                            let _ = frame_pool_tx.send_timeout(incoming, Duration::ZERO);
                            got_frame = true;
                        }
                    }

                    while let Ok(evt) = from_emu_serial_rx.try_recv() {
                        if let EmuEvent::Serial { data, frame_index } = evt
                            && !data.is_empty()
                        {
                            debug!(
                                target: "vibe_emu_ui::serial",
                                "[SERIAL {frame_index}] {}",
                                format_serial_bytes(&data)
                            );
                        }
                    }

                    if got_frame {
                        for win in windows.values() {
                            win.win.request_redraw();
                        }
                    }
                }
                Event::UserEvent(UserEvent::DebuggerWake) => {
                    if let Ok(mut trace) = exec_trace.lock()
                        && !trace.is_empty()
                    {
                        let drained = std::mem::take(&mut *trace);
                        ui_state
                            .debugger
                            .note_executed_instructions(drained.as_slice());
                    }
                    for win in windows.values() {
                        win.win.request_redraw();
                    }
                }
                Event::UserEvent(UserEvent::DebuggerAck { cmd_id }) => {
                    ui_state.debugger.ack_debug_cmd(*cmd_id);
                    for win in windows.values() {
                        win.win.request_redraw();
                    }
                }
                Event::UserEvent(UserEvent::DebuggerBreak { bank, addr }) => {
                    ui_state.paused = true;
                    ui_state.pending_pause = Some(true);
                    ui_state.menu_resume_armed = false;
                    ui_state.debugger_animate_active = false;
                    ui_state.spawn_debugger = true;
                    ui_state.debugger_pending_focus = true;
                    ui_state.debugger.note_breakpoint_hit(*bank, *addr);
                    for win in windows.values() {
                        win.win.request_redraw();
                    }
                }
                Event::UserEvent(UserEvent::DebuggerWatchpoint { hit }) => {
                    ui_state.paused = true;
                    ui_state.pending_pause = Some(true);
                    ui_state.menu_resume_armed = false;
                    ui_state.debugger_animate_active = false;
                    ui_state.spawn_debugger = true;
                    ui_state.debugger_pending_focus = true;
                    ui_state.debugger.note_watchpoint_hit(hit);
                    ui_state.watchpoints.note_watchpoint_hit(hit);
                    for win in windows.values() {
                        win.win.request_redraw();
                    }
                }
                Event::WindowEvent {
                    window_id,
                    event: win_event,
                    ..
                } => {
                    if let Some(win) = windows.get_mut(window_id) {
                        let (want_capture_mouse, want_capture_keyboard, want_text_input) = {
                            let ui::window::UiWindow {
                                win: window,
                                imgui,
                                platform,
                                ..
                            } = win;

                            let suspended = imgui
                                .take()
                                .expect("UiWindow missing imgui context (internal error)");
                            let mut ctx = suspended.activate().expect(
                                "no ImGui context should be active while handling a window event",
                            );

                            platform.handle_event(ctx.io_mut(), window, &event);
                            let io = ctx.io();
                            let res = (
                                io.want_capture_mouse,
                                io.want_capture_keyboard,
                                io.want_text_input,
                            );
                            *imgui = Some(ctx.suspend());
                            res
                        };

                        // Ensure auxiliary windows (debugger/VRAM) stay responsive even if
                        // emulation frames are being dropped due to backpressure.
                        if matches!(
                            win_event,
                            WindowEvent::CursorMoved { .. }
                                | WindowEvent::MouseInput { .. }
                                | WindowEvent::MouseWheel { .. }
                                | WindowEvent::KeyboardInput { .. }
                        ) {
                            // When paused, the UI must still redraw on input so menu bars and
                            // auxiliary tools remain interactive.
                            if !matches!(win.kind, WindowKind::Main)
                                || ui_state.paused
                                || want_capture_mouse
                                || want_capture_keyboard
                            {
                                win.win.request_redraw();
                            }
                        }
                        match win_event {
                            WindowEvent::ModifiersChanged(mods) => {
                                ui_state.key_modifiers = mods.state();
                            }
                            WindowEvent::CloseRequested => {
                                if matches!(win.kind, WindowKind::Main) {
                                    if !sent_shutdown {
                                        let _ =
                                            to_emu_tx.send(UiToEmu::Command(EmuCommand::Shutdown));
                                        sent_shutdown = true;
                                    }
                                    target.exit();
                                    #[allow(clippy::needless_return)]
                                    return;
                                } else {
                                    windows.remove(window_id);
                                }
                            }
                            WindowEvent::Resized(size) => {
                                win.resize(*size);
                                win.win.request_redraw();
                            }
                            WindowEvent::ScaleFactorChanged { .. } => {
                                // winit 0.30 provides an InnerSizeWriter; the current physical
                                // size can be queried from the window.
                                win.resize(win.win.inner_size());
                                win.win.request_redraw();
                            }
                            WindowEvent::Focused(focused) => {
                                let focused = *focused;

                                if matches!(win.kind, WindowKind::Debugger) {
                                    if focused {
                                        // Always request a paused snapshot on focus so the
                                        // disassembly highlights the current paused PC.
                                        ui_state.pending_pause = Some(true);

                                        if !ui_state.debugger_focus_pause_active {
                                            ui_state.debugger_focus_pause_active = true;
                                            ui_state.debugger_focus_resume_armed = !ui_state.paused;
                                            if !ui_state.paused {
                                                ui_state
                                                    .debugger
                                                    .set_pause_reason(ui::debugger::DebuggerPauseReason::DebuggerFocus);
                                                ui_state.paused = true;
                                                ui_state.pending_pause = Some(true);
                                            }
                                        }
                                    } else if ui_state.debugger_focus_pause_active {
                                        ui_state.debugger_focus_pause_active = false;
                                        if ui_state.debugger_focus_resume_armed {
                                            ui_state.debugger_focus_resume_armed = false;
                                            ui_state.paused = false;
                                            ui_state.pending_pause = Some(false);
                                        }
                                    }
                                }
                            }
                            WindowEvent::KeyboardInput { event, .. }
                                if matches!(win.kind, WindowKind::Main | WindowKind::Options) =>
                            {
                                if let PhysicalKey::Code(code) = event.physical_key {
                                    let pressed = event.state == ElementState::Pressed;

                                    // Key rebinding takes precedence over all bindings.
                                    if pressed && let Some(target) = ui_state.rebinding.take() {
                                        match target {
                                            RebindTarget::Joypad(mask) => {
                                                keybinds.set_joypad_binding(mask, code);
                                            }
                                            RebindTarget::Pause => {
                                                keybinds.set_pause_key(code);
                                            }
                                            RebindTarget::FastForward => {
                                                keybinds.set_fast_forward_key(code);
                                            }
                                            RebindTarget::Quit => {
                                                keybinds.set_quit_key(code);
                                            }
                                        }

                                        if let Err(e) = keybinds.save_to_file(&keybinds_path) {
                                            warn!(
                                                "Failed to save keybinds file {}: {e}",
                                                keybinds_path.display()
                                            );
                                        }

                                        win.win.request_redraw();
                                        return;
                                    }

                                    // In the Options window we only care about rebinding capture.
                                    if matches!(win.kind, WindowKind::Options) {
                                        return;
                                    }

                                    // Quit is always honored.
                                    if code == keybinds.quit_key() {
                                        if pressed {
                                            ui_state.pending_exit = true;
                                        }
                                        return;
                                    }

                                    // Pause toggle is always honored (unless ImGui is typing into a widget).
                                    if code == keybinds.pause_key()
                                        && pressed
                                        && !want_text_input
                                        && !ui_state.menu_pause_active
                                    {
                                        ui_state.paused = !ui_state.paused;
                                        let _ = to_emu_tx.send(UiToEmu::Command(
                                            EmuCommand::SetPaused(ui_state.paused),
                                        ));
                                        win.win.request_redraw();
                                        return;
                                    }

                                    // Fast-forward is a hold action.
                                    if code == keybinds.fast_forward_key() {
                                        speed.fast = pressed;
                                        speed.factor = if speed.fast { FF_MULT } else { 1.0 };
                                        let _ = to_emu_tx
                                            .send(UiToEmu::Command(EmuCommand::SetSpeed(speed)));
                                        return;
                                    }

                                    // Joypad input is disabled while paused or while ImGui is consuming text.
                                    if ui_state.paused || want_text_input {
                                        return;
                                    }

                                    if let Some(mask) = keybinds.joypad_mask_for(code) {
                                        if pressed {
                                            state &= !mask;
                                        } else {
                                            state |= mask;
                                        }
                                        let _ = to_emu_tx
                                            .send(UiToEmu::Command(EmuCommand::UpdateInput(state)));
                                    }
                                }
                            }
                            WindowEvent::KeyboardInput { event, .. }
                                if matches!(win.kind, WindowKind::Debugger) =>
                            {
                                if let PhysicalKey::Code(code) = event.physical_key {
                                    let pressed = event.state == ElementState::Pressed;
                                    let shift = ui_state
                                        .key_modifiers
                                        .contains(winit::keyboard::ModifiersState::SHIFT);
                                    let ctrl = ui_state
                                        .key_modifiers
                                        .contains(winit::keyboard::ModifiersState::CONTROL);
                                    let alt = ui_state
                                        .key_modifiers
                                        .contains(winit::keyboard::ModifiersState::ALT);

                                    if pressed && code == keybinds.quit_key() {
                                        ui_state.pending_exit = true;
                                        return;
                                    }

                                    if want_text_input {
                                        return;
                                    }

                                    if pressed
                                        && code == winit::keyboard::KeyCode::NumpadMultiply
                                    {
                                        ui_state.pending_action = Some(UiAction::Reset);
                                        win.win.request_redraw();
                                    }

                                    if pressed && code == winit::keyboard::KeyCode::F5 {
                                        ui_state.debugger.request_continue_and_focus_main();
                                        win.win.request_redraw();
                                    }

                                    if pressed && code == winit::keyboard::KeyCode::F9 {
                                        if ctrl {
                                            ui_state
                                                .debugger
                                                .request_run_not_this_break_and_focus_main();
                                        } else if shift {
                                            ui_state
                                                .debugger
                                                .request_continue_no_break_and_focus_main();
                                        } else {
                                            ui_state
                                                .debugger
                                                .request_continue_and_focus_main();
                                        }
                                        win.win.request_redraw();
                                    }

                                    if pressed && code == winit::keyboard::KeyCode::F7 {
                                        ui_state.debugger.request_step();
                                        win.win.request_redraw();
                                    }

                                    if pressed && code == winit::keyboard::KeyCode::F3 {
                                        ui_state.debugger.request_step_over();
                                        win.win.request_redraw();
                                    }

                                    if pressed && code == winit::keyboard::KeyCode::F4 {
                                        if shift {
                                            ui_state
                                                .debugger
                                                .request_run_to_cursor_no_break();
                                        } else {
                                            ui_state.debugger.request_run_to_cursor();
                                        }
                                        win.win.request_redraw();
                                    }

                                    if pressed && code == winit::keyboard::KeyCode::F6 {
                                        ui_state.debugger.request_jump_to_cursor();
                                        win.win.request_redraw();
                                    }

                                    if pressed && code == winit::keyboard::KeyCode::F8 {
                                        ui_state.debugger.request_step_out();
                                        win.win.request_redraw();
                                    }

                                    if pressed && alt && code == winit::keyboard::KeyCode::KeyA {
                                        ui_state.debugger.request_toggle_animate();
                                        win.win.request_redraw();
                                    }

                                    if pressed && code == winit::keyboard::KeyCode::F10 {
                                        ui_state.debugger.request_step();
                                        win.win.request_redraw();
                                    }
                                }
                            }
                            WindowEvent::RedrawRequested => {
                                // Ensure the Pixels surface matches the actual window size.
                                // In some multi-window + DPI scenarios we can miss/lose a resize event,
                                // which otherwise leads to wgpu validation panics on scissor.
                                win.ensure_surface_matches_window();

                                let (surface_w, surface_h) = win.surface_size();
                                let is_main = matches!(win.kind, WindowKind::Main);
                                let use_integer_scaling =
                                    ui_config.window_size.use_integer_scaling();
                                let game_scaler = if is_main && use_integer_scaling {
                                    win.game_scaler.clone()
                                } else {
                                    None
                                };
                                let (buffer_w, buffer_h) = win.buffer_size();

                                let ui::window::UiWindow {
                                    win: window,
                                    imgui,
                                    platform,
                                    pixels,
                                    renderer,
                                    kind,
                                    vram_viewer,
                                    ..
                                } = win;

                                let suspended = imgui
                                    .take()
                                    .expect("UiWindow missing imgui context (internal error)");
                                let mut ctx = suspended
                                    .activate()
                                    .expect("no ImGui context should be active while rendering");

                                if let Err(e) = platform.prepare_frame(ctx.io_mut(), window) {
                                    error!("imgui prepare_frame failed: {e}");
                                    *imgui = Some(ctx.suspend());
                                    return;
                                }

                                let fb_scale_y = ctx.io().display_framebuffer_scale[1];
                                let ui = ctx.frame();

                                let top_padding_px = if matches!(*kind, WindowKind::Main) {
                                    (ui.frame_height() * fb_scale_y).ceil().max(0.0) as u32
                                } else {
                                    0
                                };

                                if matches!(*kind, WindowKind::Main)
                                    && !ui_config.window_size.is_fullscreen()
                                    && enforce_main_window_inner_size(
                                        &mut ui_state,
                                        window,
                                        ui_config
                                            .window_size
                                            .scale_factor_px()
                                            .unwrap_or(DEFAULT_WINDOW_SCALE),
                                        top_padding_px,
                                    )
                                {
                                    platform.prepare_render(ui, window);
                                    let _ = ctx.render();
                                    *imgui = Some(ctx.suspend());
                                    window.request_redraw();
                                    return;
                                }

                                match *kind {
                                    WindowKind::Main => {
                                        build_ui(
                                            &mut ui_state,
                                            &mut ui_config,
                                            ui,
                                            mobile.is_some(),
                                        );
                                        draw_game_screen(pixels, &frame);
                                    }
                                    WindowKind::Debugger => {
                                        let snap =
                                            ui_snapshot.read().unwrap_or_else(|e| e.into_inner());
                                        let _ = pixels.frame_mut();
                                        ui_state.debugger.ui(ui, &snap);
                                    }
                                    WindowKind::VramViewer => {
                                        let snap =
                                            ui_snapshot.read().unwrap_or_else(|e| e.into_inner());
                                        draw_vram(vram_viewer.as_mut(), renderer, pixels, &snap, ui)
                                    }
                                    WindowKind::Watchpoints => {
                                        let _ = pixels.frame_mut();
                                        ui_state.watchpoints.ui(ui);
                                    }
                                    WindowKind::Options => {
                                        draw_options_window(
                                            &mut ui_state,
                                            &mut ui_config,
                                            &keybinds,
                                            ui,
                                        );
                                    }
                                }

                                platform.prepare_render(ui, window);
                                let draw_data = ctx.render();

                                let render_result =
                                    pixels.render_with(|encoder, render_target, context| {
                                        if is_main {
                                            if let Some(game_scaler) = game_scaler.as_deref() {
                                                game_scaler.render(
                                                    encoder,
                                                    render_target,
                                                    context,
                                                    surface_w,
                                                    surface_h,
                                                    buffer_w,
                                                    buffer_h,
                                                    top_padding_px,
                                                );
                                            } else {
                                                context
                                                    .scaling_renderer
                                                    .render(encoder, render_target);
                                            }
                                        } else {
                                            context.scaling_renderer.render(encoder, render_target);
                                        }

                                        if draw_data.total_vtx_count > 0 {
                                            let mut rpass = encoder.begin_render_pass(
                                                &wgpu::RenderPassDescriptor {
                                                    label: Some("imgui_pass"),
                                                    color_attachments: &[Some(
                                                        wgpu::RenderPassColorAttachment {
                                                            view: render_target,
                                                            resolve_target: None,
                                                            ops: wgpu::Operations {
                                                                load: wgpu::LoadOp::Load,
                                                                store: true,
                                                            },
                                                        },
                                                    )],
                                                    depth_stencil_attachment: None,
                                                },
                                            );

                                            let (clip_x, clip_y, clip_w, clip_h) =
                                                context.scaling_renderer.clip_rect();

                                            let max_w = surface_w;
                                            let max_h = surface_h;

                                            if max_w == 0
                                                || max_h == 0
                                                || clip_w == 0
                                                || clip_h == 0
                                                || clip_x >= max_w
                                                || clip_y >= max_h
                                            {
                                                return Ok(());
                                            }

                                            let clip_x2 =
                                                (clip_x.saturating_add(clip_w)).min(max_w);
                                            let clip_y2 =
                                                (clip_y.saturating_add(clip_h)).min(max_h);
                                            let clip_w = clip_x2.saturating_sub(clip_x);
                                            let clip_h = clip_y2.saturating_sub(clip_y);

                                            if clip_w == 0 || clip_h == 0 {
                                                return Ok(());
                                            }

                                            rpass.set_scissor_rect(clip_x, clip_y, clip_w, clip_h);

                                            if let Err(e) = renderer.render_clamped(
                                                draw_data,
                                                pixels.queue(),
                                                pixels.device(),
                                                &mut rpass,
                                                [surface_w, surface_h],
                                            ) {
                                                error!("imgui render failed: {e}");
                                                return Ok(());
                                            }
                                        }
                                        Ok(())
                                    });

                                if let Err(e) = render_result {
                                    error!("Pixels render failed: {e}");
                                    target.exit();
                                }

                                *imgui = Some(ctx.suspend());
                            }
                            _ => {}
                        }
                    }
                }
                Event::AboutToWait => {
                    let mut got_frame = false;

                    while let Ok(evt) = from_emu_frame_rx.try_recv() {
                        if let EmuEvent::Frame {
                            frame: mut incoming,
                            frame_index: _,
                        } = evt
                            && incoming.len() == frame.len()
                        {
                            std::mem::swap(&mut frame, &mut incoming);
                            let _ = frame_pool_tx.send_timeout(incoming, Duration::ZERO);
                            got_frame = true;
                        }
                    }

                    while let Ok(evt) = from_emu_serial_rx.try_recv() {
                        if let EmuEvent::Serial { data, frame_index } = evt
                            && !data.is_empty()
                        {
                            debug!(
                                target: "vibe_emu_ui::serial",
                                "[SERIAL {frame_index}] {}",
                                format_serial_bytes(&data)
                            );
                        }
                    }

                    if got_frame {
                        for win in windows.values() {
                            win.win.request_redraw();
                        }
                    }

                    if ui_state.spawn_debugger
                        && !windows
                            .values()
                            .any(|w| matches!(w.kind, WindowKind::Debugger))
                    {
                        spawn_debugger_window(target, &mut windows);
                        ui_state.paused = true;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetPaused(true)));
                        ui_state.spawn_debugger = false;
                    }

                    if ui_state.debugger_pending_focus {
                        if let Some(dbg) = windows
                            .values()
                            .find(|w| matches!(w.kind, WindowKind::Debugger))
                        {
                            request_attention_and_focus(&dbg.win);
                        }
                        ui_state.debugger_pending_focus = false;
                    }
                    if ui_state.spawn_vram
                        && !windows
                            .values()
                            .any(|w| matches!(w.kind, WindowKind::VramViewer))
                    {
                        spawn_vram_window(target, &mut windows);
                        ui_state.paused = true;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetPaused(true)));
                        ui_state.spawn_vram = false;
                    }

                    if ui_state.spawn_options
                        && !windows
                            .values()
                            .any(|w| matches!(w.kind, WindowKind::Options))
                    {
                        spawn_options_window(target, &mut windows);
                        ui_state.spawn_options = false;
                    }

                    if ui_state.spawn_watchpoints
                        && !windows
                            .values()
                            .any(|w| matches!(w.kind, WindowKind::Watchpoints))
                    {
                        spawn_watchpoints_window(target, &mut windows);
                        ui_state.paused = true;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetPaused(true)));
                        ui_state.spawn_watchpoints = false;
                    }

                    if ui_state.pending_exit {
                        ui_state.pending_exit = false;
                        ui_state.pending_save_ui_config = true;
                        if !sent_shutdown {
                            let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::Shutdown));
                            sent_shutdown = true;
                        }
                        target.exit();
                        return;
                    }

                    if let Some(peripheral) = ui_state.pending_serial_peripheral.take() {
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetSerialPeripheral(
                            peripheral,
                        )));
                    }

                    let dbg_actions = ui_state.debugger.take_actions();
                    let dbg_has_run_to = dbg_actions.request_run_to.is_some();
                    if dbg_actions.breakpoints_updated {
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetBreakpoints(
                            dbg_actions.breakpoints,
                        )));
                    }

                    let wp_actions = ui_state.watchpoints.take_actions();
                    if wp_actions.watchpoints_updated {
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetWatchpoints(
                            wp_actions.watchpoints,
                        )));
                    }

                    if dbg_actions.request_toggle_animate {
                        ui_state.debugger_animate_active = !ui_state.debugger_animate_active;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetAnimate(
                            ui_state.debugger_animate_active,
                        )));

                        if ui_state.debugger_animate_active && ui_state.paused {
                            ui_state.paused = false;
                            ui_state.pending_pause = None;
                            let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::Resume {
                                ignore_breakpoints: false,
                            }));
                        }
                    }

                    if let Some(addr) = dbg_actions.request_jump_to_cursor {
                        ui_state.paused = true;
                        ui_state.pending_pause = None;
                        ui_state.debugger_animate_active = false;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::JumpTo { addr }));
                    }

                    if let Some(addr) = dbg_actions.request_call_cursor {
                        ui_state.paused = true;
                        ui_state.pending_pause = None;
                        ui_state.debugger_animate_active = false;
                        let _ =
                            to_emu_tx.send(UiToEmu::Command(EmuCommand::CallCursor { addr }));
                    }

                    if dbg_actions.request_jump_sp {
                        ui_state.paused = true;
                        ui_state.pending_pause = None;
                        ui_state.debugger_animate_active = false;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::JumpSp));
                    }

                    if let Some(cmd_id) = dbg_actions.request_step {
                        ui_state.paused = true;
                        ui_state.pending_pause = Some(true);
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::Step {
                            count: 1,
                            cmd_id: Some(cmd_id),
                            guarantee_snapshot: true,
                        }));
                    }
                    if let Some(req) = dbg_actions.request_run_to {
                        ui_state.paused = false;
                        ui_state.pending_pause = None;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::RunTo {
                            target: req.target,
                            ignore_breakpoints: req.ignore_breakpoints,
                        }));
                    }
                    if dbg_actions.request_pause {
                        ui_state.paused = true;
                        ui_state.pending_pause = Some(true);
                        ui_state.debugger_animate_active = false;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetAnimate(false)));
                    }
                    if let Some(breakpoint) = dbg_actions.request_continue_ignore_once {
                        if !dbg_has_run_to {
                            ui_state.paused = false;
                            ui_state.pending_pause = None;
                            let _ = to_emu_tx.send(UiToEmu::Command(
                                EmuCommand::ResumeIgnoreOnce { breakpoint },
                            ));
                        }

                        if let Some(main) = windows
                            .values()
                            .find(|w| matches!(w.kind, WindowKind::Main))
                        {
                            request_attention_and_focus(&main.win);
                        }
                    } else if dbg_actions.request_continue || dbg_actions.request_continue_no_break {
                        if !dbg_has_run_to {
                            ui_state.paused = false;
                            ui_state.pending_pause = None;
                            let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::Resume {
                                ignore_breakpoints: dbg_actions.request_continue_no_break,
                            }));
                        }

                        if let Some(main) = windows
                            .values()
                            .find(|w| matches!(w.kind, WindowKind::Main))
                        {
                            request_attention_and_focus(&main.win);
                        }
                    }

                    if let Some(paused) = ui_state.pending_pause.take() {
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetPaused(paused)));
                    }

                    if ui_state.pending_load_config_update {
                        ui_state.pending_load_config_update = false;
                        let updated = build_load_config(
                            &ui_config,
                            args.dmg_neutral,
                            load_config.bootrom_override.clone(),
                        );
                        load_config = updated.clone();
                        let _ =
                            to_emu_tx.send(UiToEmu::Command(EmuCommand::UpdateLoadConfig(updated)));
                    }

                    if let Some(mode) = ui_state.pending_window_size.take() {
                        if let Some(main) = windows
                            .values()
                            .find(|w| matches!(w.kind, WindowKind::Main))
                        {
                            apply_window_size_setting(&main.win, mode);
                        }
                        ui_state.last_main_inner_size = None;
                    }

                    if ui_state.pending_save_ui_config {
                        ui_state.pending_save_ui_config = false;
                        if let Err(e) = ui_config::save_to_file(&ui_config_path, &ui_config) {
                            warn!("Failed to save UI config {}: {e}", ui_config_path.display());
                        }
                    }

                    if let Some(action) = ui_state.pending_action.take() {
                        let _ = to_emu_tx.send(UiToEmu::Action(action));
                        ui_state.paused = false;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetPaused(false)));
                        got_frame = true;
                    }

                    if got_frame
                        && let Some(main) = windows
                            .values()
                            .find(|w| matches!(w.kind, WindowKind::Main))
                    {
                        main.win.request_redraw();
                    }
                }
                Event::LoopExiting => {
                    if !sent_shutdown {
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::Shutdown));
                        sent_shutdown = true;
                    }
                    if let Some(handle) = emu_handle.take() {
                        let _ = handle.join();
                    }
                }
                _ => {}
            }
        });
    } else {
        let mut frame_count = 0u64;
        let start = Instant::now();
        let mut mobile_time_accum_ns: u128 = 0;
        let mobile_active = serial_peripheral == SerialPeripheral::MobileAdapter;
        'headless: loop {
            while !gb.mmu.ppu.frame_ready() {
                gb.cpu.step(&mut gb.mmu);
                if let Some(max) = cycle_limit
                    && gb.cpu.cycles >= max
                {
                    break 'headless;
                }
                if let Some(limit) = second_limit
                    && start.elapsed() >= limit
                {
                    break 'headless;
                }
            }

            frame.copy_from_slice(gb.mmu.ppu.framebuffer());
            gb.mmu.ppu.clear_frame_flag();

            if debug_enabled && frame_count.is_multiple_of(60) {
                let serial = gb.mmu.take_serial();
                if !serial.is_empty() {
                    debug!(
                        target: "vibe_emu_ui::serial",
                        "[SERIAL] {}",
                        format_serial_bytes(&serial)
                    );
                }

                debug!(target: "vibe_emu_ui::cpu", "{}", gb.cpu.debug_state());
            }

            frame_count += 1;

            if mobile_active && let Some(mobile) = mobile.as_ref() {
                mobile_time_accum_ns += FRAME_TIME.as_nanos();
                let delta_ms = (mobile_time_accum_ns / 1_000_000) as u32;
                mobile_time_accum_ns %= 1_000_000;
                if delta_ms != 0
                    && let Ok(mut adapter) = mobile.lock()
                {
                    let _ = adapter.poll(delta_ms);
                }
            }

            if let Some(max) = frame_limit
                && frame_count >= max as u64
            {
                break;
            }
            if let Some(limit) = second_limit
                && start.elapsed() >= limit
            {
                break;
            }
        }

        if let Some(mobile) = mobile.as_ref()
            && let Ok(mut adapter) = mobile.lock()
        {
            let _ = adapter.stop();
        }
    }
}

#[cfg(test)]
mod run_to_regression_tests {
    use super::*;
    use std::time::Instant;
    use vibe_emu_core::cartridge::Cartridge;
    use vibe_emu_core::gameboy::GameBoy;

    fn build_vblank_run_to_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];

        // Header: no MBC.
        rom[0x0147] = 0x00;

        // Main loop at 0x0100: EI; NOP; HALT; JP 0x0102.
        rom[0x0100] = 0xFB;
        rom[0x0101] = 0x00;
        rom[0x0102] = 0x76;
        rom[0x0103] = 0xC3;
        rom[0x0104] = 0x02;
        rom[0x0105] = 0x01;

        // VBlank interrupt vector: JP 0x018E.
        rom[0x0040] = 0xC3;
        rom[0x0041] = 0x8E;
        rom[0x0042] = 0x01;

        // VBlank handler at 0x018E: NOP; NOP; NOP; NOP; RETI.
        rom[0x018E] = 0x00;
        rom[0x018F] = 0x00;
        rom[0x0190] = 0x00;
        rom[0x0191] = 0x00;
        rom[0x0192] = 0xD9;

        rom
    }

    fn snapshot_or_default(ui_snapshot: &Arc<RwLock<UiSnapshot>>) -> UiSnapshot {
        ui_snapshot
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    fn wait_for_break_at(
        ui_snapshot: &Arc<RwLock<UiSnapshot>>,
        addr: u16,
        timeout: Duration,
    ) -> UiSnapshot {
        let start = Instant::now();
        loop {
            let snap = snapshot_or_default(ui_snapshot);
            if snap.debugger.paused && snap.cpu.pc == addr {
                return snap;
            }
            if start.elapsed() > timeout {
                panic!(
                    "timed out waiting for break at {:04X} (last pc={:04X} paused={})",
                    addr, snap.cpu.pc, snap.debugger.paused
                );
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn run_to_cursor_does_not_take_a_frame_of_cycles() {
        #[cfg(target_os = "linux")]
        {
            let has_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var_os("WAYLAND_SOCKET").is_some();
            let has_x11 = std::env::var_os("DISPLAY").is_some();
            if !(has_wayland || has_x11) {
                return;
            }
        }

        let rom = build_vblank_run_to_rom();
        let cart = Cartridge::load(rom);

        let mut gb = GameBoy::new();
        gb.mmu.load_cart(cart);
        gb.mmu.write_byte(0xFFFF, 0x01); // IE: VBlank
        gb.mmu.write_byte(0xFF40, 0x91); // LCDC: LCD on

        let gb = Arc::new(Mutex::new(gb));
        let ui_snapshot = Arc::new(RwLock::new(UiSnapshot::default()));

        let (to_emu_tx, to_emu_rx) = mpsc::channel::<UiToEmu>();
        let (frame_tx, _frame_rx) = cb::unbounded::<EmuEvent>();
        let (serial_tx, _serial_rx) = cb::unbounded::<EmuEvent>();
        let (frame_pool_tx, frame_pool_rx) = cb::unbounded::<Vec<u32>>();
        let _ = frame_pool_tx.send(vec![0u32; 160 * 144]);

        let event_loop = {
            let mut builder = EventLoop::<UserEvent>::with_user_event();

            #[cfg(target_os = "windows")]
            {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                builder.with_any_thread(true);
            }

            // winit enforces main-thread event loop creation on some Linux backends.
            // Tests run on worker threads, so opt into the platform escape hatch.
            #[cfg(target_os = "linux")]
            {
                #[allow(unused_imports)]
                use winit::platform::wayland::EventLoopBuilderExtWayland;
                #[allow(unused_imports)]
                use winit::platform::x11::EventLoopBuilderExtX11;

                let _ = winit::platform::wayland::EventLoopBuilderExtWayland::with_any_thread(
                    &mut builder,
                    true,
                );
                let _ = winit::platform::x11::EventLoopBuilderExtX11::with_any_thread(
                    &mut builder,
                    true,
                );
            }

            builder
                .build()
                .expect("failed to build winit event loop for tests")
        };
        let wake_proxy = event_loop.create_proxy();

        let exec_trace = Arc::new(Mutex::new(Vec::<ui::code_data::ExecutedInstruction>::new()));

        let channels = EmuThreadChannels {
            rx: to_emu_rx,
            frame_tx,
            serial_tx,
            frame_pool_tx,
            frame_pool_rx,
            wake_proxy,
            exec_trace,
        };

        let speed = Speed {
            factor: 1.0,
            fast: true,
        };

        let load_config = LoadConfig {
            emulation_mode: EmulationMode::ForceDmg,
            dmg_neutral: false,
            bootrom_override: None,
            dmg_bootrom_path: None,
            cgb_bootrom_path: None,
        };

        let emu_thread = {
            let gb = Arc::clone(&gb);
            let ui_snapshot = Arc::clone(&ui_snapshot);
            thread::spawn(move || {
                run_emulator_thread(
                    gb,
                    ui_snapshot,
                    speed,
                    false,
                    false,
                    None,
                    SerialPeripheral::None,
                    false,
                    load_config,
                    channels,
                )
            })
        };

        // Break at the start of the VBlank handler.
        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetBreakpoints(vec![
            ui::debugger::BreakpointSpec {
                bank: 0x00,
                addr: 0x018E,
            },
        ])));

        let first = wait_for_break_at(&ui_snapshot, 0x018E, Duration::from_secs(2));
        let cycles_at_018e = first.cpu.cycles;

        // Clear the breakpoint so continue doesn't immediately re-break.
        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetBreakpoints(Vec::new())));

        // Run to a nearby instruction in the same handler.
        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::RunTo {
            target: ui::debugger::BreakpointSpec {
                bank: 0x00,
                addr: 0x0191,
            },
            ignore_breakpoints: false,
        }));

        let second = wait_for_break_at(&ui_snapshot, 0x0191, Duration::from_secs(2));
        let delta_cycles = second.cpu.cycles.saturating_sub(cycles_at_018e);

        assert!(
            delta_cycles < 256,
            "run-to took too long: delta_cycles={delta_cycles} (expected <256)"
        );

        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::Shutdown));
        let _ = emu_thread.join();
        drop(event_loop);
    }
}
