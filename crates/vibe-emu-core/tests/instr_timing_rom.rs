mod common;
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

fn run_instr_timing<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) -> String {
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

#[test]
fn instr_timing() {
    let output = run_instr_timing(
        common::rom_path("blargg/instr_timing/instr_timing.gb"),
        10_000_000,
    );
    assert!(output.contains("Passed"), "instr_timing failed: {}", output);
}
