use vibeEmu::{cartridge::Cartridge, gameboy::GameBoy};

fn run_cpu_instrs(max_cycles: u64) -> String {
    let mut gb = GameBoy::new();
    let rom = std::fs::read("roms/blargg/cpu_instrs/cpu_instrs.gb").expect("rom not found");
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
fn cpu_instrs_rom() {
    let output = run_cpu_instrs(50_000_000); // around 12s of emu time
    assert!(output.contains("Passed"), "Test output: {}", output);
}
