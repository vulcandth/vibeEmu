#![allow(dead_code)]

mod apu;
mod cartridge;
mod cpu;
mod debugger;
mod egui_minifb;
mod gameboy;
mod input;
mod mmu;
mod ppu;
mod serial;
mod timer;

use crate::egui_minifb::EguiMiniFb;
use clap::Parser;
use debugger::{Debugger, RunState};
use log::info;
use minifb::{Key, Scale, Window, WindowOptions};
use std::sync::Arc;
use std::time::Duration;

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
        Some(apu::Apu::start_stream(Arc::clone(&gb.mmu.apu)))
    };

    let mut frame = vec![0u32; 160 * 144];
    let mut frame_count = 0u64;

    if !args.headless {
        let mut window = Window::new(
            "vibeEmu",
            160,
            144,
            WindowOptions {
                scale: Scale::X4,
                ..WindowOptions::default()
            },
        )
        .expect("Failed to create window");
        window.limit_update_rate(Some(Duration::from_micros(16_700)));
        let egui_ctx = egui::Context::default();
        let mut egui_mb = EguiMiniFb::new(160, 144);
        let mut dbg = Debugger::new();
        let mut runstate = RunState::Running;

        while window.is_open() {
            // -------------------------------- egui ---------------------------------
            let raw = egui_mb.raw_input(&window);
            let full = egui_ctx.run(raw, |ctx| {
                runstate = dbg.ui(&mut gb, ctx, runstate);
            });
            egui_mb.paint(&egui_ctx, full.shapes, &full.textures_delta);

            // Gather input
            let mut state = 0xFFu8;
            if window.is_key_down(Key::Right) {
                state &= !0x01;
            }
            if window.is_key_down(Key::Left) {
                state &= !0x02;
            }
            if window.is_key_down(Key::Up) {
                state &= !0x04;
            }
            if window.is_key_down(Key::Down) {
                state &= !0x08;
            }
            if window.is_key_down(Key::S) {
                state &= !0x10;
            }
            if window.is_key_down(Key::A) {
                state &= !0x20;
            }
            if window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift) {
                state &= !0x40;
            }
            if window.is_key_down(Key::Enter) {
                state &= !0x80;
            }
            gb.mmu.input.update_state(state, &mut gb.mmu.if_reg);

            // ========= 2. emulate =========
            let want_step = dbg.consume_step();
            if runstate == RunState::Running || want_step {
                loop {
                    gb.cpu.step(&mut gb.mmu);
                    if want_step || dbg.breakpoint_hit(gb.cpu.pc) {
                        runstate = RunState::Paused;
                        break;
                    }
                    if gb.mmu.ppu.frame_ready() {
                        break;
                    }
                }
            }

            // ----------- mix Game Boy framebuffer with debugger UI ------------
            if gb.mmu.ppu.frame_ready() {
                frame.copy_from_slice(gb.mmu.ppu.framebuffer());
                gb.mmu.ppu.clear_frame_flag();
            }
            let ui_fb = egui_mb.framebuffer();
            for (dst, &src) in frame.iter_mut().zip(ui_fb) {
                let sa = (src >> 24) & 0xFF;
                if sa == 0 {
                    continue;
                }
                let sr = (src >> 16) & 0xFF;
                let sg = (src >> 8) & 0xFF;
                let sb = src & 0xFF;

                let da = 0xFF - sa;
                let dr = (*dst >> 16) & 0xFF;
                let dg = (*dst >> 8) & 0xFF;
                let db = *dst & 0xFF;

                let r = (sr * sa + dr * da) >> 8;
                let g = (sg * sa + dg * da) >> 8;
                let b = (sb * sa + db * da) >> 8;

                *dst = (0xFF << 24) | (r << 16) | (g << 8) | b;
            }

            window
                .update_with_buffer(&frame, 160, 144)
                .expect("Failed to update window");

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
