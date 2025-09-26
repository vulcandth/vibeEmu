mod common;
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

#[test]
fn cgb_acid2_rom() {
    let mut gb = GameBoy::new_with_mode(true);
    let rom = std::fs::read(common::rom_path("cgb-acid2/cgb-acid2.gbc")).expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    let mut frames = 0u32;
    while frames < 120 {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
        }
    }

    let (width, height, expected) =
        common::load_png_rgb(common::rom_path("cgb-acid2/cgb-acid2.png"));
    assert_eq!(width, 160);
    assert_eq!(height, 144);

    let frame = gb.mmu.ppu.framebuffer();
    for (idx, pixel) in expected.iter().enumerate() {
        let &[r, g, b] = pixel;
        let expected_color = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        assert_eq!(frame[idx], expected_color, "pixel mismatch at index {idx}");
    }
}
