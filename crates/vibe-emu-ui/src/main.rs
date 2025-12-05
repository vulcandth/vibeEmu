#![allow(dead_code)]

mod audio;
mod ui;

use clap::Parser;
use cpal::traits::StreamTrait;
use imgui::{ConfigFlags, Context as ImguiContext};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use log::{error, info, warn};
use pixels::{Pixels, SurfaceTexture};
use rfd::FileDialog;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::CgbRevision};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Icon, Window, WindowAttributes};

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

const SCALE: u32 = 2;
const GB_FPS: f64 = 59.7275;
const FRAME_TIME: Duration = Duration::from_nanos((1e9_f64 / GB_FPS) as u64);
const FF_MULT: f32 = 4.0;
const AUDIO_WARMUP_TARGET_RATIO: f32 = 0.9;
const AUDIO_WARMUP_CHECK_INTERVAL: u32 = 1024;
const AUDIO_WARMUP_TIMEOUT_MS: u64 = 200;

#[derive(Default)]
struct UiState {
    paused: bool,
    show_context: bool,
    ctx_pos: [f32; 2],
    spawn_debugger: bool,
    spawn_vram: bool,
    pending_action: Option<UiAction>,
}

enum UiAction {
    Reset,
    Load(Cartridge),
}

#[derive(Clone, Copy)]
struct Speed {
    factor: f32,
    fast: bool,
}

#[derive(Clone, Copy)]
enum EmuCommand {
    SetPaused(bool),
    SetSpeed(Speed),
    UpdateInput(u8),
    Shutdown,
}

enum UiToEmu {
    Command(EmuCommand),
    Action(UiAction),
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
}

fn cursor_in_screen(window: &winit::window::Window, pos: PhysicalPosition<f64>) -> bool {
    let size = window.inner_size();
    let width = (160 * SCALE) as f64;
    let height = (144 * SCALE) as f64;
    let x_in = pos.x >= 0.0 && pos.x < width.min(size.width as f64);
    let y_in = pos.y >= 0.0 && pos.y < height.min(size.height as f64);
    x_in && y_in
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

fn spawn_debugger_window(
    event_loop: &ActiveEventLoop,
    platform: &mut WinitPlatform,
    imgui: &mut ImguiContext,
    windows: &mut HashMap<winit::window::WindowId, UiWindow>,
) {
    use winit::dpi::LogicalSize;
    let attrs = enforce_square_corners(
        Window::default_attributes()
            .with_title("vibeEmu \u{2013} Debugger")
            .with_window_icon(load_window_icon())
            .with_inner_size(LogicalSize::new((160 * SCALE) as f64, (144 * SCALE) as f64)),
    );
    let w = event_loop.create_window(attrs).unwrap();

    let size = w.inner_size();
    let surface = pixels::SurfaceTexture::new(size.width, size.height, &w);
    let pixels = pixels::Pixels::new(1, 1, surface).expect("Pixels error");

    platform.attach_window(imgui.io_mut(), &w, HiDpiMode::Rounded);

    let ui_win = UiWindow::new(WindowKind::Debugger, w, pixels, (1, 1), imgui);
    let id = ui_win.win.id();
    windows.insert(id, ui_win);
    if let Some(win) = windows.get_mut(&id) {
        win.resize(win.win.inner_size());
    }
}

fn spawn_vram_window(
    event_loop: &ActiveEventLoop,
    platform: &mut WinitPlatform,
    imgui: &mut ImguiContext,
    windows: &mut HashMap<winit::window::WindowId, UiWindow>,
) {
    use winit::dpi::LogicalSize;
    let attrs = enforce_square_corners(
        Window::default_attributes()
            .with_title("vibeEmu \u{2013} VRAM")
            .with_window_icon(load_window_icon())
            .with_inner_size(LogicalSize::new((160 * SCALE) as f64, (144 * SCALE) as f64)),
    );
    let w = event_loop.create_window(attrs).unwrap();

    let size = w.inner_size();
    let surface = pixels::SurfaceTexture::new(size.width, size.height, &w);
    let pixels = pixels::Pixels::new(1, 1, surface).expect("Pixels error");

    platform.attach_window(imgui.io_mut(), &w, HiDpiMode::Rounded);

    let ui_win = UiWindow::new(WindowKind::VramViewer, w, pixels, (1, 1), imgui);
    let id = ui_win.win.id();
    windows.insert(id, ui_win);
    if let Some(win) = windows.get_mut(&id) {
        win.resize(win.win.inner_size());
    }
}

fn run_emulator_thread(
    gb: Arc<Mutex<GameBoy>>,
    mut speed: Speed,
    debug: bool,
    rx: mpsc::Receiver<UiToEmu>,
    tx: mpsc::Sender<EmuEvent>,
) {
    let mut paused = false;
    let mut frame_count = 0u64;
    let mut next_frame = Instant::now() + FRAME_TIME;
    let mut audio_stream = None;

    if let Ok(mut gb) = gb.lock() {
        rebuild_audio_stream(&mut gb, speed, &mut audio_stream);
    }

    loop {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                UiToEmu::Command(cmd) => match cmd {
                    EmuCommand::SetPaused(p) => {
                        paused = p;
                        next_frame = Instant::now() + FRAME_TIME;
                    }
                    EmuCommand::SetSpeed(new_speed) => {
                        speed = new_speed;
                        next_frame = Instant::now() + FRAME_TIME;
                        if let Ok(gb) = gb.lock()
                            && let Ok(mut apu) = gb.mmu.apu.lock()
                        {
                            apu.set_speed(speed.factor);
                        }
                    }
                    EmuCommand::UpdateInput(state) => {
                        if let Ok(mut gb) = gb.lock() {
                            let mmu = &mut gb.mmu;
                            let if_reg = &mut mmu.if_reg;
                            mmu.input.update_state(state, if_reg);
                        }
                    }
                    EmuCommand::Shutdown => {
                        if let Ok(mut gb) = gb.lock() {
                            gb.mmu.save_cart_ram();
                        }
                        return;
                    }
                },
                UiToEmu::Action(action) => {
                    if let Ok(mut gb) = gb.lock() {
                        apply_ui_action(action, &mut gb, &mut audio_stream, speed);
                        gb.mmu.ppu.clear_frame_flag();
                        frame_count = 0;
                        next_frame = Instant::now() + FRAME_TIME;
                    }
                }
            }
        }

        if paused {
            thread::sleep(Duration::from_millis(1));
            continue;
        }

        let frame_start = Instant::now();
        let mut frame_buf = None;
        let mut serial = None;

        if let Ok(mut gb) = gb.lock() {
            if let Ok(mut apu) = gb.mmu.apu.lock() {
                apu.set_speed(speed.factor);
            }

            {
                let (cpu, mmu) = {
                    let GameBoy { cpu, mmu, .. } = &mut *gb;
                    (cpu, mmu)
                };
                while !mmu.ppu.frame_ready() {
                    cpu.step(mmu);
                }

                frame_buf = Some(mmu.ppu.framebuffer().to_vec());
                mmu.ppu.clear_frame_flag();
            }

            if !speed.fast {
                let elapsed = frame_start.elapsed();
                let warn_threshold = FRAME_TIME + FRAME_TIME / 2;
                if elapsed > warn_threshold {
                    if let Ok(apu) = gb.mmu.apu.lock() {
                        warn!(
                            "Frame emulation exceeded budget: {:?} vs {:?} (audio queue {} / {})",
                            elapsed,
                            FRAME_TIME,
                            apu.queued_frames(),
                            apu.max_queue_capacity()
                        );
                    } else {
                        warn!(
                            "Frame emulation exceeded budget: {:?} vs {:?} (audio queue unavailable)",
                            elapsed, FRAME_TIME
                        );
                    }
                }
            }

            if debug && frame_count.is_multiple_of(60) {
                let out = gb.mmu.take_serial();
                if !out.is_empty() {
                    serial = Some(out);
                }
                println!("{}", gb.cpu.debug_state());
            }
        }

        if let Some(frame) = frame_buf
            && tx
                .send(EmuEvent::Frame {
                    frame,
                    frame_index: frame_count,
                })
                .is_err()
        {
            break;
        }

        if let Some(serial) = serial {
            let _ = tx.send(EmuEvent::Serial {
                data: serial,
                frame_index: frame_count,
            });
        }

        frame_count = frame_count.wrapping_add(1);

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

fn draw_debugger(pixels: &mut Pixels, gb: &Arc<Mutex<GameBoy>>, ui: &imgui::Ui) {
    let _ = pixels.frame_mut();
    if let Ok(gb) = gb.try_lock() {
        if let Some(_table) = ui.begin_table("regs", 2) {
            ui.table_next_row();
            ui.table_next_column();
            ui.text("A");
            ui.table_next_column();
            ui.text(format!("{:02X}", gb.cpu.a));

            ui.table_next_column();
            ui.text("F");
            ui.table_next_column();
            ui.text(format!("{:02X}", gb.cpu.f));

            ui.table_next_column();
            ui.text("B");
            ui.table_next_column();
            ui.text(format!("{:02X}", gb.cpu.b));

            ui.table_next_column();
            ui.text("C");
            ui.table_next_column();
            ui.text(format!("{:02X}", gb.cpu.c));

            ui.table_next_column();
            ui.text("D");
            ui.table_next_column();
            ui.text(format!("{:02X}", gb.cpu.d));

            ui.table_next_column();
            ui.text("E");
            ui.table_next_column();
            ui.text(format!("{:02X}", gb.cpu.e));

            ui.table_next_column();
            ui.text("H");
            ui.table_next_column();
            ui.text(format!("{:02X}", gb.cpu.h));

            ui.table_next_column();
            ui.text("L");
            ui.table_next_column();
            ui.text(format!("{:02X}", gb.cpu.l));

            ui.table_next_column();
            ui.text("SP");
            ui.table_next_column();
            ui.text(format!("{:04X}", gb.cpu.sp));

            ui.table_next_column();
            ui.text("PC");
            ui.table_next_column();
            ui.text(format!("{:04X}", gb.cpu.pc));

            ui.table_next_column();
            ui.text("IME");
            ui.table_next_column();
            ui.text(format!("{}", gb.cpu.ime));

            ui.table_next_column();
            ui.text("Cycles");
            ui.table_next_column();
            ui.text(format!("{}", gb.cpu.cycles));
        }
    } else {
        ui.text("Emulator busy; debugger waiting for state");
    }
}

fn draw_vram(win: &mut ui::window::UiWindow, gb: &Arc<Mutex<GameBoy>>, ui: &imgui::Ui) {
    let _ = win.pixels.frame_mut();
    if let Ok(mut gb) = gb.try_lock() {
        if let Some(viewer) = win.vram_viewer.as_mut() {
            viewer.ui(
                ui,
                &mut gb.mmu.ppu,
                &mut win.renderer,
                win.pixels.device(),
                win.pixels.queue(),
            );
        } else {
            ui.text("VRAM viewer not initialized");
        }
    } else {
        ui.text("Emulator busy; VRAM view unavailable");
    }
}

fn draw_game_screen(pixels: &mut Pixels, frame: &[u32]) {
    for (dst, &src) in pixels.frame_mut().chunks_exact_mut(4).zip(frame.iter()) {
        let r = ((src >> 16) & 0xFF) as u8;
        let g = ((src >> 8) & 0xFF) as u8;
        let b = (src & 0xFF) as u8;
        dst[0] = r;
        dst[1] = g;
        dst[2] = b;
        dst[3] = 0xFF;
    }
}

#[allow(clippy::too_many_arguments)]
fn build_ui(state: &mut UiState, ui: &imgui::Ui) {
    if state.show_context {
        let flags = imgui::WindowFlags::NO_TITLE_BAR
            | imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_DECORATION;
        let mut open = state.show_context;
        let mut close_menu = false;
        ui.window("ctx")
            .position(state.ctx_pos, imgui::Condition::Always)
            .flags(flags)
            .always_auto_resize(true)
            .opened(&mut open)
            .build(|| {
                if ui.button("Load ROM") {
                    if let Some(path) = FileDialog::new()
                        .add_filter("Game Boy ROM", &["gb", "gbc"])
                        .pick_file()
                        && let Ok(cart) = Cartridge::from_file(&path)
                    {
                        state.pending_action = Some(UiAction::Load(cart));
                        state.paused = false;
                    }
                    close_menu = true;
                }
                if ui.button("Reset GB") {
                    state.pending_action = Some(UiAction::Reset);
                    state.paused = false;
                    close_menu = true;
                }
                if ui.button("Debugger") {
                    state.spawn_debugger = true;
                    close_menu = true;
                }
                if ui.button("VRAM Viewer") {
                    state.spawn_vram = true;
                    close_menu = true;
                }
            });
        state.show_context = open && !close_menu;
    }
}

fn configure_wgpu_backend() {
    if std::env::var_os("WGPU_BACKEND").is_none() {
        // Prefer DirectX on Windows to avoid buggy Vulkan/ANGLE present modes.
        unsafe {
            std::env::set_var("WGPU_BACKEND", "dx12");
        }
    }
}

fn prime_audio_queue(gb: &mut GameBoy) -> (usize, usize) {
    let capacity = {
        let apu = gb.mmu.apu.lock().unwrap();
        apu.max_queue_capacity().max(1)
    };
    let target_frames = ((capacity as f32) * AUDIO_WARMUP_TARGET_RATIO).ceil() as usize;

    let mut queued = {
        let apu = gb.mmu.apu.lock().unwrap();
        apu.queued_frames()
    };
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
            let apu = gb.mmu.apu.lock().unwrap();
            queued = apu.queued_frames();
            if queued >= target_frames || now >= deadline {
                break;
            }
        }
    }

    if queued < target_frames {
        queued = {
            let apu = gb.mmu.apu.lock().unwrap();
            apu.queued_frames()
        };
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

    if let Ok(mut apu) = gb.mmu.apu.lock() {
        apu.set_speed(speed.factor);
    }

    *audio_stream = audio::start_stream(Arc::clone(&gb.mmu.apu), false);
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
) {
    match action {
        UiAction::Reset => {
            info!("Resetting Game Boy");
            gb.reset();
        }
        UiAction::Load(cart) => {
            info!("Loading new ROM");
            gb.reset();
            gb.mmu.load_cart(cart);
        }
    }

    rebuild_audio_stream(gb, speed, audio_stream);
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    configure_wgpu_backend();

    let args = Args::parse();

    info!("Starting emulator");

    let rom_path = match args.rom {
        Some(p) => p,
        None => {
            error!("No ROM supplied");
            return;
        }
    };

    let cart = match Cartridge::from_file(&rom_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load ROM: {e}");
            return;
        }
    };

    let cgb_mode = if args.dmg {
        false
    } else if args.cgb {
        true
    } else {
        cart.cgb
    };
    let mut gb = GameBoy::new_with_revision(cgb_mode, CgbRevision::default());
    gb.mmu.load_cart(cart);
    // If user requested a neutral/non-green DMG palette, apply it.
    if !cgb_mode && args.dmg_neutral {
        const NEUTRAL_DMG_PALETTE: [u32; 4] = [0x00E0F8D0, 0x0088C070, 0x00346856, 0x00081820];
        gb.mmu.ppu.set_dmg_palette(NEUTRAL_DMG_PALETTE);
    }

    if let Some(path) = args.bootrom {
        match std::fs::read(&path) {
            Ok(data) => gb.mmu.load_boot_rom(data),
            Err(e) => warn!("Failed to load boot ROM: {e}"),
        }
    }

    info!(
        "Emulator initialized in {} mode",
        if cgb_mode { "CGB" } else { "DMG" }
    );

    let headless = args.headless;
    let debug_enabled = args.debug;
    let frame_limit = args.frames;
    let cycle_limit = args.cycles;
    let second_limit = args.seconds.map(Duration::from_secs);

    let mut frame = vec![0u32; 160 * 144];

    if !headless {
        let gb = Arc::new(Mutex::new(gb));
        let mut speed = Speed {
            factor: 1.0,
            fast: false,
        };
        let mut ui_state = UiState::default();

        let (to_emu_tx, to_emu_rx) = mpsc::channel();
        let (from_emu_tx, from_emu_rx) = mpsc::channel();
        let emu_gb = Arc::clone(&gb);
        let emu_handle = thread::spawn(move || {
            run_emulator_thread(emu_gb, speed, debug_enabled, to_emu_rx, from_emu_tx);
        });
        let mut emu_handle = Some(emu_handle);
        let mut sent_shutdown = false;

        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::UpdateInput(0xFF)));

        let event_loop = EventLoop::builder().build().unwrap();
        let attrs = enforce_square_corners(
            Window::default_attributes()
                .with_title("vibeEmu")
                .with_window_icon(load_window_icon())
                .with_inner_size(winit::dpi::LogicalSize::new(
                    (160 * SCALE) as f64,
                    (144 * SCALE) as f64,
                )),
        );
        #[allow(deprecated)]
        let window = event_loop.create_window(attrs).unwrap();

        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, &window);
        let pixels = Pixels::new(160, 144, surface).expect("Pixels error");

        let mut imgui = ImguiContext::create();
        imgui.io_mut().config_flags |= ConfigFlags::DOCKING_ENABLE;
        let mut platform = WinitPlatform::new(&mut imgui);
        platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Rounded);

        let mut windows = HashMap::new();
        let main_win = UiWindow::new(WindowKind::Main, window, pixels, (160, 144), &mut imgui);
        let main_id = main_win.win.id();
        windows.insert(main_id, main_win);
        if let Some(win) = windows.get_mut(&main_id) {
            win.resize(win.win.inner_size());
        }

        let mut state = 0xFFu8;
        let mut cursor_pos = PhysicalPosition::new(0.0, 0.0);

        #[allow(deprecated)]
        let _ = event_loop.run(move |event, target| {
            target.set_control_flow(ControlFlow::Poll);
            match &event {
                Event::WindowEvent {
                    window_id,
                    event: win_event,
                    ..
                } => {
                    if let Some(win) = windows.get_mut(window_id) {
                        platform.handle_event(imgui.io_mut(), &win.win, &event);
                        if ui_state.show_context
                            && imgui.io().want_capture_mouse
                            && matches!(win_event, WindowEvent::MouseInput { .. })
                        {
                            return;
                        }
                        match win_event {
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
                            }
                            WindowEvent::ScaleFactorChanged { .. } => {
                                let size = win.win.inner_size();
                                let _ = win.win.request_inner_size(size);
                                win.resize(size);
                            }
                            WindowEvent::CursorMoved { position, .. }
                                if matches!(win.kind, WindowKind::Main) =>
                            {
                                cursor_pos = *position;
                            }
                            WindowEvent::MouseInput {
                                state: ElementState::Pressed,
                                button: MouseButton::Right,
                                ..
                            } if matches!(win.kind, WindowKind::Main) => {
                                if !ui_state.paused && cursor_in_screen(&win.win, cursor_pos) {
                                    ui_state.paused = true;
                                    ui_state.show_context = true;
                                    ui_state.ctx_pos = [cursor_pos.x as f32, cursor_pos.y as f32];
                                    let _ = to_emu_tx
                                        .send(UiToEmu::Command(EmuCommand::SetPaused(true)));
                                }
                            }
                            WindowEvent::MouseInput {
                                state: ElementState::Pressed,
                                button: MouseButton::Left,
                                ..
                            } if matches!(win.kind, WindowKind::Main) => {
                                if ui_state.show_context && imgui.io().want_capture_mouse {
                                    // Menu click handled by ImGui
                                } else if cursor_in_screen(&win.win, cursor_pos) {
                                    ui_state.paused = false;
                                    ui_state.show_context = false;
                                    let _ = to_emu_tx
                                        .send(UiToEmu::Command(EmuCommand::SetPaused(false)));
                                }
                            }
                            WindowEvent::KeyboardInput { event, .. }
                                if matches!(win.kind, WindowKind::Main) =>
                            {
                                if !(ui_state.paused || imgui.io().want_text_input)
                                    && let PhysicalKey::Code(code) = event.physical_key
                                {
                                    let pressed = event.state == ElementState::Pressed;
                                    let mask = if code == KeyCode::Space {
                                        speed.fast = pressed;
                                        speed.factor = if speed.fast { FF_MULT } else { 1.0 };
                                        let _ = to_emu_tx
                                            .send(UiToEmu::Command(EmuCommand::SetSpeed(speed)));
                                        None
                                    } else {
                                        match code {
                                            KeyCode::ArrowRight => Some(0x01),
                                            KeyCode::ArrowLeft => Some(0x02),
                                            KeyCode::ArrowUp => Some(0x04),
                                            KeyCode::ArrowDown => Some(0x08),
                                            KeyCode::KeyS => Some(0x10),
                                            KeyCode::KeyA => Some(0x20),
                                            KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(0x40),
                                            KeyCode::Enter => Some(0x80),
                                            KeyCode::Escape => {
                                                if pressed {
                                                    target.exit();
                                                }
                                                None
                                            }
                                            _ => None,
                                        }
                                    };
                                    if let Some(mask) = mask {
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
                            WindowEvent::RedrawRequested => {
                                platform.prepare_frame(imgui.io_mut(), &win.win).unwrap();
                                let ui = imgui.frame();

                                match win.kind {
                                    WindowKind::Main => {
                                        build_ui(&mut ui_state, ui);
                                        draw_game_screen(&mut win.pixels, &frame);
                                    }
                                    WindowKind::Debugger => draw_debugger(&mut win.pixels, &gb, ui),
                                    WindowKind::VramViewer => draw_vram(win, &gb, ui),
                                }

                                platform.prepare_render(ui, &win.win);
                                let draw_data = imgui.render();

                                let render_result =
                                    win.pixels.render_with(|encoder, render_target, context| {
                                        context.scaling_renderer.render(encoder, render_target);

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

                                            if clip_w == 0 || clip_h == 0 {
                                                return Ok(());
                                            }

                                            rpass.set_scissor_rect(clip_x, clip_y, clip_w, clip_h);

                                            win.renderer
                                                .render(
                                                    draw_data,
                                                    win.pixels.queue(),
                                                    win.pixels.device(),
                                                    &mut rpass,
                                                )
                                                .expect("imgui render failed");
                                        }
                                        Ok(())
                                    });
                                if render_result.is_err() {
                                    target.exit();
                                }

                                if ui_state.spawn_debugger
                                    && !windows
                                        .values()
                                        .any(|w| matches!(w.kind, WindowKind::Debugger))
                                {
                                    spawn_debugger_window(
                                        target,
                                        &mut platform,
                                        &mut imgui,
                                        &mut windows,
                                    );
                                    ui_state.paused = true;
                                    let _ = to_emu_tx
                                        .send(UiToEmu::Command(EmuCommand::SetPaused(true)));
                                    ui_state.spawn_debugger = false;
                                }
                                if ui_state.spawn_vram
                                    && !windows
                                        .values()
                                        .any(|w| matches!(w.kind, WindowKind::VramViewer))
                                {
                                    spawn_vram_window(
                                        target,
                                        &mut platform,
                                        &mut imgui,
                                        &mut windows,
                                    );
                                    ui_state.paused = true;
                                    let _ = to_emu_tx
                                        .send(UiToEmu::Command(EmuCommand::SetPaused(true)));
                                    ui_state.spawn_vram = false;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Event::AboutToWait => {
                    while let Ok(evt) = from_emu_rx.try_recv() {
                        match evt {
                            EmuEvent::Frame {
                                frame: new_frame,
                                frame_index: _,
                            } => {
                                if new_frame.len() == frame.len() {
                                    frame.copy_from_slice(&new_frame);
                                }
                            }
                            EmuEvent::Serial { data, frame_index } => {
                                if !data.is_empty() {
                                    print!("[SERIAL {frame_index}] ");
                                    for b in &data {
                                        if b.is_ascii_graphic() || *b == b' ' {
                                            print!("{}", *b as char);
                                        } else {
                                            print!("\\x{b:02X}");
                                        }
                                    }
                                    println!();
                                }
                            }
                        }
                    }

                    if let Some(action) = ui_state.pending_action.take() {
                        let _ = to_emu_tx.send(UiToEmu::Action(action));
                        ui_state.paused = false;
                        ui_state.show_context = false;
                        let _ = to_emu_tx.send(UiToEmu::Command(EmuCommand::SetPaused(false)));
                    }

                    for win in windows.values() {
                        win.win.request_redraw();
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
                    print!("[SERIAL] ");
                    for b in &serial {
                        if b.is_ascii_graphic() || *b == b' ' {
                            print!("{}", *b as char);
                        } else {
                            print!("\\x{b:02X}");
                        }
                    }
                    println!();
                }

                println!("{}", gb.cpu.debug_state());
            }

            frame_count += 1;

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
    }
}
