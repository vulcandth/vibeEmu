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
use log::info;
use minifb::{Key, Scale, Window, WindowOptions};
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
struct Args {
    /// Path to ROM file
    rom: Option<std::path::PathBuf>,

    /// Force DMG mode
    #[arg(long)]
    dmg: bool,

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

    let cgb_mode = if args.dmg { false } else { cart.cgb };
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

    let _stream = apu::Apu::start_stream(Arc::clone(&gb.mmu.apu));

    let mut frame = vec![0u32; 160 * 144];
    let mut frame_count = 0u64;

    if !args.headless {
        let mut window = Window::new(
            "vibeEmu",
            160,
            144,
            WindowOptions {
                scale: Scale::X2,
                ..WindowOptions::default()
            },
        )
        .expect("Failed to create window");
        window.limit_update_rate(Some(Duration::from_micros(16_700)));

        while window.is_open() && !window.is_key_down(Key::Escape) {
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

            while !gb.mmu.ppu.frame_ready() {
                gb.cpu.step(&mut gb.mmu);
            }

            frame.copy_from_slice(gb.mmu.ppu.framebuffer());
            gb.mmu.ppu.clear_frame_flag();

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
        const MAX_FRAMES: usize = 10;
        for _ in 0..MAX_FRAMES {
            while !gb.mmu.ppu.frame_ready() {
                gb.cpu.step(&mut gb.mmu);
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
        }
    }

    gb.mmu.save_cart_ram();
}
