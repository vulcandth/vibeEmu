use vibe_emu_core::{
    hardware::{CgbRevision, Model},
    ppu::Ppu,
};

#[test]
fn lcd_startup_blanks_only_the_first_frame_without_stopping_the_ppu() {
    for model in [Model::default(), Model::Cgb(CgbRevision::default())] {
        let mut ppu = Ppu::new(model);
        assert!(ppu.framebuffer().iter().all(|&pixel| pixel == 0xFFFFFF));
        ppu.set_dmg_palette([0xFFFFFF, 0, 0, 0]);
        ppu.write_reg(0xFF47, 0xE4);
        ppu.write_reg(0xFF4B, 7);
        for row in 0..8 {
            ppu.vram[0][row * 2] = 0xFF;
        }
        ppu.write_reg(0xFF40, 0xB1);
        let mut interrupts = 0;
        for first_frame in [true, false] {
            ppu.clear_frame_flag();
            let mut dots = 0;
            while !ppu.frame_ready() {
                assert!(dots < 70_224 * 2);
                ppu.step(4, &mut interrupts);
                dots += 4;
            }
            assert_eq!(ppu.ly(), 144);
            assert_ne!(interrupts & 1, 0, "VBlank must still fire");
            assert!(ppu.window_line_counter() > 0, "window must still advance");
            let expected = if first_frame { 0xFFFFFF } else { 0 };
            assert!(
                ppu.framebuffer().iter().all(|&pixel| pixel == expected),
                "{model:?}, first_frame={first_frame}"
            );
        }
        ppu.write_reg(0xFF40, 0);
        assert!(ppu.framebuffer().iter().all(|&pixel| pixel == 0xFFFFFF));
        ppu.write_reg(0xFF40, 0xB1);
        ppu.clear_frame_flag();
        for _ in 0..144 {
            ppu.step(456, &mut interrupts);
        }
        assert!(ppu.framebuffer().iter().all(|&pixel| pixel == 0xFFFFFF));
    }
}

#[test]
fn cgb_lcd_enable_draws_line_zero_before_advancing_ly() {
    for compat in [false, true] {
        let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
        ppu.set_dmg_compat_mode(compat);
        ppu.write_reg(0xFF40, 0);
        ppu.write_reg(0xFF40, 0x91);
        let mut interrupts = 0;
        ppu.step(79, &mut interrupts);
        assert_eq!(ppu.ly(), 0);
        assert_eq!(ppu.read_reg(0xFF41) & 3, 0);
        assert!(ppu.vram_read_accessible());
        assert!(ppu.oam_read_accessible());
        ppu.step(1, &mut interrupts);
        assert_eq!(ppu.ly(), 0);
        assert_eq!(ppu.mode(), 3);
        assert!(!ppu.vram_read_accessible());
        ppu.step(376, &mut interrupts);
        assert_eq!(ppu.ly(), 1);
        assert_eq!(ppu.mode(), 2);
        ppu.step(80, &mut interrupts);
        assert_eq!(ppu.ly(), 1);
        assert!(!ppu.vram_read_accessible());

        // Restarting the LCD must repeat the initial drawing period.
        ppu.write_reg(0xFF40, 0);
        ppu.write_reg(0xFF40, 0x91);
        ppu.step(80, &mut interrupts);
        assert_eq!((ppu.ly(), ppu.mode()), (0, 3));
    }
}

#[test]
fn disabled_window_pixel_depends_on_scroll_alignment_and_prior_window_start() {
    for model in [Model::default(), Model::Cgb(CgbRevision::default())] {
        for triggered in [false, true] {
            for scx in 0..8u8 {
                for wx in 0..=168u8 {
                    let mut ppu = Ppu::new(model);
                    // Color ID 0 must use the palette, rather than hardcoded black.
                    ppu.set_dmg_palette([3, 2, 1, 0]);
                    if model.is_cgb() {
                        ppu.apply_dmg_compatibility_palettes();
                    }
                    ppu.write_reg(0xFF47, 0xE4);
                    ppu.write_reg(0xFF4A, 0);
                    ppu.write_reg(0xFF4B, 7);
                    ppu.write_reg(0xFF40, if triggered { 0xB1 } else { 0x91 });
                    ppu.skip_startup_for_test();
                    // A repeating 0,1,2,3 pattern exposes both inserted pixels
                    // and a one-pixel shift, including insertions left of X=0.
                    for row in 0..8 {
                        ppu.vram[0][row * 2] = 0x55;
                        ppu.vram[0][row * 2 + 1] = 0x33;
                    }
                    let mut interrupts = 0;
                    ppu.step(456, &mut interrupts);
                    ppu.write_reg(0xFF40, 0x91);
                    ppu.write_reg(0xFF43, scx);
                    ppu.write_reg(0xFF4B, wx);
                    // The effect persists beyond the immediately following line.
                    ppu.step(456 * 4, &mut interrupts);
                    for x in 0..160usize {
                        let glitch =
                            model.is_dmg() && triggered && wx <= 166 && (wx & 7) == 7 - scx;
                        let origin = i16::from(wx) - 7;
                        let color_id = if glitch && x as i16 == origin {
                            0
                        } else {
                            (x + scx as usize - usize::from(glitch && x as i16 > origin)) & 3
                        };
                        let expected = if model.is_cgb() {
                            ppu.bg_palette_color(0, color_id)
                        } else {
                            3 - color_id as u32
                        };
                        assert_eq!(
                            ppu.framebuffer()[4 * 160 + x],
                            expected,
                            "{model:?}, triggered={triggered}, SCX={scx}, WX={wx}, x={x}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn disabled_window_glitch_resets_at_new_frame_and_lcd_disable() {
    for reset_lcd in [false, true] {
        let mut ppu = Ppu::new(Model::default());
        ppu.set_dmg_palette([0, 1, 2, 3]);
        ppu.write_reg(0xFF47, 0xE4);
        ppu.write_reg(0xFF4B, 7);
        ppu.write_reg(0xFF40, 0xB1);
        ppu.skip_startup_for_test();
        for row in 0..8 {
            ppu.vram[0][row * 2] = 0xFF;
        }
        let mut interrupts = 0;
        ppu.step(456, &mut interrupts);
        ppu.write_reg(0xFF40, 0x91);
        ppu.step(456, &mut interrupts);
        assert_eq!(&ppu.framebuffer()[160..162], &[0, 1]);

        if reset_lcd {
            ppu.write_reg(0xFF40, 0);
            ppu.write_reg(0xFF40, 0x91);
            ppu.skip_startup_for_test();
        } else {
            for _ in 2..154 {
                ppu.step(456, &mut interrupts);
            }
        }
        ppu.step(456, &mut interrupts);
        assert_eq!(&ppu.framebuffer()[..2], &[1, 1]);
    }
}

#[test]
fn sprite_tiles_do_not_depend_on_background_scroll() {
    for model in [Model::default(), Model::Cgb(CgbRevision::default())] {
        for scx in 0..8 {
            for flags in [0, 0x20, 0x40, 0x60] {
                let mut ppu = Ppu::new(model);
                if model.is_cgb() {
                    ppu.apply_dmg_compatibility_palettes();
                }
                ppu.write_reg(0xFF40, 0x93); // 8x8 sprites, BG enabled
                ppu.write_reg(0xFF47, 0);
                ppu.write_reg(0xFF48, 0xE4);
                ppu.write_reg(0xFF43, scx);
                // Tile 0 is transparent; tile 1 is solid color 1. An 8x8 OBJ
                // must retain the odd tile index at every scroll position.
                for row in 0..8 {
                    ppu.vram[0][16 + row * 2] = 0xFF;
                }
                for (i, x) in [48, 80, 112].into_iter().enumerate() {
                    ppu.oam[i * 4..i * 4 + 4].copy_from_slice(&[16, x, 1, flags]);
                }
                ppu.skip_startup_for_test();
                let mut interrupts = 0;
                for y in 0..8 {
                    ppu.step(456, &mut interrupts);
                    let color = if model.is_cgb() {
                        ppu.ob_palette_color(0, 1)
                    } else {
                        0x008BAC0F
                    };
                    for x in [40, 72, 104] {
                        assert!(
                            ppu.framebuffer[y * 160 + x..y * 160 + x + 8]
                                .iter()
                                .all(|&pixel| pixel == color),
                            "{model:?}, SCX={scx}, flags={flags:#04x}, x={x}, y={y}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn visible_sprites_are_not_dropped_by_fetch_timing() {
    for model in [Model::default(), Model::Cgb(CgbRevision::default())] {
        for scx in 0..8 {
            for count in [1, 2, 7, 10] {
                for first_x in 1..168u8 {
                    let mut ppu = Ppu::new(model);
                    if model.is_cgb() {
                        ppu.apply_dmg_compatibility_palettes();
                    }
                    ppu.write_reg(0xFF40, 0x87); // 8x16 OBJs, BG color 0
                    ppu.write_reg(0xFF47, 0);
                    ppu.write_reg(0xFF48, 0xE4);
                    ppu.write_reg(0xFF43, scx);
                    for row in 0..16 {
                        ppu.vram[0][32 + row * 2] = 0xFF;
                    }
                    for i in 0..count {
                        let x = first_x + i as u8 * 8;
                        ppu.oam[i * 4..i * 4 + 4].copy_from_slice(&[16, x, 2, 0]);
                    }
                    ppu.skip_startup_for_test();
                    let mut interrupts = 0;
                    ppu.step(456, &mut interrupts);
                    let color = if model.is_cgb() {
                        ppu.ob_palette_color(0, 1)
                    } else {
                        0x008BAC0F
                    };
                    for x in (first_x as usize).saturating_sub(8)
                        ..(first_x as usize + count * 8 - 8).min(160)
                    {
                        assert_eq!(
                            ppu.framebuffer[x], color,
                            "{model:?}, SCX={scx}, count={count}, OAM X={first_x}, pixel={x}"
                        );
                    }
                }
            }
        }
    }
}

// Rewriting SCX without changing its value must leave both the background
// coordinates and sprite output alone, including while OBJ fetches stall the
// pixel pipeline. Games can repeat the same scroll write on every scanline.
#[test]
fn unchanged_scx_writes_with_sprites_preserve_pixels() {
    for model in [Model::default(), Model::Cgb(CgbRevision::default())] {
        for fine_scroll in 0..8 {
            let render = |repeat_write: bool| {
                let mut ppu = Ppu::new(model);
                if model.is_cgb() {
                    ppu.apply_dmg_compatibility_palettes();
                }
                ppu.write_reg(0xFF40, 0x97); // BG and 8x16 OBJs, unsigned tiles
                ppu.write_reg(0xFF47, 0xE4);
                ppu.write_reg(0xFF48, 0xE4);
                ppu.write_reg(0xFF43, 160 + fine_scroll);
                for tile in 1..=32 {
                    for row in 0..8 {
                        ppu.vram[0][tile * 16 + row * 2] = (tile as u8).rotate_left(row as u32);
                        ppu.vram[0][tile * 16 + row * 2 + 1] = !(tile as u8);
                    }
                }
                for col in 0..32 {
                    ppu.vram[0][0x1800 + col] = col as u8 + 1;
                }
                for (i, x) in [78, 86, 94, 102, 126, 134, 142].into_iter().enumerate() {
                    ppu.oam[i * 4..i * 4 + 4].copy_from_slice(&[16, x, 2, 0]);
                }
                ppu.skip_startup_for_test();
                let mut interrupts = 0;
                ppu.step(456 + 80, &mut interrupts); // second line, start of mode 3
                assert_eq!(ppu.mode(), 3);
                for dot in 1..=300 {
                    ppu.step(1, &mut interrupts);
                    if repeat_write && [12, 40, 80, 120, 160].contains(&dot) {
                        ppu.write_reg(0xFF43, 160 + fine_scroll);
                    }
                }
                ppu.framebuffer[160..320].to_vec()
            };
            assert_eq!(
                render(true),
                render(false),
                "unchanged SCX, {model:?}, fine scroll {fine_scroll}"
            );
        }
    }
}

#[test]
fn register_access() {
    let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
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
    let mut ppu = Ppu::new(Model::default());
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
fn render_sprite_scanline() {
    let mut ppu = Ppu::new(Model::default());
    ppu.write_reg(0xFF40, 0x82); // LCD on, sprites enabled
    ppu.skip_startup_for_test();
    let mut if_reg = 0u8;
    ppu.write_reg(0xFF48, 0xE4); // palette
    for i in 0..8 {
        ppu.vram[0][i * 2] = 0xFF;
        ppu.vram[0][i * 2 + 1] = 0x00;
    }
    ppu.oam[0] = 16; // y
    ppu.oam[1] = 8; // x
    ppu.oam[2] = 0; // tile
    ppu.oam[3] = 0; // flags
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x008BAC0F);
}

#[test]
fn sprite_8x16_tile_offset() {
    let mut ppu = Ppu::new(Model::default());
    ppu.write_reg(0xFF40, 0x86); // LCD on, sprites 8x16
    ppu.skip_startup_for_test();
    let mut if_reg = 0u8;
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
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x008BAC0F);
    for _ in 0..8 {
        ppu.step(456, &mut if_reg);
    }
    assert_eq!(ppu.framebuffer[8 * 160], 0x00306230);
}

#[test]
fn sprite_x_priority() {
    let mut ppu = Ppu::new(Model::default());
    ppu.write_reg(0xFF40, 0x82); // LCD on, sprites enabled
    ppu.skip_startup_for_test();
    let mut if_reg = 0u8;
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
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[1], 0x008BAC0F);
}

#[test]
fn cgb_obj_priority_mode_cgb() {
    let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
    ppu.write_reg(0xFF40, 0x82); // LCD on, sprites enabled
    ppu.skip_startup_for_test(); // Test priority on a normal line with OAM scanning.
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
    let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
    ppu.write_reg(0xFF40, 0x82); // LCD on, sprites enabled
    ppu.skip_startup_for_test(); // Test priority on a normal line with OAM scanning.
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
    let mut ppu = Ppu::new(Model::default());
    ppu.write_reg(0xFF40, 0x83); // LCD on, BG and OBJ
    ppu.skip_startup_for_test();
    let mut if_reg = 0u8;
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
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x008BAC0F);
}

#[test]
fn cgb_bg_attr_priority() {
    let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
    ppu.write_reg(0xFF40, 0x93); // BG and OBJ
    ppu.skip_startup_for_test(); // Exercise rendering after the blank startup frame.
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
    let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
    // LCD on, OBJ enabled, master priority cleared
    ppu.write_reg(0xFF40, 0x92);
    ppu.skip_startup_for_test(); // Test priority on a normal line with OAM scanning.
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
    let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
    ppu.write_reg(0xFF40, 0x91);
    ppu.skip_startup_for_test();
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
    let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
    ppu.write_reg(0xFF40, 0x91);
    ppu.skip_startup_for_test();
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
    let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
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
    let mut ppu = Ppu::new(Model::Cgb(CgbRevision::default()));
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
    let mut ppu = Ppu::new(Model::default());
    // LCD enabled, background/window disabled
    ppu.write_reg(0xFF40, 0x80);
    ppu.write_reg(0xFF47, 0xFC); // default palette
    ppu.skip_startup_for_test();
    let mut if_reg = 0u8;
    ppu.step(456, &mut if_reg);
    assert_eq!(ppu.framebuffer[0], 0x009BBC0F);
    assert_eq!(ppu.framebuffer[159], 0x009BBC0F);
}
