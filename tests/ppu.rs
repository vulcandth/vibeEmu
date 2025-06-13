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
    ppu.write_reg(0xFF40, 0x80);
    let mut if_reg = 0u8;
    for _ in 0..144 {
        ppu.step(456, &mut if_reg);
    }
    assert_eq!(ppu.read_reg(0xFF44), 144);
    assert_eq!(ppu.read_reg(0xFF41) & 0x03, 1); // mode 1
    assert!(if_reg & 0x01 != 0);
}

#[test]
fn render_bg_scanline() {
    let mut ppu = Ppu::new();
    // enable LCD and BG with tile data at 0x8000
    ppu.write_reg(0xFF40, 0x91);
    // palette mapping: color 1 -> value 1
    ppu.write_reg(0xFF47, 0xE4);
    for i in 0..8 {
        ppu.vram[0][i * 2] = 0xFF;
        ppu.vram[0][i * 2 + 1] = 0x00;
    }
    ppu.vram[0][0x1800] = 0x00;
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 1);
    assert_eq!(ppu.framebuffer[7], 1);
}

#[test]
fn render_window_scanline() {
    let mut ppu = Ppu::new();
    ppu.write_reg(0xFF40, 0xB1); // LCD on, window enabled
    ppu.write_reg(0xFF47, 0xE4);
    ppu.write_reg(0xFF4A, 0); // WY
    ppu.write_reg(0xFF4B, 7); // WX so window starts at x=0
    for i in 0..8 {
        ppu.vram[0][16 + i * 2] = 0xFF;
        ppu.vram[0][16 + i * 2 + 1] = 0x00;
    }
    ppu.vram[0][0x1800] = 0x01;
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 1);
}

#[test]
fn render_sprite_scanline() {
    let mut ppu = Ppu::new();
    ppu.write_reg(0xFF40, 0x82); // LCD on, sprites enabled
    ppu.write_reg(0xFF48, 0xE4); // palette
    for i in 0..8 {
        ppu.vram[0][i * 2] = 0xFF;
        ppu.vram[0][i * 2 + 1] = 0x00;
    }
    ppu.oam[0] = 16; // y
    ppu.oam[1] = 8; // x
    ppu.oam[2] = 0; // tile
    ppu.oam[3] = 0; // flags
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 1);
}
