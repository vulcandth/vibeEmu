#![allow(dead_code)]

mod apu;
mod audio;
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
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalPosition;
use winit::{
    event::MouseButton,
    event::{ElementState, Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[allow(static_mut_refs)]
static mut NEXT_FRAME: Option<Instant> = None;

#[derive(Clone, Copy)]
struct Speed {
    factor: f32,
    fast: bool,
}

use ui::window::resize_pixels;
#[allow(static_mut_refs)]
fn emulate_until(gb: &mut gameboy::GameBoy, speed: Speed, control_flow: &mut ControlFlow) {
    let target = unsafe { NEXT_FRAME.get_or_insert_with(|| Instant::now() + FRAME_TIME) };

    if let Ok(mut apu) = gb.mmu.apu.lock() {
        apu.set_speed(speed.factor);
    }

    while !gb.mmu.ppu.frame_ready() {
        gb.cpu.step(&mut gb.mmu);
    }

    if !speed.fast {
        *control_flow = ControlFlow::WaitUntil(*target);
        while Instant::now() < *target {
            std::hint::spin_loop();
        }
        *target += FRAME_TIME;
    } else {
        *control_flow = ControlFlow::Poll;
        *target = Instant::now();
    }
}

use ui::display::{
    FF_MULT, FRAME_TIME, SCALE, UiState, build_ui, cursor_in_screen, draw_debugger,
    draw_game_screen, draw_vram, load_window_icon, spawn_debugger_window, spawn_vram_window,
};
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
        audio::start_stream(Arc::clone(&gb.mmu.apu))
    };

    let mut frame = vec![0u32; 160 * 144];
    let mut frame_count = 0u64;
    let mut ui_state = UiState::default();
    let mut speed = Speed {
        factor: 1.0,
        fast: false,
    };

    if !args.headless {
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title("vibeEmu")
            .with_window_icon(load_window_icon())
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
                                        let mask = if key == VirtualKeyCode::Space {
                                            speed.fast = pressed;
                                            speed.factor = if speed.fast { FF_MULT } else { 1.0 };
                                            if let Ok(mut apu) = gb.mmu.apu.lock() {
                                                apu.set_speed(speed.factor);
                                            }
                                            None
                                        } else {
                                            match key {
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
                                            }
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
                            }
                            WindowKind::Debugger => draw_debugger(&mut win.pixels, &mut gb, ui),
                            WindowKind::VramViewer => draw_vram(win, &mut gb, ui),
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
                    if ui_state.spawn_vram
                        && !windows
                            .values()
                            .any(|w| matches!(w.kind, WindowKind::VramViewer))
                    {
                        spawn_vram_window(target, &mut platform, &mut imgui, &mut windows);
                        ui_state.paused = true;
                        ui_state.spawn_vram = false;
                    }
                }
                Event::MainEventsCleared => {
                    if !ui_state.paused {
                        emulate_until(&mut gb, speed, control_flow);
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
