mod common;

use std::{fmt::Write as _, path::Path};

use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

fn tile_id_ascii(tile_id: u8) -> char {
    match tile_id {
        0x00 => ' ',
        0x20..=0x7E => tile_id as char,
        _ => '.',
    }
}

fn bg_tilemap_ascii_dump(gb: &mut GameBoy) -> String {
    let lcdc = gb.mmu.read_byte(0xFF40);
    let scy = gb.mmu.read_byte(0xFF42);
    let scx = gb.mmu.read_byte(0xFF43);

    let bg_map_base = if lcdc & 0x08 != 0 {
        0x1C00usize
    } else {
        0x1800usize
    };
    let vram = &gb.mmu.ppu.vram[0];

    let tile_row0 = (scy / 8) as usize;
    let tile_col0 = (scx / 8) as usize;
    let visible_rows = 18usize;
    let visible_cols = 20usize;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "BG tilemap IDs (interpreted as ASCII) lcdc={:02X} scx={:02X} scy={:02X} map_base=0x{:04X}",
        lcdc,
        scx,
        scy,
        0x8000u16 + bg_map_base as u16
    );

    for y in 0..visible_rows {
        for x in 0..visible_cols {
            let map_row = (tile_row0 + y) & 31;
            let map_col = (tile_col0 + x) & 31;
            let tile_id = vram[bg_map_base + map_row * 32 + map_col];
            out.push(tile_id_ascii(tile_id));
        }
        out.push('\n');
    }

    let _ = writeln!(
        out,
        "BG tilemap IDs (interpreted as ASCII) at origin (0,0):"
    );
    for y in 0..visible_rows {
        for x in 0..visible_cols {
            let map_row = y & 31;
            let map_col = x & 31;
            let tile_id = vram[bg_map_base + map_row * 32 + map_col];
            out.push(tile_id_ascii(tile_id));
        }
        out.push('\n');
    }

    let _ = writeln!(out, "BG tilemap IDs (hex) scrolled:");
    for y in 0..visible_rows {
        for x in 0..visible_cols {
            let map_row = (tile_row0 + y) & 31;
            let map_col = (tile_col0 + x) & 31;
            let tile_id = vram[bg_map_base + map_row * 32 + map_col];
            let _ = write!(out, "{:02X} ", tile_id);
        }
        out.push('\n');
    }

    out
}

fn run_rom<P: AsRef<Path>, Q: AsRef<Path>>(rom_path: P, screenshot_path: Q, frames_to_run: u32) {
    let mut gb = GameBoy::new(Model::default());
    let rom = std::fs::read(rom_path).expect("rom not found");
    gb.mmu.load_cart(Cartridge::from_bytes(rom));

    let mut frames = 0u32;
    while frames < frames_to_run {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
        }
    }

    let (width, height, expected) = common::load_png_rgb(screenshot_path);
    assert_eq!(width, 160);
    assert_eq!(height, 144);

    let mut first_mismatch: Option<(usize, u32, u32)> = None;
    for (idx, pixel) in expected.iter().enumerate() {
        let expected_color = match *pixel {
            [0xE0, 0xF8, 0xD0] => DMG_PALETTE[0],
            [0x08, 0x18, 0x20] => DMG_PALETTE[3],
            other => panic!("unexpected color {:?}", other),
        };

        let actual_color = gb.mmu.ppu.framebuffer()[idx];
        if actual_color != expected_color {
            first_mismatch = Some((idx, expected_color, actual_color));
            break;
        }
    }

    if let Some((idx, expected_color, actual_color)) = first_mismatch {
        let tilemap = bg_tilemap_ascii_dump(&mut gb);
        let apu_state = gb.mmu.apu.debug_state();
        panic!(
            "pixel mismatch at index {idx}: expected 0x{expected_color:08X} got 0x{actual_color:08X}\nAPU: {apu_state:?}\n{tilemap}"
        );
    }
}

fn run_single_with_frames(name: &str, frames_to_run: u32) {
    let rom = common::roms_dir()
        .join("blargg/dmg_sound/rom_singles")
        .join(name);
    let screenshot_name = name.replace(' ', "_").replace(".gb", ".png");
    let screenshot = common::test_assets_dir()
        .join("blargg/dmg_sound/rom_singles")
        .join(screenshot_name);
    run_rom(rom, screenshot, frames_to_run);
}

fn run_single(name: &str) {
    run_single_with_frames(name, 120);
}

#[test]
fn dmg_sound_01_registers() {
    run_single("01-registers.gb");
}

#[test]
fn dmg_sound_02_len_ctr() {
    // This ROM takes longer than 2 seconds to settle on its final PASS/FAIL screen.
    run_single_with_frames("02-len ctr.gb", 800);
}

#[test]
fn dmg_sound_03_trigger() {
    run_single_with_frames("03-trigger.gb", 1200);
}

#[test]
fn dmg_sound_04_sweep() {
    run_single("04-sweep.gb");
}

#[test]
fn dmg_sound_05_sweep_details() {
    run_single("05-sweep details.gb");
}

#[test]
fn dmg_sound_06_overflow_on_trigger() {
    run_single("06-overflow on trigger.gb");
}

#[test]
fn dmg_sound_07_len_sweep_period_sync() {
    run_single("07-len sweep period sync.gb");
}

#[test]
fn dmg_sound_08_len_ctr_during_power() {
    run_single("08-len ctr during power.gb");
}

#[test]
fn dmg_sound_09_wave_read_while_on() {
    run_single("09-wave read while on.gb");
}

#[test]
fn dmg_sound_10_wave_trigger_while_on() {
    run_single_with_frames("10-wave trigger while on.gb", 300);
}

#[test]
fn dmg_sound_11_regs_after_power() {
    run_single("11-regs after power.gb");
}

#[test]
fn dmg_sound_12_wave_write_while_on() {
    run_single_with_frames("12-wave write while on.gb", 300);
}
