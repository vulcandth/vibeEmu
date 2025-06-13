#![allow(dead_code)]

mod apu;
mod cartridge;
mod cpu;
mod gameboy;
mod input;
mod mmu;
mod ppu;
mod timer;

use clap::Parser;
use log::info;

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

    // TODO: main emulation loop will go here
    println!(
        "Emulator initialized in {} mode",
        if cgb_mode { "CGB" } else { "DMG" }
    );

    gb.mmu.save_cart_ram();
}
