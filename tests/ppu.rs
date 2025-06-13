use vibeEmu::ppu::Ppu;

#[test]
fn register_access() {
    let mut ppu = Ppu::new();
    ppu.write_reg(0xFF40, 0x91);
    ppu.write_reg(0xFF47, 0xFC);
    ppu.write_reg(0xFF4A, 0x01);
    ppu.write_reg(0xFF4B, 0x20);
    assert_eq!(ppu.read_reg(0xFF40), 0x91);
    assert_eq!(ppu.read_reg(0xFF47), 0xFC);
    assert_eq!(ppu.read_reg(0xFF4A), 0x01);
    assert_eq!(ppu.read_reg(0xFF4B), 0x20);

    // write palette data with auto-increment
    ppu.write_reg(0xFF68, 0x83); // index 3, auto-inc
    ppu.write_reg(0xFF69, 0xAA);
    ppu.write_reg(0xFF69, 0x55);
    assert_eq!(ppu.read_reg(0xFF68) & 0x3F, 5);
    // read back first written entry
    ppu.write_reg(0xFF68, 0x03);
    assert_eq!(ppu.read_reg(0xFF69), 0xAA);
}

#[test]
fn step_vblank_interrupt() {
    let mut ppu = Ppu::new();
    let mut if_reg = 0u8;
    for _ in 0..144 {
        ppu.step(456, &mut if_reg);
    }
    assert_eq!(ppu.read_reg(0xFF44), 144);
    assert_eq!(ppu.read_reg(0xFF41) & 0x03, 1); // mode 1
    assert!(if_reg & 0x01 != 0);
}
