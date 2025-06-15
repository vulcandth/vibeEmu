use image::io::Reader as ImageReader;
use vibeEmu::{cartridge::Cartridge, gameboy::GameBoy};

const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

#[test]
fn dmg_acid2_rom() {
    let mut gb = GameBoy::new();
    let rom = std::fs::read("roms/dmg-acid2/dmg-acid2.gb").expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    let mut frames = 0u32;
    while frames < 120 {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
        }
    }

    let expected = ImageReader::open("roms/dmg-acid2/dmg-acid2-dmg.png")
        .unwrap()
        .decode()
        .unwrap()
        .to_rgb8();
    assert_eq!(expected.width(), 160);
    assert_eq!(expected.height(), 144);

    let frame = gb.mmu.ppu.framebuffer();
    for (idx, pixel) in expected.pixels().enumerate() {
        let expected_color = match pixel.0 {
            [0x00, 0x00, 0x00] => DMG_PALETTE[3],
            [0x55, 0x55, 0x55] => DMG_PALETTE[2],
            [0xAA, 0xAA, 0xAA] => DMG_PALETTE[1],
            [0xFF, 0xFF, 0xFF] => DMG_PALETTE[0],
            _ => panic!("unexpected color {:?}", pixel),
        };
        assert_eq!(frame[idx], expected_color, "pixel mismatch at index {idx}");
    }
}
