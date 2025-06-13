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
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    info!("Starting emulator");

    let mut gb = gameboy::GameBoy::new();

    if let Some(path) = args.rom {
        match cartridge::Cartridge::from_file(&path) {
            Ok(cart) => {
                gb.mmu.load_cart(cart);
            }
            Err(e) => {
                eprintln!("Failed to load ROM: {e}");
                return;
            }
        }
    } else {
        eprintln!("No ROM supplied");
        return;
    }

    // TODO: main emulation loop will go here
    println!(
        "Emulator initialized in {} mode",
        if args.dmg { "DMG" } else { "CGB" }
    );
}
