mod common;

use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

fn run_rom(name: &str, model: Model) -> GameBoy {
    let rom = std::fs::read(common::rom_path(format!("little-things-gb/{name}")))
        .expect("failed to read little-things ROM");
    let mut gb = GameBoy::new(model);
    gb.mmu.load_cart(Cartridge::from_bytes(rom));
    // Match the four grayscale levels in the author's digital SGB capture.
    gb.mmu
        .ppu
        .set_dmg_palette([0xFFFFFF, 0xBFBFBF, 0x7F7F7F, 0x3F3F3F]);
    // Bound execution even if the ROM locks up or disables the LCD.
    while gb.cpu.cycles < 70_224 * 60 {
        gb.cpu.step(&mut gb.mmu);
        assert!(
            !gb.cpu.stopped && !gb.cpu.faulted,
            "{name} stopped or faulted at PC={:04X}",
            gb.cpu.pc
        );
    }
    gb
}

fn double_halt_cancel(name: &str, model: Model, fractional_div: u8) {
    let gb = run_rom(name, model);
    // Upstream HANDLE_RESULT prints ASCII tiles at $98C8 only after checking
    // RST $38, return address +1, LY=1, and the exact DIV phase. Checking the
    // ROM's own PASS marker also catches lockups and incomplete execution.
    assert_eq!(
        &gb.mmu.ppu.vram[0][0x18c8..0x18cd],
        b"PASS!",
        "captured HRAM: {:02X?}",
        &gb.mmu.hram[..7]
    );
    // HRAM layout from the pinned v1.0 source: ret_type, LY, DIV, DIV fraction.
    assert_eq!(&gb.mmu.hram[3..7], &[0, 1, 2, fractional_div]);
}

#[test]
fn double_halt_cancel_dmg() {
    double_halt_cancel("double-halt-cancel.gb", Model::default(), 0x15);
}

#[test]
fn double_halt_cancel_cgb() {
    double_halt_cancel(
        "double-halt-cancel-gbconly.gb",
        Model::Cgb(Default::default()),
        0x16,
    );
}

#[test]
fn double_halt_cancel_cgb_dmg_compat() {
    double_halt_cancel(
        "double-halt-cancel.gb",
        Model::Cgb(Default::default()),
        0x16,
    );
}

#[test]
fn windesync_validate_dmg() {
    let gb = run_rom("windesync-validate.gb", Model::default());
    // Upstream's digital capture from Super Game Boy hardware, not emulator output.
    let (width, height, expected) = common::load_png_rgb(common::rom_path(
        "little-things-gb/windesync-reference-sgb.png",
    ));
    assert_eq!((width, height), (160, 144));
    let mismatches: Vec<_> = gb
        .mmu
        .ppu
        .framebuffer()
        .iter()
        .zip(expected.iter())
        .enumerate()
        .filter_map(|(i, (&actual, &[r, g, b]))| {
            let expected = (r as u32) << 16 | (g as u32) << 8 | b as u32;
            (actual != expected).then_some((i % 160, i / 160, actual, expected))
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} mismatched pixels; first: {:?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(20)]
    );
}
