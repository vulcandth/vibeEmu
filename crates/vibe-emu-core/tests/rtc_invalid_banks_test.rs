mod common;

use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

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

fn expected_pixel_to_frame_color(expected: [u8; 3]) -> u32 {
    match expected {
        [0x00, 0x00, 0x00] => DMG_PALETTE[3],
        [0x55, 0x55, 0x55] => DMG_PALETTE[2],
        [0xAA, 0xAA, 0xAA] => DMG_PALETTE[1],
        [0xFF, 0xFF, 0xFF] => DMG_PALETTE[0],
        other => panic!("unexpected color {other:?} in reference png"),
    }
}

fn write_png_rgb(path: &std::path::Path, width: u32, height: u32, rgb: &[u8]) {
    assert_eq!(rgb.len(), (width * height * 3) as usize);
    let Some(parent) = path.parent() else {
        return;
    };
    let _ = std::fs::create_dir_all(parent);
    let file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = match encoder.write_header() {
        Ok(w) => w,
        Err(_) => return,
    };
    let _ = writer.write_image_data(rgb);
}

fn frame_to_rgb(frame: &[u32]) -> Vec<u8> {
    let mut out = vec![0u8; frame.len() * 3];
    for (i, &px) in frame.iter().enumerate() {
        out[i * 3] = ((px >> 16) & 0xFF) as u8;
        out[i * 3 + 1] = ((px >> 8) & 0xFF) as u8;
        out[i * 3 + 2] = (px & 0xFF) as u8;
    }
    out
}

#[test]
fn rtc_invalid_banks_png() {
    let mut gb = GameBoy::new(Model::default());

    let rom = std::fs::read(common::rom_path(
        "gbeshootout/cpp/rtc-invalid-banks-test.gb",
    ))
    .expect("rom not found");
    gb.mmu.load_cart(Cartridge::from_bytes(rom));

    // Give the ROM time to draw its result screen.
    run_for_frames(&mut gb, 600);

    let (width, height, expected) = common::load_png_rgb(common::rom_path(
        "gbeshootout/cpp/rtc-invalid-banks-test.png",
    ));
    assert_eq!(width, 160);
    assert_eq!(height, 144);

    let expected_frame: Vec<u32> = expected
        .iter()
        .copied()
        .map(expected_pixel_to_frame_color)
        .collect();

    let frame = gb.mmu.ppu.framebuffer();

    let mut mismatches = 0usize;
    let mut min_x = 160u32;
    let mut min_y = 144u32;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for (idx, &expected_color) in expected_frame.iter().enumerate() {
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
        println!(
            "rtc-invalid-banks-test mismatch: {mismatches} pixels differ (bbox x={min_x}..={max_x}, y={min_y}..={max_y})"
        );

        let actual_path = std::path::Path::new("target/tmp/rtc-invalid-banks-test.actual.png");
        write_png_rgb(actual_path, 160, 144, &frame_to_rgb(frame));

        let expected_path = std::path::Path::new("target/tmp/rtc-invalid-banks-test.expected.png");
        let expected_flat: Vec<u8> = expected.iter().flatten().copied().collect();
        write_png_rgb(expected_path, 160, 144, &expected_flat);

        let mut diff = vec![[0u8; 3]; expected_frame.len()];
        for (i, &expected_color) in expected_frame.iter().enumerate() {
            diff[i] = if frame[i] == expected_color {
                [0x00, 0x00, 0x00]
            } else {
                [0xFF, 0x00, 0x00]
            };
        }
        let diff_path = std::path::Path::new("target/tmp/rtc-invalid-banks-test.diff.png");
        let diff_flat: Vec<u8> = diff.into_iter().flatten().collect();
        write_png_rgb(diff_path, 160, 144, &diff_flat);

        // The ROM prints text; dump the BG map in a simple ASCII-ish view to make
        // it easier to see what differs even when the PNG mismatch is large.
        fn decode_tile(c: u8) -> char {
            if c <= 0x5F { (c + 0x20) as char } else { '.' }
        }

        fn dump_map(vram: &[[u8; 0x2000]; 2], base: usize) -> String {
            let mut out = String::new();
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

        let map_9800 = dump_map(&gb.mmu.ppu.vram, 0x1800);
        let map_9c00 = dump_map(&gb.mmu.ppu.vram, 0x1C00);
        println!("--- BG map @ 0x9800 ---\n{map_9800}");
        println!("--- BG map @ 0x9C00 ---\n{map_9c00}");

        fn render_tile_unsigned(vram: &[u8; 0x2000], tile: u8) -> [u32; 64] {
            let base = tile as usize * 16;
            let mut out = [0u32; 64];
            for row in 0..8usize {
                let lo = vram[base + row * 2];
                let hi = vram[base + row * 2 + 1];
                for col in 0..8usize {
                    let bit = 7 - col;
                    let color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
                    out[row * 8 + col] = DMG_PALETTE[color as usize];
                }
            }
            out
        }

        fn render_tile_signed(vram: &[u8; 0x2000], tile: u8) -> [u32; 64] {
            let base = 0x1000isize + (tile as i8 as isize) * 16;
            let base = base as usize;
            let mut out = [0u32; 64];
            for row in 0..8usize {
                let lo = vram[base + row * 2];
                let hi = vram[base + row * 2 + 1];
                for col in 0..8usize {
                    let bit = 7 - col;
                    let color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
                    out[row * 8 + col] = DMG_PALETTE[color as usize];
                }
            }
            out
        }

        fn decode_expected_tile(
            expected_frame: &[u32],
            tiles: &[[u32; 64]; 256],
            tile_x: usize,
            tile_y: usize,
        ) -> Option<u8> {
            let mut block = [0u32; 64];
            for row in 0..8usize {
                let y = tile_y * 8 + row;
                for col in 0..8usize {
                    let x = tile_x * 8 + col;
                    block[row * 8 + col] = expected_frame[y * 160 + x];
                }
            }
            tiles.iter().position(|t| t == &block).map(|idx| idx as u8)
        }

        fn tile_block_from_frame(frame: &[u32], tile_x: usize, tile_y: usize) -> [u32; 64] {
            let mut block = [0u32; 64];
            for row in 0..8usize {
                let y = tile_y * 8 + row;
                for col in 0..8usize {
                    let x = tile_x * 8 + col;
                    block[row * 8 + col] = frame[y * 160 + x];
                }
            }
            block
        }

        fn tile_matches(frame: &[u32], expected: &[u32], tile_x: usize, tile_y: usize) -> bool {
            for row in 0..8usize {
                let y = tile_y * 8 + row;
                for col in 0..8usize {
                    let x = tile_x * 8 + col;
                    let idx = y * 160 + x;
                    if frame[idx] != expected[idx] {
                        return false;
                    }
                }
            }
            true
        }

        let mut tiles_unsigned = [[0u32; 64]; 256];
        let mut tiles_signed = [[0u32; 64]; 256];
        for i in 0..256u16 {
            tiles_unsigned[i as usize] = render_tile_unsigned(&gb.mmu.ppu.vram[0], i as u8);
            tiles_signed[i as usize] = render_tile_signed(&gb.mmu.ppu.vram[0], i as u8);
        }

        // If the expected tile block can't be matched against current VRAM tile data,
        // try inferring its ID by finding an identical 8x8 block elsewhere on the
        // screen where we *do* match the reference.
        let mut expected_block_to_id: std::collections::HashMap<[u32; 64], u8> =
            std::collections::HashMap::new();
        for ty in 0..18usize {
            for tx in 0..20usize {
                if !tile_matches(frame, &expected_frame, tx, ty) {
                    continue;
                }
                let block = tile_block_from_frame(&expected_frame, tx, ty);
                let id = gb.mmu.ppu.vram[0][0x1800 + ty * 32 + tx];
                expected_block_to_id.entry(block).or_insert(id);
            }
        }

        // The mismatch bbox is consistently in the rightmost few BG tiles.
        // Decode the expected tile IDs for those columns so we can see which
        // values are incorrect.
        let tile_min_x = (min_x / 8) as usize;
        let tile_max_x = (max_x / 8) as usize;
        let tile_min_y = (min_y / 8) as usize;
        let tile_max_y = (max_y / 8) as usize;
        println!("--- Expected vs actual BG tile IDs (0x9800) for mismatch region ---");
        for ty in tile_min_y..=tile_max_y.min(17) {
            for tx in tile_min_x..=tile_max_x.min(19) {
                let actual = gb.mmu.ppu.vram[0][0x1800 + ty * 32 + tx];
                let expected_tile = decode_expected_tile(&expected_frame, &tiles_unsigned, tx, ty)
                    .or_else(|| decode_expected_tile(&expected_frame, &tiles_signed, tx, ty))
                    .or_else(|| {
                        let block = tile_block_from_frame(&expected_frame, tx, ty);
                        expected_block_to_id.get(&block).copied()
                    })
                    .map(|v| format!("{:02X}", v))
                    .unwrap_or_else(|| "??".to_string());
                print!("({},{}) exp={} act={:02X}  ", tx, ty, expected_tile, actual);
            }
            println!();
        }

        println!(
            "serial={}",
            String::from_utf8_lossy(gb.mmu.serial.peek_output())
        );
    }

    assert_eq!(mismatches, 0, "rtc-invalid-banks-test PNG mismatch");
}
