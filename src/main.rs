#![allow(dead_code)]

mod apu;
mod cartridge;
mod cpu;
mod gameboy;
mod input;
mod mmu;
mod ppu;
mod serial;
mod timer;
mod ui;

use clap::Parser;
use imgui::{ConfigFlags, Context as ImguiContext};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use log::info;
use pixels::{Pixels, SurfaceTexture};
use rfd::FileDialog;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "debug-ui")]
use ui::vram_viewer::VramViewer;
use winit::dpi::PhysicalPosition;
use winit::{
    event::MouseButton,
    event::{ElementState, Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const SCALE: u32 = 3;

struct UiState {
    paused: bool,
    show_context: bool,
    ctx_pos: [f32; 2],
    spawn_debugger: bool,
    #[cfg(feature = "debug-ui")]
    vram_viewer: VramViewer,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            paused: false,
            show_context: false,
            ctx_pos: [0.0, 0.0],
            spawn_debugger: false,
            #[cfg(feature = "debug-ui")]
            vram_viewer: VramViewer::new(),
        }
    }
}

use ui::window::resize_pixels;
use ui::window::{UiWindow, WindowKind};

#[derive(Parser)]
struct Args {
    /// Path to ROM file
    rom: Option<std::path::PathBuf>,

    /// Force DMG mode
    #[arg(long, conflicts_with = "cgb")]
    dmg: bool,

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

fn spawn_debugger_window(
    event_loop: &winit::event_loop::EventLoopWindowTarget<()>,
    platform: &mut WinitPlatform,
    imgui: &mut ImguiContext,
    windows: &mut HashMap<winit::window::WindowId, UiWindow>,
) {
    use winit::dpi::LogicalSize;

    let w = winit::window::WindowBuilder::new()
        .with_title("vibeEmu \u{2013} Debugger")
        .with_inner_size(LogicalSize::new((160 * SCALE) as f64, (144 * SCALE) as f64))
        .build(event_loop)
        .unwrap();

    let size = w.inner_size();
    let surface = pixels::SurfaceTexture::new(size.width, size.height, &w);
    let pixels = pixels::Pixels::new(1, 1, surface).expect("Pixels error");

    platform.attach_window(imgui.io_mut(), &w, HiDpiMode::Rounded);

    let ui_win = UiWindow::new(WindowKind::Debugger, w, pixels, imgui);
    windows.insert(ui_win.win.id(), ui_win);
}

fn draw_debugger(pixels: &mut Pixels, gb: &mut gameboy::GameBoy, ui: &imgui::Ui) {
    let _ = pixels.frame_mut();
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
}

fn draw_game_screen(pixels: &mut Pixels, frame: &[u32]) {
    let pixel_frame: &mut [u32] = bytemuck::cast_slice_mut(pixels.frame_mut());
    for (dst, src) in pixel_frame.iter_mut().zip(frame) {
        let r = ((src >> 16) & 0xFF) as u8;
        let g = ((src >> 8) & 0xFF) as u8;
        let b = (src & 0xFF) as u8;
        *dst = u32::from_ne_bytes([r, g, b, 0xFF]);
    }
}

#[allow(clippy::too_many_arguments)]
fn build_ui(
    state: &mut UiState,
    ui: &imgui::Ui,
    gb: &mut gameboy::GameBoy,
    _event_loop: &winit::event_loop::EventLoopWindowTarget<()>,
    _platform: &mut WinitPlatform,
) {
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
                    {
                        if let Ok(cart) = cartridge::Cartridge::from_file(&path) {
                            gb.reset();
                            gb.mmu.load_cart(cart);
                            state.paused = false;
                        }
                    }
                    close_menu = true;
                }
                if ui.button("Reset GB") {
                    gb.reset();
                    state.paused = false;
                    close_menu = true;
                }
                if ui.button("Debugger") {
                    state.spawn_debugger = true;
                    close_menu = true;
                }
                #[cfg(feature = "debug-ui")]
                if ui.button("VRAM Viewer") {
                    state.vram_viewer.open = true;
                    close_menu = true;
                }
            });
        state.show_context = open && !close_menu;
    }
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    info!("Starting emulator");

    let rom_path = match args.rom {
        Some(p) => p,
        None => {
            eprintln!("No ROM supplied");
            return;
        }
    };

    let cart = match cartridge::Cartridge::from_file(&rom_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load ROM: {e}");
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
    let mut gb = gameboy::GameBoy::new_with_mode(cgb_mode);
    gb.mmu.load_cart(cart);

    if let Some(path) = args.bootrom {
        match std::fs::read(&path) {
            Ok(data) => gb.mmu.load_boot_rom(data),
            Err(e) => eprintln!("Failed to load boot ROM: {e}"),
        }
    }

    println!(
        "Emulator initialized in {} mode",
        if cgb_mode { "CGB" } else { "DMG" }
    );

    let _stream = if args.headless {
        None
    } else {
        apu::Apu::start_stream(Arc::clone(&gb.mmu.apu))
    };

    let mut frame = vec![0u32; 160 * 144];
    let mut frame_count = 0u64;
    let mut ui_state = UiState::default();

    if !args.headless {
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title("vibeEmu")
            .with_inner_size(winit::dpi::LogicalSize::new(
                (160 * SCALE) as f64,
                (144 * SCALE) as f64,
            ))
            .build(&event_loop)
            .expect("Failed to create window");

        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, &window);
        let pixels = Pixels::new(160, 144, surface).expect("Pixels error");

        let mut imgui = ImguiContext::create();
        imgui.io_mut().config_flags |= ConfigFlags::DOCKING_ENABLE;
        let mut platform = WinitPlatform::init(&mut imgui);
        platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Rounded);

        let mut windows = HashMap::new();
        let main_win = UiWindow::new(WindowKind::Main, window, pixels, &mut imgui);
        windows.insert(main_win.win.id(), main_win);

        let mut state = 0xFFu8;
        let mut cursor_pos = PhysicalPosition::new(0.0, 0.0);

        event_loop.run(move |event, target, control_flow| {
            *control_flow = ControlFlow::Poll;
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
                                    // Flush any pending cartridge RAM before quitting.
                                    gb.mmu.save_cart_ram();

                                    // Tell winit to end the loop and let `event_loop.run` return.
                                    *control_flow = ControlFlow::Exit;

                                    // Nothing else to process.
                                    #[allow(clippy::needless_return)]
                                    return;
                                } else {
                                    // Non-main editor/aux window – just close it.
                                    windows.remove(window_id);
                                }
                            }
                            WindowEvent::Resized(size) => {
                                resize_pixels(&mut win.pixels, *size);
                            }
                            WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                                resize_pixels(&mut win.pixels, **new_inner_size);
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
                                }
                            }
                            WindowEvent::MouseInput {
                                state: ElementState::Pressed,
                                button: MouseButton::Left,
                                ..
                            } if matches!(win.kind, WindowKind::Main) => {
                                // If the context menu is open and ImGui wants the mouse,
                                // let the menu handle the click without closing it.
                                if ui_state.show_context && imgui.io().want_capture_mouse {
                                    // Menu click: handled by ImGui.
                                } else if cursor_in_screen(&win.win, cursor_pos) {
                                    // Clicking the bare Game Boy screen closes the menu and resumes.
                                    ui_state.paused = false;
                                    ui_state.show_context = false;
                                }
                            }
                            WindowEvent::KeyboardInput { input, .. }
                                if matches!(win.kind, WindowKind::Main) =>
                            {
                                if !(ui_state.paused || imgui.io().want_text_input) {
                                    if let Some(key) = input.virtual_keycode {
                                        let pressed = input.state == ElementState::Pressed;
                                        let mask = match key {
                                            VirtualKeyCode::Right => Some(0x01),
                                            VirtualKeyCode::Left => Some(0x02),
                                            VirtualKeyCode::Up => Some(0x04),
                                            VirtualKeyCode::Down => Some(0x08),
                                            VirtualKeyCode::S => Some(0x10),
                                            VirtualKeyCode::A => Some(0x20),
                                            VirtualKeyCode::LShift | VirtualKeyCode::RShift => {
                                                Some(0x40)
                                            }
                                            VirtualKeyCode::Return => Some(0x80),
                                            VirtualKeyCode::Escape => {
                                                if pressed {
                                                    *control_flow = ControlFlow::Exit;
                                                }
                                                None
                                            }
                                            _ => None,
                                        };
                                        if let Some(mask) = mask {
                                            if pressed {
                                                state &= !mask;
                                            } else {
                                                state |= mask;
                                            }
                                            gb.mmu.input.update_state(state, &mut gb.mmu.if_reg);
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Event::RedrawRequested(window_id) => {
                    if let Some(win) = windows.get_mut(window_id) {
                        platform.prepare_frame(imgui.io_mut(), &win.win).unwrap();
                        let ui = imgui.frame();

                        match win.kind {
                            WindowKind::Main => {
                                build_ui(&mut ui_state, ui, &mut gb, target, &mut platform);
                                draw_game_screen(&mut win.pixels, &frame);
                                #[cfg(feature = "debug-ui")]
                                ui_state.vram_viewer.ui(
                                    ui,
                                    &gb.mmu.ppu,
                                    &gb.mmu,
                                    win.pixels.device(),
                                    win.pixels.queue(),
                                    &mut win.renderer,
                                );
                            }
                            WindowKind::Debugger => draw_debugger(&mut win.pixels, &mut gb, ui),
                        }

                        platform.prepare_render(ui, &win.win);
                        let draw_data = imgui.render();

                        let render_result =
                            win.pixels.render_with(|encoder, render_target, context| {
                                context.scaling_renderer.render(encoder, render_target);

                                if draw_data.total_vtx_count > 0 {
                                    let mut rpass =
                                        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
                                        });

                                    let surface_size = win.win.inner_size();
                                    rpass.set_scissor_rect(
                                        0,
                                        0,
                                        surface_size.width,
                                        surface_size.height,
                                    );

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
                            *control_flow = ControlFlow::Exit;
                        }
                    }

                    if ui_state.spawn_debugger
                        && !windows
                            .values()
                            .any(|w| matches!(w.kind, WindowKind::Debugger))
                    {
                        spawn_debugger_window(target, &mut platform, &mut imgui, &mut windows);
                        ui_state.paused = true;
                        ui_state.spawn_debugger = false;
                    }
                }
                Event::MainEventsCleared => {
                    if !ui_state.paused {
                        while !gb.mmu.ppu.frame_ready() {
                            gb.cpu.step(&mut gb.mmu);
                        }
                    }

                    if gb.mmu.ppu.frame_ready() {
                        frame.copy_from_slice(gb.mmu.ppu.framebuffer());
                        gb.mmu.ppu.clear_frame_flag();
                    }

                    for win in windows.values() {
                        win.win.request_redraw();
                    }

                    if args.debug && frame_count % 60 == 0 {
                        let serial = gb.mmu.take_serial();
                        if !serial.is_empty() {
                            print!("[SERIAL] ");
                            for b in &serial {
                                if b.is_ascii_graphic() || *b == b' ' {
                                    print!("{}", *b as char);
                                } else {
                                    print!("\\x{:02X}", b);
                                }
                            }
                            println!();
                        }

                        println!("{}", gb.cpu.debug_state());
                    }

                    frame_count += 1;
                }
                Event::LoopDestroyed => {
                    // Extra safety: if we ever reach here without having saved yet.
                    gb.mmu.save_cart_ram();
                }
                _ => {}
            }
        });
    } else {
        let frame_limit = args.frames;
        let cycle_limit = args.cycles;
        let second_limit = args.seconds.map(Duration::from_secs);

        let start = std::time::Instant::now();
        'headless: loop {
            while !gb.mmu.ppu.frame_ready() {
                gb.cpu.step(&mut gb.mmu);
                if let Some(max) = cycle_limit {
                    if gb.cpu.cycles >= max {
                        break 'headless;
                    }
                }
                if let Some(limit) = second_limit {
                    if start.elapsed() >= limit {
                        break 'headless;
                    }
                }
            }

            frame.copy_from_slice(gb.mmu.ppu.framebuffer());
            gb.mmu.ppu.clear_frame_flag();

            if args.debug && frame_count % 60 == 0 {
                let serial = gb.mmu.take_serial();
                if !serial.is_empty() {
                    print!("[SERIAL] ");
                    for b in &serial {
                        if b.is_ascii_graphic() || *b == b' ' {
                            print!("{}", *b as char);
                        } else {
                            print!("\\x{:02X}", b);
                        }
                    }
                    println!();
                }

                println!("{}", gb.cpu.debug_state());
            }

            frame_count += 1;

            if let Some(max) = frame_limit {
                if frame_count >= max as u64 {
                    break;
                }
            }
            if let Some(limit) = second_limit {
                if start.elapsed() >= limit {
                    break;
                }
            }
        }
    }
}
