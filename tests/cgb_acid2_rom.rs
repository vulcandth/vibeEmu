use image::io::Reader as ImageReader;
use vibeEmu::{cartridge::Cartridge, gameboy::GameBoy};

#[test]
fn cgb_acid2_rom() {
    let mut gb = GameBoy::new_with_mode(true);
    let rom = std::fs::read("roms/cgb-acid2/cgb-acid2.gbc").expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    let mut frames = 0u32;
    while frames < 120 {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
        }
    }

    let expected = ImageReader::open("roms/cgb-acid2/cgb-acid2.png")
        .unwrap()
        .decode()
        .unwrap()
        .to_rgb8();
    assert_eq!(expected.width(), 160);
    assert_eq!(expected.height(), 144);

    let frame = gb.mmu.ppu.framebuffer();
    for (idx, pixel) in expected.pixels().enumerate() {
        let expected_color = ((pixel[0] as u32) << 16) | ((pixel[1] as u32) << 8) | pixel[2] as u32;
        assert_eq!(frame[idx], expected_color, "pixel mismatch at index {idx}");
    }
}
