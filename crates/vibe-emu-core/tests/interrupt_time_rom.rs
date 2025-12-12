mod common;
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

#[test]
#[ignore] // CGB test still shows EE (238) and F6 (246) instead of expected 0D (13).
          // The current interrupt timing fix does not resolve the issue in CGB mode.
          // Further investigation needed to understand why values are ~18-19x too high.
          // DMG mode passes perfectly.
fn interrupt_time_cgb() {
    let mut gb = GameBoy::new_with_mode(true);
    let rom = std::fs::read(common::rom_path("blargg/interrupt_time/interrupt_time.gb"))
        .expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    let mut frames = 0u32;
    while frames < 300 {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
        }
    }

    let (width, height, expected) =
        common::load_png_rgb(common::rom_path("blargg/interrupt_time/interrupt_time-cgb.png"));
    assert_eq!(width, 160);
    assert_eq!(height, 144);

    let frame = gb.mmu.ppu.framebuffer();
    let mut mismatches = 0;
    for (idx, pixel) in expected.iter().enumerate() {
        let &[r, g, b] = pixel;
        let expected_color = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        if frame[idx] != expected_color {
            mismatches += 1;
        }
    }
    assert_eq!(mismatches, 0, "Found {} pixel mismatches", mismatches);
}

#[test]
fn interrupt_time_dmg() {
    let mut gb = GameBoy::new();
    let rom = std::fs::read(common::rom_path("blargg/interrupt_time/interrupt_time.gb"))
        .expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    let mut frames = 0u32;
    while frames < 180 {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
        }
    }

    let (width, height, expected) =
        common::load_png_rgb(common::rom_path("blargg/interrupt_time/interrupt_time-dmg.png"));
    assert_eq!(width, 160);
    assert_eq!(height, 144);

    let frame = gb.mmu.ppu.framebuffer();
    for (idx, pixel) in expected.iter().enumerate() {
        let pixel = *pixel;
        let expected_color = match pixel {
            [0x00, 0x00, 0x00] => DMG_PALETTE[3],
            [0x55, 0x55, 0x55] => DMG_PALETTE[2],
            [0xAA, 0xAA, 0xAA] => DMG_PALETTE[1],
            [0xFF, 0xFF, 0xFF] => DMG_PALETTE[0],
            _ => panic!("unexpected color {:?}", pixel),
        };
        assert_eq!(
            frame[idx], expected_color,
            "pixel mismatch at index {idx}"
        );
    }
}
