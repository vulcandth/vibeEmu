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
    assert_eq!(ppu.framebuffer[0], 0x008BAC0F);
    assert_eq!(ppu.framebuffer[7], 0x008BAC0F);
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
    assert_eq!(ppu.framebuffer[0], 0x008BAC0F);
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
    assert_eq!(ppu.framebuffer[0], 0x008BAC0F);
}

#[test]
fn sprite_8x16_tile_offset() {
    let mut ppu = Ppu::new();
    ppu.write_reg(0xFF40, 0x86); // LCD on, sprites 8x16
    ppu.write_reg(0xFF48, 0xE4);
    // top tile -> color 1
    ppu.vram[0][0] = 0xFF;
    ppu.vram[0][1] = 0x00;
    // bottom tile -> color 2
    ppu.vram[0][16] = 0x00;
    ppu.vram[0][17] = 0xFF;
    ppu.oam[0] = 16;
    ppu.oam[1] = 8;
    ppu.oam[2] = 1; // bit0 ignored
    ppu.oam[3] = 0;
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x008BAC0F);
    for _ in 0..8 {
        ppu.step(456, &mut if_reg);
    }
    assert_eq!(ppu.framebuffer[8 * 160], 0x00306230);
}

#[test]
fn sprite_x_priority() {
    let mut ppu = Ppu::new();
    ppu.write_reg(0xFF40, 0x82); // LCD on, sprites enabled
    ppu.write_reg(0xFF48, 0xE4);
    // tile 0 -> color 2
    ppu.vram[0][0] = 0x00;
    ppu.vram[0][1] = 0xFF;
    // tile 1 -> color 1
    ppu.vram[0][16] = 0xFF;
    ppu.vram[0][17] = 0x00;
    // sprite 0 at x=9 (behind)
    ppu.oam[0] = 16;
    ppu.oam[1] = 9;
    ppu.oam[2] = 0;
    ppu.oam[3] = 0;
    // sprite 1 at x=8 (front)
    ppu.oam[4] = 16;
    ppu.oam[5] = 8;
    ppu.oam[6] = 1;
    ppu.oam[7] = 0;
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[1], 0x008BAC0F);
}

#[test]
fn cgb_obj_priority_mode_cgb() {
    let mut ppu = Ppu::new_with_mode(true);
    ppu.write_reg(0xFF40, 0x82); // LCD on, sprites enabled
    ppu.write_reg(0xFF48, 0xE4);
    // two sprite tiles -> color1
    ppu.vram[0][0] = 0xFF;
    ppu.vram[0][1] = 0x00;
    ppu.vram[0][16] = 0xFF;
    ppu.vram[0][17] = 0x00;
    // sprite 0 at x=9 (should be drawn on top)
    ppu.oam[0] = 16;
    ppu.oam[1] = 9;
    ppu.oam[2] = 0;
    ppu.oam[3] = 0;
    // sprite 1 at x=8
    ppu.oam[4] = 16;
    ppu.oam[5] = 8;
    ppu.oam[6] = 1;
    ppu.oam[7] = 0;
    // sprite palette 0 color1 -> blue
    ppu.write_reg(0xFF6A, 0x80); // index 0 with auto inc
    ppu.write_reg(0xFF6B, 0x00);
    ppu.write_reg(0xFF6B, 0x00);
    ppu.write_reg(0xFF6B, 0x00);
    ppu.write_reg(0xFF6B, 0x7C);
    // CGB-style priority: prioritize by OAM order
    ppu.write_reg(0xFF6C, 0);
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    // sprite 0 should be visible at x=1
    assert_eq!(ppu.framebuffer[1], 0x000000FF);
}

#[test]
fn cgb_obj_priority_mode_dmg() {
    let mut ppu = Ppu::new_with_mode(true);
    ppu.write_reg(0xFF40, 0x82); // LCD on, sprites enabled
    ppu.write_reg(0xFF48, 0xE4);
    ppu.vram[0][0] = 0xFF;
    ppu.vram[0][1] = 0x00;
    ppu.vram[0][16] = 0xFF;
    ppu.vram[0][17] = 0x00;
    // sprite 0 at x=9
    ppu.oam[0] = 16;
    ppu.oam[1] = 9;
    ppu.oam[2] = 0;
    ppu.oam[3] = 0;
    // sprite 1 at x=8 (should be drawn on top when DMG priority)
    ppu.oam[4] = 16;
    ppu.oam[5] = 8;
    ppu.oam[6] = 1;
    ppu.oam[7] = 0;
    // sprite palette 0 color1 -> blue
    ppu.write_reg(0xFF6A, 0x80);
    ppu.write_reg(0xFF6B, 0x00);
    ppu.write_reg(0xFF6B, 0x00);
    ppu.write_reg(0xFF6B, 0x00);
    ppu.write_reg(0xFF6B, 0x7C);
    // DMG-style priority
    ppu.write_reg(0xFF6C, 1);
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    // sprite 1 should be visible at x=1
    assert_eq!(ppu.framebuffer[1], 0x000000FF);
}

#[test]
fn obj_priority_color0() {
    let mut ppu = Ppu::new();
    ppu.write_reg(0xFF40, 0x83); // LCD on, BG and OBJ
    ppu.write_reg(0xFF47, 0xE4);
    ppu.write_reg(0xFF48, 0xE4);
    // BG tile -> color 0
    ppu.vram[0][0] = 0x00;
    ppu.vram[0][1] = 0x00;
    ppu.vram[0][0x1800] = 0x00;
    // sprite tile -> color 1
    ppu.vram[0][16] = 0xFF;
    ppu.vram[0][17] = 0x00;
    ppu.oam[0] = 16;
    ppu.oam[1] = 8;
    ppu.oam[2] = 1;
    ppu.oam[3] = 0x80; // behind BG
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x008BAC0F);
}

#[test]
fn cgb_bg_attr_priority() {
    let mut ppu = Ppu::new_with_mode(true);
    ppu.write_reg(0xFF40, 0x93); // BG and OBJ
    // BG palette 0 color1 -> red
    ppu.write_reg(0xFF68, 0x80);
    ppu.write_reg(0xFF69, 0x00);
    ppu.write_reg(0xFF69, 0x00);
    ppu.write_reg(0xFF69, 0x1F);
    ppu.write_reg(0xFF69, 0x00);
    // sprite palette 0 color1 -> blue
    ppu.write_reg(0xFF6A, 0x80); // start at index 0 with auto inc
    ppu.write_reg(0xFF6B, 0x00); // color0 lo
    ppu.write_reg(0xFF6B, 0x00); // color0 hi
    ppu.write_reg(0xFF6B, 0x00); // color1 lo
    ppu.write_reg(0xFF6B, 0x7C); // color1 hi (blue)
    // BG tile
    ppu.vram[0][0] = 0xFF;
    ppu.vram[0][1] = 0x00;
    ppu.vram[0][0x1800] = 0x00;
    ppu.vram[1][0x1800] = 0x80; // priority
    // sprite tile
    ppu.vram[0][16] = 0xFF;
    ppu.vram[0][17] = 0x00;
    ppu.oam[0] = 16;
    ppu.oam[1] = 8;
    ppu.oam[2] = 1;
    ppu.oam[3] = 0;
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x00FF0000);
}

#[test]
fn cgb_master_priority() {
    let mut ppu = Ppu::new_with_mode(true);
    // LCD on, OBJ enabled, master priority cleared
    ppu.write_reg(0xFF40, 0x92);
    // BG palette 0 color1 -> red
    ppu.write_reg(0xFF68, 0x80);
    ppu.write_reg(0xFF69, 0x00);
    ppu.write_reg(0xFF69, 0x00);
    ppu.write_reg(0xFF69, 0x1F);
    ppu.write_reg(0xFF69, 0x00);
    // sprite palette 0 color1 -> blue
    ppu.write_reg(0xFF6A, 0x80); // start index 0 with autoinc
    ppu.write_reg(0xFF6B, 0x00); // color0 lo
    ppu.write_reg(0xFF6B, 0x00); // color0 hi
    ppu.write_reg(0xFF6B, 0x00); // color1 lo
    ppu.write_reg(0xFF6B, 0x7C); // color1 hi
    // BG tile with priority attribute
    ppu.vram[0][0] = 0xFF;
    ppu.vram[0][1] = 0x00;
    ppu.vram[0][0x1800] = 0x00;
    ppu.vram[1][0x1800] = 0x80; // priority bit set
    // sprite tile
    ppu.vram[0][16] = 0xFF;
    ppu.vram[0][17] = 0x00;
    ppu.oam[0] = 16;
    ppu.oam[1] = 8;
    ppu.oam[2] = 1;
    ppu.oam[3] = 0;
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    // sprite should appear on top despite BG priority
    assert_eq!(ppu.framebuffer[0], 0x000000FF);
}

#[test]
fn cgb_bg_palette() {
    let mut ppu = Ppu::new_with_mode(true);
    ppu.write_reg(0xFF40, 0x91);
    // palette 2 color 1 -> red
    ppu.write_reg(0xFF68, 0x80 | 0x10); // index 0x10 with auto inc
    ppu.write_reg(0xFF69, 0x00); // color 0
    ppu.write_reg(0xFF69, 0x00);
    ppu.write_reg(0xFF69, 0x1F); // color 1 lo
    ppu.write_reg(0xFF69, 0x00); // color 1 hi
    for i in 0..8 {
        ppu.vram[0][i * 2] = 0xFF;
        ppu.vram[0][i * 2 + 1] = 0x00;
    }
    ppu.vram[0][0x1800] = 0x00;
    ppu.vram[1][0x1800] = 0x02; // use palette 2
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x00FF0000);
}

#[test]
fn cgb_bg_bank_select() {
    let mut ppu = Ppu::new_with_mode(true);
    ppu.write_reg(0xFF40, 0x91);
    // palette 0 color 1 -> red
    ppu.write_reg(0xFF68, 0x80); // index 0 with auto inc
    ppu.write_reg(0xFF69, 0x00); // color 0 lo
    ppu.write_reg(0xFF69, 0x00); // color 0 hi
    ppu.write_reg(0xFF69, 0x1F); // color 1 lo
    ppu.write_reg(0xFF69, 0x00); // color 1 hi
    for i in 0..8 {
        ppu.vram[1][i * 2] = 0xFF;
        ppu.vram[1][i * 2 + 1] = 0x00;
    }
    ppu.vram[0][0x1800] = 0x00; // tile index
    ppu.vram[1][0x1800] = 0x08; // use bank 1
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x00FF0000);
}

#[test]
fn cgb_obj_palette_autoinc_read() {
    let mut ppu = Ppu::new_with_mode(true);
    // write two values with auto-increment
    ppu.write_reg(0xFF6A, 0x80); // index 0, auto inc
    ppu.write_reg(0xFF6B, 0x11);
    ppu.write_reg(0xFF6B, 0x22);

    // read back with auto-increment
    ppu.write_reg(0xFF6A, 0x80); // index 0, auto inc
    assert_eq!(ppu.read_reg(0xFF6B), 0x11);
    assert_eq!(ppu.read_reg(0xFF6A) & 0x3F, 1);
    assert_eq!(ppu.read_reg(0xFF6B), 0x22);
    assert_eq!(ppu.read_reg(0xFF6A) & 0x3F, 2);
}

#[test]
fn cgb_bg_palette_autoinc_read() {
    let mut ppu = Ppu::new_with_mode(true);
    ppu.write_reg(0xFF68, 0x80); // index 0, auto inc
    ppu.write_reg(0xFF69, 0x33);
    ppu.write_reg(0xFF69, 0x44);

    ppu.write_reg(0xFF68, 0x80); // index 0, auto inc
    assert_eq!(ppu.read_reg(0xFF69), 0x33);
    assert_eq!(ppu.read_reg(0xFF68) & 0x3F, 1);
    assert_eq!(ppu.read_reg(0xFF69), 0x44);
    assert_eq!(ppu.read_reg(0xFF68) & 0x3F, 2);
}

#[test]
fn bg_disable_yields_color0() {
    let mut ppu = Ppu::new();
    // LCD enabled, background/window disabled
    ppu.write_reg(0xFF40, 0x80);
    ppu.write_reg(0xFF47, 0xFC); // default palette
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x009BBC0F);
    assert_eq!(ppu.framebuffer[159], 0x009BBC0F);
}

#[test]
fn window_internal_line_counter() {
    let mut ppu = Ppu::new();
    // LCD on and window enabled
    ppu.write_reg(0xFF40, 0xB1);
    ppu.write_reg(0xFF47, 0xE4);
    ppu.write_reg(0xFF4A, 0); // WY=0
    ppu.write_reg(0xFF4B, 7); // WX so window at x=0

    // tile 0 -> color1, tile 1 -> color2
    for i in 0..8 {
        ppu.vram[0][i * 2] = 0xFF; // color1
        ppu.vram[0][16 + i * 2] = 0x00; // color2
        ppu.vram[0][16 + i * 2 + 1] = 0xFF;
    }
    for i in 0..8 {
        ppu.vram[0][i * 2 + 1] = 0x00;
    }
    ppu.vram[0][0x1800] = 0x00; // first line
    ppu.vram[0][0x1820] = 0x01; // second line

    let mut if_reg = 0u8;
    // first scanline -> uses tile 0
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x008BAC0F);
    let cnt1 = ppu.window_line_counter();
    println!("counter1 {}", cnt1);

    // hide window by moving off-screen
    ppu.write_reg(0xFF4B, 167);
    ppu.step(456, &mut if_reg);
    ppu.step(456, &mut if_reg);

    // bring window back
    ppu.write_reg(0xFF4B, 7);
    ppu.step(456, &mut if_reg);
    let cnt2 = ppu.window_line_counter();
    println!("counter2 {}", cnt2);
    assert_eq!(cnt1 + 1, cnt2);
}
