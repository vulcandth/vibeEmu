mod common;
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

fn run_mem_timing<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) -> String {
    let mut gb = GameBoy::new();
    let rom = std::fs::read(rom_path).expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    let mut checked_up_to = 0;
    while gb.cpu.cycles < max_cycles {
        gb.cpu.step(&mut gb.mmu);
        if common::serial_contains_result(gb.mmu.serial.peek_output(), &mut checked_up_to) {
            break;
        }
    }

    String::from_utf8(gb.mmu.take_serial()).unwrap()
}

fn run_individual(rom_name: &str) {
    let path = common::roms_dir()
        .join("blargg/mem_timing/individual")
        .join(rom_name);
    let output = run_mem_timing(&path, 10_000_000);
    assert!(output.contains("Passed"), "{} failed: {}", rom_name, output);
}

#[test]
fn mem_timing_read() {
    run_individual("01-read_timing.gb");
}

#[test]
fn mem_timing_write() {
    run_individual("02-write_timing.gb");
}

#[test]
fn mem_timing_modify() {
    run_individual("03-modify_timing.gb");
}
