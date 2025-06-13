use vibeEmu::{cartridge::Cartridge, gameboy::GameBoy};

fn run_cpu_instrs<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) -> String {
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
fn cpu_instrs_individual() {
    let roms_dir = std::path::Path::new("roms/blargg/cpu_instrs/individual");
    for entry in std::fs::read_dir(roms_dir).expect("read_dir failed") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gb") {
            continue;
        }

        let rom_name = path.file_name().unwrap().to_string_lossy().into_owned();
        let output = run_cpu_instrs(&path, 100_000_000);
        assert!(output.contains("Passed"), "{} failed: {}", rom_name, output);
    }
}
