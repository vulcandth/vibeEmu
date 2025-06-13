use vibeEmu::{cartridge::Cartridge, gameboy::GameBoy};

fn run_instr_timing<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) -> String {
    let mut gb = GameBoy::new();
    let rom = std::fs::read(rom_path).expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    while gb.cpu.cycles < max_cycles {
        gb.cpu.step(&mut gb.mmu);
        let out = String::from_utf8_lossy(&gb.mmu.serial_out);
        if out.contains("Passed") || out.contains("Failed") {
            break;
        }
    }

    String::from_utf8(gb.mmu.take_serial()).unwrap()
}

#[test]
#[ignore]
fn instr_timing() {
    let output = run_instr_timing("roms/blargg/instr_timing/instr_timing.gb", 10_000_000);
    assert!(output.contains("Passed"), "instr_timing failed: {}", output);
}
