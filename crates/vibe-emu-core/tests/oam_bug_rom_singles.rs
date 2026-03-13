mod common;

use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

fn decode_blargg_tile_space_at_zero(tile: u8) -> char {
    // Common blargg shell convention: printable ASCII starting at tile 0 (space).
    if tile <= 0x5F {
        (tile + 0x20) as char
    } else {
        '.'
    }
}

fn decode_blargg_tile_ascii_index(tile: u8) -> char {
    // Some ROMs upload a font where tile index == ASCII code.
    if (0x20..=0x7E).contains(&tile) {
        tile as char
    } else {
        '.'
    }
}

fn dump_bg_map_ascii(vram: &[[u8; 0x2000]; 2], base: usize, decode: fn(u8) -> char) -> String {
    let mut out = String::new();
    // Visible screen is 20x18 tiles.
    for row in 0..18usize {
        let start = base + row * 32;
        let line: String = vram[0][start..start + 20]
            .iter()
            .copied()
            .map(decode)
            .collect();
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn tilemap_row_contains(hay_row: &[u8], needle: &[u8]) -> bool {
    hay_row.windows(needle.len()).any(|window| window == needle)
}

fn tilemap_contains_word(map: &[u8], needle: &[u8]) -> bool {
    debug_assert_eq!(map.len(), 32 * 32);
    for row in 0..32usize {
        let start = row * 32;
        if tilemap_row_contains(&map[start..start + 32], needle) {
            return true;
        }
    }
    false
}

fn screen_contains_pass_fail(vram: &[[u8; 0x2000]; 2]) -> Option<bool> {
    // Scan both possible tilemap bases (LCDC bit 3 / bit 6 select BG/WIN maps).
    // Layout in our VRAM array: tilemap @ 0x9800 == offset 0x1800, @ 0x9C00 == 0x1C00.
    let maps = [
        &vram[0][0x1800..0x1800 + 1024],
        &vram[0][0x1C00..0x1C00 + 1024],
    ];

    // Two common encodings:
    // - tile index == ASCII code
    // - tile index 0 == ' ' (ASCII 0x20)
    const PASSED_ASCII: &[u8] = b"Passed";
    const FAILED_ASCII: &[u8] = b"Failed";

    let passed_tile0 = PASSED_ASCII
        .iter()
        .map(|b| b.saturating_sub(0x20))
        .collect::<Vec<_>>();
    let failed_tile0 = FAILED_ASCII
        .iter()
        .map(|b| b.saturating_sub(0x20))
        .collect::<Vec<_>>();

    for map in maps {
        if tilemap_contains_word(map, PASSED_ASCII) || tilemap_contains_word(map, &passed_tile0) {
            return Some(true);
        }
        if tilemap_contains_word(map, FAILED_ASCII) || tilemap_contains_word(map, &failed_tile0) {
            return Some(false);
        }
    }

    None
}

fn run_until_passed_or_failed(
    gb: &mut GameBoy,
    max_frames: u32,
    max_dot_cycles: u64,
) -> Result<(), String> {
    let mut frames = 0u32;
    let start_cycles = gb.cpu.cycles;
    let mut last_check_cycles = start_cycles;

    while frames < max_frames && gb.cpu.cycles.wrapping_sub(start_cycles) < max_dot_cycles {
        gb.cpu.step(&mut gb.mmu);

        let mut should_check = false;
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
            should_check = true;
        }

        // Some failures (or LCD-off states) might never produce frame_ready.
        // Check periodically based on dot-cycles so tests can't hang.
        let cycles = gb.cpu.cycles;
        if cycles.wrapping_sub(last_check_cycles) >= 10_000 {
            last_check_cycles = cycles;
            should_check = true;
        }

        if should_check {
            match screen_contains_pass_fail(&gb.mmu.ppu.vram) {
                Some(true) => return Ok(()),
                Some(false) => {
                    let map_9800_space0 = dump_bg_map_ascii(
                        &gb.mmu.ppu.vram,
                        0x1800,
                        decode_blargg_tile_space_at_zero,
                    );
                    let map_9c00_space0 = dump_bg_map_ascii(
                        &gb.mmu.ppu.vram,
                        0x1C00,
                        decode_blargg_tile_space_at_zero,
                    );
                    let map_9800_ascii =
                        dump_bg_map_ascii(&gb.mmu.ppu.vram, 0x1800, decode_blargg_tile_ascii_index);
                    let map_9c00_ascii =
                        dump_bg_map_ascii(&gb.mmu.ppu.vram, 0x1C00, decode_blargg_tile_ascii_index);

                    return Err(format!(
                        "screen reported Failed\nframes={frames} cycles={} lcd_enabled={}\n--- BG map @ 0x9800 (space-at-0 decode) ---\n{map_9800_space0}\n--- BG map @ 0x9C00 (space-at-0 decode) ---\n{map_9c00_space0}\n--- BG map @ 0x9800 (ascii-index decode) ---\n{map_9800_ascii}\n--- BG map @ 0x9C00 (ascii-index decode) ---\n{map_9c00_ascii}\nserial={}\n",
                        gb.cpu.cycles.wrapping_sub(start_cycles),
                        gb.mmu.ppu.lcd_enabled(),
                        String::from_utf8_lossy(gb.mmu.serial.peek_output())
                    ));
                }
                None => {}
            }
        }
    }

    let map_9800_space0 =
        dump_bg_map_ascii(&gb.mmu.ppu.vram, 0x1800, decode_blargg_tile_space_at_zero);
    let map_9c00_space0 =
        dump_bg_map_ascii(&gb.mmu.ppu.vram, 0x1C00, decode_blargg_tile_space_at_zero);
    let map_9800_ascii =
        dump_bg_map_ascii(&gb.mmu.ppu.vram, 0x1800, decode_blargg_tile_ascii_index);
    let map_9c00_ascii =
        dump_bg_map_ascii(&gb.mmu.ppu.vram, 0x1C00, decode_blargg_tile_ascii_index);
    Err(format!(
        "timeout waiting for Passed on-screen (max_frames={max_frames}, max_cycles={max_dot_cycles})\nframes={frames} cycles={} lcd_enabled={}\n--- BG map @ 0x9800 (space-at-0 decode) ---\n{map_9800_space0}\n--- BG map @ 0x9C00 (space-at-0 decode) ---\n{map_9c00_space0}\n--- BG map @ 0x9800 (ascii-index decode) ---\n{map_9800_ascii}\n--- BG map @ 0x9C00 (ascii-index decode) ---\n{map_9c00_ascii}\nserial={}\n",
        gb.cpu.cycles.wrapping_sub(start_cycles),
        gb.mmu.ppu.lcd_enabled(),
        String::from_utf8_lossy(gb.mmu.serial.peek_output())
    ))
}

#[test]
fn blargg_oam_bug_rom_singles_dmg() {
    // The folder contents are stable; keeping the list explicit makes failures clearer.
    let roms = [
        "blargg/oam_bug/rom_singles/1-lcd_sync.gb",
        "blargg/oam_bug/rom_singles/2-causes.gb",
        "blargg/oam_bug/rom_singles/3-non_causes.gb",
        "blargg/oam_bug/rom_singles/4-scanline_timing.gb",
        "blargg/oam_bug/rom_singles/5-timing_bug.gb",
        "blargg/oam_bug/rom_singles/6-timing_no_bug.gb",
        "blargg/oam_bug/rom_singles/8-instr_effect.gb",
    ];

    let rom_filter = std::env::var("VIBEEMU_OAMBUG_ROM_FILTER").ok();

    let mut failures = Vec::new();

    for rel in roms {
        if let Some(filter) = &rom_filter
            && !rel.contains(filter)
        {
            continue;
        }
        let mut gb = GameBoy::new(Model::default());
        let rom = std::fs::read(common::rom_path(rel)).expect("rom not found");
        gb.mmu.load_cart(Cartridge::from_bytes(rom));

        // These are small single-purpose ROMs; they should finish quickly.
        // Use a hard limit in dot-cycles so the test can't run forever even if
        // the PPU never produces a completed frame.
        let max_frames = 1200;
        let max_dot_cycles = 400u64 * 70224u64; // ~400 frames worth of dot cycles
        if let Err(details) = run_until_passed_or_failed(&mut gb, max_frames, max_dot_cycles) {
            failures.push(format!("{rel}: {details}"));
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} oam_bug rom_singles failures:\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}
