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

use clap::Parser;
use imgui::{ConfigFlags, Context as ImguiContext};
use imgui_wgpu::{Renderer, RendererConfig};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use log::info;
use pixels::{Pixels, SurfaceTexture};
use rfd::FileDialog;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::dpi::PhysicalPosition;
use winit::{
    event::MouseButton,
    event::{ElementState, Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const SCALE: u32 = 3;
const FRAME_CYCLES: u64 = 70224;
const FRAME_TIME: Duration = Duration::from_nanos(16_744_000);
static LAST_FRAME: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Default)]
struct UiState {
    paused: bool,
    show_context: bool,
    ctx_pos: [f32; 2],
    show_debugger: bool,
    show_vram: bool,
}

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

fn build_ui(state: &mut UiState, ui: &imgui::Ui, gb: &mut gameboy::GameBoy) {
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
                    state.show_debugger = true;
                    close_menu = true;
                }
                if ui.button("VRAM Viewer") {
                    state.show_vram = true;
                    close_menu = true;
                }
            });
        state.show_context = open && !close_menu;
    }

    if state.show_debugger {
        ui.window("Debugger")
            .opened(&mut state.show_debugger)
            .build(|| {
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
            });
    }

    if state.show_vram {
        ui.window("VRAM Viewer")
            .size([640.0, 480.0], imgui::Condition::FirstUseEver)
            .opened(&mut state.show_vram)
            .build(|| {
                if let Some(_tabbar) = imgui::TabBar::new("vram_tabs").begin(ui) {
                    if let Some(_tab) = imgui::TabItem::new("BG Map").begin(ui) {
                        // draw BG map texture
                    }
                    if let Some(_tab) = imgui::TabItem::new("Tiles").begin(ui) {
                        // 8×8 tile atlas
                    }
                    if let Some(_tab) = imgui::TabItem::new("OAM").begin(ui) {
                        // sprite inspector
                    }
                    if let Some(_tab) = imgui::TabItem::new("Palettes").begin(ui) {
                        // CGB palette table
                    }
                }
            });
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
        let mut pixels = Pixels::new(160, 144, surface).expect("Pixels error");

        let mut imgui = ImguiContext::create();
        imgui.io_mut().config_flags |= ConfigFlags::DOCKING_ENABLE | ConfigFlags::VIEWPORTS_ENABLE;
        let mut platform = WinitPlatform::init(&mut imgui);
        platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Rounded);
        let renderer_config = RendererConfig {
            texture_format: pixels.render_texture_format(),
            ..Default::default()
        };
        let mut renderer =
            Renderer::new(&mut imgui, pixels.device(), pixels.queue(), renderer_config);
        let mut state = 0xFFu8;
        let mut cursor_pos = PhysicalPosition::new(0.0, 0.0);

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            platform.handle_event(imgui.io_mut(), &window, &event);
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::Resized(size) => {
                        let _ = pixels.resize_surface(size.width, size.height);
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        cursor_pos = position;
                    }
                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Right,
                        ..
                    } => {
                        if !ui_state.paused && cursor_in_screen(&window, cursor_pos) {
                            ui_state.paused = true;
                            ui_state.show_context = true;
                            ui_state.ctx_pos = [cursor_pos.x as f32, cursor_pos.y as f32];
                        }
                    }
                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                        ..
                    } => {
                        if ui_state.paused && !imgui.io().want_capture_mouse {
                            ui_state.paused = false;
                            ui_state.show_context = false;
                        }
                    }
                    WindowEvent::KeyboardInput { input, .. } => {
                        // Allow arrows unless we're paused or actively typing in ImGui
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
                                    VirtualKeyCode::LShift | VirtualKeyCode::RShift => Some(0x40),
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
                },
                Event::MainEventsCleared => {
                    {
                        let mut last = LAST_FRAME.lock().unwrap();
                        if let Some(prev) = *last {
                            let now = Instant::now();
                            let since = now - prev;
                            if since < FRAME_TIME {
                                std::thread::sleep(FRAME_TIME - since);
                            }
                        }
                        *last = Some(Instant::now());
                    }

                    if !ui_state.paused {
                        while !gb.mmu.ppu.frame_ready() {
                            gb.cpu.step(&mut gb.mmu);
                        }
                    }

                    if gb.mmu.ppu.frame_ready() {
                        frame.copy_from_slice(gb.mmu.ppu.framebuffer());
                        gb.mmu.ppu.clear_frame_flag();
                    }

                    // UI built during RedrawRequested
                    window.request_redraw();

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
                Event::RedrawRequested(_) => {
                    platform
                        .prepare_frame(imgui.io_mut(), &window)
                        .expect("prepare frame");
                    let ui = imgui.frame();
                    build_ui(&mut ui_state, ui, &mut gb);
                    platform.prepare_render(ui, &window);

                    let pixel_frame: &mut [u32] = bytemuck::cast_slice_mut(pixels.frame_mut());
                    for (dst, src) in pixel_frame.iter_mut().zip(&frame) {
                        let r = ((src >> 16) & 0xFF) as u8;
                        let g = ((src >> 8) & 0xFF) as u8;
                        let b = (src & 0xFF) as u8;
                        *dst = u32::from_ne_bytes([r, g, b, 0xFF]);
                    }
                    let draw_data = imgui.render();
                    let render_result = pixels.render_with(|encoder, render_target, context| {
                        context.scaling_renderer.render(encoder, render_target);

                        if draw_data.total_vtx_count > 0 {
                            let mut rpass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: Some("imgui_pass"),
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: render_target,
                                        resolve_target: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Load,
                                            store: true,
                                        },
                                    })],
                                    depth_stencil_attachment: None,
                                });
                            renderer
                                .render(draw_data, pixels.queue(), pixels.device(), &mut rpass)
                                .expect("imgui render failed");
                        }
                        Ok(())
                    });
                    if render_result.is_err() {
                        *control_flow = ControlFlow::Exit;
                    }
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

    gb.mmu.save_cart_ram();
}
