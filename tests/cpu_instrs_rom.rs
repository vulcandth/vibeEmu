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

fn run_individual(rom_name: &str) {
    let path = std::path::Path::new("roms/blargg/cpu_instrs/individual").join(rom_name);
    let output = run_cpu_instrs(&path, 100_000_000);
    assert!(output.contains("Passed"), "{} failed: {}", rom_name, output);
}

#[test]
fn cpu_instrs_01_special() {
    run_individual("01-special.gb");
}

#[test]
fn cpu_instrs_02_interrupts() {
    run_individual("02-interrupts.gb");
}

#[test]
fn cpu_instrs_03_op_sp_hl() {
    run_individual("03-op sp,hl.gb");
}

#[test]
fn cpu_instrs_04_op_r_imm() {
    run_individual("04-op r,imm.gb");
}

#[test]
fn cpu_instrs_05_op_rp() {
    run_individual("05-op rp.gb");
}

#[test]
fn cpu_instrs_06_ld_r_r() {
    run_individual("06-ld r,r.gb");
}

#[test]
fn cpu_instrs_07_jr_jp_call_ret_rst() {
    run_individual("07-jr,jp,call,ret,rst.gb");
}

#[test]
fn cpu_instrs_08_misc_instrs() {
    run_individual("08-misc instrs.gb");
}

#[test]
fn cpu_instrs_09_op_r_r() {
    run_individual("09-op r,r.gb");
}

#[test]
fn cpu_instrs_10_bit_ops() {
    run_individual("10-bit ops.gb");
}

#[test]
fn cpu_instrs_11_op_a_hl() {
    run_individual("11-op a,(hl).gb");
}
