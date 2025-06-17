use vibeEmu::mmu::Mmu;

#[test]
fn gdma_transfer() {
    let mut mmu = Mmu::new_with_mode(true);
    for i in 0..16u16 {
        mmu.write_byte(0xC000 + i, i as u8);
    }
    mmu.write_byte(0xFF51, 0xC0);
    mmu.write_byte(0xFF52, 0x00);
    mmu.write_byte(0xFF53, 0x80);
    mmu.write_byte(0xFF54, 0x00);
    mmu.write_byte(0xFF55, 0x00); // length 0 -> 1 block GDMA
    mmu.cgb_dma_step(32);
    for i in 0..16u16 {
        assert_eq!(mmu.ppu.vram[0][i as usize], i as u8);
    }
    assert_eq!(mmu.read_byte(0xFF55), 0xFF);
}

#[test]
fn hdma_stop_after_first_block() {
    let mut mmu = Mmu::new_with_mode(true);
    for i in 0..32u16 {
        mmu.write_byte(0xC000 + i, i as u8);
    }
    mmu.write_byte(0xFF51, 0xC0);
    mmu.write_byte(0xFF52, 0x00);
    mmu.write_byte(0xFF53, 0x80);
    mmu.write_byte(0xFF54, 0x00);
    mmu.write_byte(0xFF55, 0x81); // 2 blocks HDMA
    mmu.write_byte(0xFF55, 0x00); // stop after current block
    mmu.on_hblank();
    mmu.cgb_dma_step(32);
    for i in 0..16u16 {
        assert_eq!(mmu.ppu.vram[0][i as usize], i as u8);
    }
    assert_eq!(mmu.ppu.vram[0][16], 0);
    assert_eq!(mmu.read_byte(0xFF55), 0xFF);
}
