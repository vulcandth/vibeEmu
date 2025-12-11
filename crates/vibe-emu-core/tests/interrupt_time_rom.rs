mod common;
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

fn run_interrupt_time<P: AsRef<std::path::Path>>(
    rom_path: P,
    max_cycles: u64,
    cgb: bool,
) -> String {
    let rom = std::fs::read(rom_path).expect("rom not found");

    let mut gb = GameBoy::new_with_mode(cgb);
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
#[ignore] // TODO: ROM doesn't produce output yet - needs investigation
fn interrupt_time_cgb() {
    let output = run_interrupt_time(
        common::rom_path("blargg/interrupt_time/interrupt_time.gb"),
        20_000_000,
        true,
    );
    println!("interrupt_time CGB output:\n{}", output);
    assert!(output.contains("Passed"), "interrupt_time CGB failed");
}

#[test]
#[ignore] // TODO: ROM doesn't produce output yet - needs investigation
fn interrupt_time_dmg() {
    let output = run_interrupt_time(
        common::rom_path("blargg/interrupt_time/interrupt_time.gb"),
        20_000_000,
        false,
    );
    println!("interrupt_time DMG output:\n{}", output);
    assert!(output.contains("Passed"), "interrupt_time DMG failed");
}
