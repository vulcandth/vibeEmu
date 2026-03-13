mod common;
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, Model},
};

const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

fn run_for_frames(gb: &mut GameBoy, frames: u32) {
    let mut completed = 0u32;
    while completed < frames {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            completed += 1;
        }
    }
}

#[test]
fn interrupt_time_dmg_png() {
    let mut gb = GameBoy::new(Model::default());
    let rom = std::fs::read(common::rom_path("blargg/interrupt_time/interrupt_time.gb"))
        .expect("rom not found");
    gb.mmu.load_cart(Cartridge::from_bytes(rom));

    run_for_frames(&mut gb, 120);

    let (width, height, expected) = common::load_png_rgb(common::rom_path(
        "blargg/interrupt_time/interrupt_time-dmg.png",
    ));
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
        assert_eq!(frame[idx], expected_color, "pixel mismatch at index {idx}");
    }
}

#[test]
fn interrupt_time_cgb_png() {
    let mut gb = GameBoy::new(Model::Cgb(CgbRevision::default()));
    let rom = std::fs::read(common::rom_path("blargg/interrupt_time/interrupt_time.gb"))
        .expect("rom not found");
    gb.mmu.load_cart(Cartridge::from_bytes(rom));

    run_for_frames(&mut gb, 120);

    let (width, height, expected) = common::load_png_rgb(common::rom_path(
        "blargg/interrupt_time/interrupt_time-cgb.png",
    ));
    assert_eq!(width, 160);
    assert_eq!(height, 144);

    let frame = gb.mmu.ppu.framebuffer();

    let mut mismatches = 0usize;
    let mut min_x = 160u32;
    let mut min_y = 144u32;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for (idx, pixel) in expected.iter().enumerate() {
        let &[r, g, b] = pixel;
        let expected_color = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        if frame[idx] != expected_color {
            mismatches += 1;
            let x = (idx as u32) % 160;
            let y = (idx as u32) / 160;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    if mismatches != 0 {
        fn decode_tile(c: u8) -> char {
            // blargg's shell font typically packs printable ASCII starting at tile 0.
            if c <= 0x5F { (c + 0x20) as char } else { '.' }
        }

        fn dump_map(vram: &[[u8; 0x2000]; 2], base: usize) -> String {
            let mut out = String::new();
            // Visible screen is 20x18 tiles.
            for row in 0..18usize {
                let start = base + row * 32;
                let line: String = vram[0][start..start + 20]
                    .iter()
                    .copied()
                    .map(decode_tile)
                    .collect();
                out.push_str(&line);
                out.push('\n');
            }
            out
        }

        fn dump_map_hex(vram: &[[u8; 0x2000]; 2], base: usize, rows: usize) -> String {
            let mut out = String::new();
            for row in 0..rows {
                let start = base + row * 32;
                for &tile in &vram[0][start..start + 20] {
                    out.push_str(&format!("{tile:02X} "));
                }
                out.push('\n');
            }
            out
        }

        // BG map base is controlled by LCDC bit 3; dump both common bases.
        let map_9800 = dump_map(&gb.mmu.ppu.vram, 0x1800);
        let map_9c00 = dump_map(&gb.mmu.ppu.vram, 0x1C00);
        println!("--- BG map @ 0x9800 ---\n{}", map_9800);
        println!("--- BG map @ 0x9C00 ---\n{}", map_9c00);

        println!(
            "--- BG map @ 0x9800 (hex, first 10 rows) ---\n{}",
            dump_map_hex(&gb.mmu.ppu.vram, 0x1800, 10)
        );

        // Decode the *reference* PNG into characters using the font tiles the
        // ROM has uploaded into VRAM (tile index == ASCII code).
        fn glyph_on(vram: &[u8; 0x2000], ascii: u8, x: usize, y: usize) -> bool {
            let base = ascii as usize * 16;
            if base + 15 >= vram.len() {
                return false;
            }
            let lo = vram[base + y * 2];
            let hi = vram[base + y * 2 + 1];
            let bit = 7 - x;
            let color = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
            color != 0
        }

        fn decode_reference_png(expected_rgb: &[[u8; 3]], vram: &[[u8; 0x2000]; 2]) -> String {
            let mut out = String::new();
            for ty in 0..18usize {
                for tx in 0..20usize {
                    // Determine block background color as the most common pixel.
                    let mut bg = [0u8; 3];
                    let mut bg_count = 0u32;
                    for py in 0..8usize {
                        for px in 0..8usize {
                            let idx = (ty * 8 + py) * 160 + (tx * 8 + px);
                            let p = expected_rgb[idx];
                            if bg_count == 0 {
                                bg = p;
                                bg_count = 1;
                            } else if p == bg {
                                bg_count += 1;
                            }
                        }
                    }

                    // Match against printable ASCII.
                    let mut matched = None;
                    'ascii: for ascii in 0x20u8..=0x7Eu8 {
                        for py in 0..8usize {
                            for px in 0..8usize {
                                let idx = (ty * 8 + py) * 160 + (tx * 8 + px);
                                let on = expected_rgb[idx] != bg;
                                if on != glyph_on(&vram[0], ascii, px, py) {
                                    continue 'ascii;
                                }
                            }
                        }
                        matched = Some(ascii as char);
                        break;
                    }

                    out.push(matched.unwrap_or('?'));
                }
                out.push('\n');
            }
            out
        }

        println!(
            "--- Decoded reference CGB PNG (best-effort) ---\n{}",
            decode_reference_png(&expected, &gb.mmu.ppu.vram)
        );
    }

    assert!(
        mismatches == 0,
        "CGB PNG mismatch: {mismatches} pixels differ (bbox x={min_x}..={max_x}, y={min_y}..={max_y}); serial={}",
        String::from_utf8_lossy(gb.mmu.serial.peek_output())
    );
}
