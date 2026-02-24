#![allow(non_snake_case)]

mod common;

use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision},
};

const SCREEN_W: u32 = 160;
const SCREEN_H: u32 = 144;
const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

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

fn expected_rgb_to_frame_color(rgb: [u8; 3], cgb: bool) -> u32 {
    if cgb {
        // CGB expectations use literal RGB values, including grayscale shades.
        let [r, g, b] = rgb;
        return ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
    }

    match rgb {
        // Mealybug DMG expectations use exact grayscale shades: 00/55/AA/FF.
        [0x00, 0x00, 0x00] => DMG_PALETTE[3],
        [0x55, 0x55, 0x55] => DMG_PALETTE[2],
        [0xAA, 0xAA, 0xAA] => DMG_PALETTE[1],
        [0xFF, 0xFF, 0xFF] => DMG_PALETTE[0],
        [r, g, b] => ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
    }
}

fn run_until_ld_b_b(
    rom_path: &std::path::Path,
    cgb: bool,
    dmg_revision: DmgRevision,
    cgb_revision: CgbRevision,
    max_cycles: u64,
) -> GameBoy {
    let rom = std::fs::read(rom_path).expect("rom not found");
    let cart = Cartridge::load(rom);

    let mut gb = GameBoy::new_with_revisions(cgb, dmg_revision, cgb_revision);
    gb.mmu.load_cart(cart);

    while gb.cpu.cycles < max_cycles {
        let pc = gb.cpu.pc;
        let opcode = gb.mmu.read_byte(pc);

        // Mealybug Tearoom exit condition: execute LD B,B (0x40).
        // The suite expects the screenshot taken at the breakpoint, i.e. on
        // instruction execution rather than at fetch.
        if opcode == 0x40 {
            gb.cpu.step(&mut gb.mmu);
            return gb;
        }

        gb.cpu.step(&mut gb.mmu);
    }

    println!("mealybug tearoom: timeout");
    println!(
        "pc={:04X} opcode={:02X}",
        gb.cpu.pc,
        gb.mmu.read_byte(gb.cpu.pc)
    );
    println!(
        "af={:02X}{:02X} bc={:02X}{:02X} de={:02X}{:02X} hl={:02X}{:02X} sp={:04X}",
        gb.cpu.a, gb.cpu.f, gb.cpu.b, gb.cpu.c, gb.cpu.d, gb.cpu.e, gb.cpu.h, gb.cpu.l, gb.cpu.sp
    );
    println!("serial output (partial): {:?}", gb.mmu.serial.peek_output());
    panic!("mealybug tearoom: timeout");
}

fn assert_png_match(gb: &GameBoy, expected_png: &std::path::Path, debug_stem: &str, cgb: bool) {
    let (w, h, expected) = common::load_png_rgb(expected_png);
    assert_eq!(w, SCREEN_W);
    assert_eq!(h, SCREEN_H);

    let expected_frame: Vec<u32> = expected
        .iter()
        .copied()
        .map(|rgb| expected_rgb_to_frame_color(rgb, cgb))
        .collect();

    let frame = gb.mmu.ppu.framebuffer();
    let mut mismatches = 0usize;
    let mut min_x = SCREEN_W;
    let mut min_y = SCREEN_H;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for (idx, &exp) in expected_frame.iter().enumerate() {
        if frame[idx] != exp {
            mismatches += 1;
            let x = (idx as u32) % SCREEN_W;
            let y = (idx as u32) / SCREEN_W;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    if mismatches != 0 {
        println!(
            "{debug_stem} mismatch: {mismatches} pixels differ (bbox x={min_x}..={max_x}, y={min_y}..={max_y})"
        );
        if std::env::var("VIBEEMU_DEBUG_MISMATCH_COORDS")
            .ok()
            .is_some()
        {
            let limit = std::env::var("VIBEEMU_DEBUG_MISMATCH_LIMIT")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(256);
            let mut printed = 0usize;
            for (idx, &exp) in expected_frame.iter().enumerate() {
                if frame[idx] == exp {
                    continue;
                }
                let x = (idx as u32) % SCREEN_W;
                let y = (idx as u32) / SCREEN_W;
                println!(
                    "mismatch_xy x={} y={} actual={:08X} expected={:08X}",
                    x, y, frame[idx], exp
                );
                printed += 1;
                if printed >= limit {
                    break;
                }
            }
        }

        let actual_path = std::path::Path::new("target/tmp/mealybug-tearoom-tests")
            .join(format!("{debug_stem}.actual.png"));
        write_png_rgb(&actual_path, SCREEN_W, SCREEN_H, &frame_to_rgb(frame));

        let expected_path = std::path::Path::new("target/tmp/mealybug-tearoom-tests")
            .join(format!("{debug_stem}.expected.png"));
        let expected_flat: Vec<u8> = expected.iter().flatten().copied().collect();
        write_png_rgb(&expected_path, SCREEN_W, SCREEN_H, &expected_flat);

        let mut diff = vec![[0u8; 3]; expected_frame.len()];
        for (i, &exp) in expected_frame.iter().enumerate() {
            diff[i] = if frame[i] == exp {
                [0x00, 0x00, 0x00]
            } else {
                [0xFF, 0x00, 0x00]
            };
        }
        let diff_path = std::path::Path::new("target/tmp/mealybug-tearoom-tests")
            .join(format!("{debug_stem}.diff.png"));
        let diff_flat: Vec<u8> = diff.into_iter().flatten().collect();
        write_png_rgb(&diff_path, SCREEN_W, SCREEN_H, &diff_flat);

        println!(
            "serial={} ",
            String::from_utf8_lossy(gb.mmu.serial.peek_output())
        );
    }

    assert_eq!(mismatches, 0, "{debug_stem} PNG mismatch");
}

fn run_mealybug_case(
    rom_rel: &str,
    expected_rel: &str,
    cgb: bool,
    dmg_revision: DmgRevision,
    cgb_revision: CgbRevision,
    max_cycles: u64,
    debug_stem: &str,
) {
    let rom_path = common::rom_path(rom_rel);
    let expected_path = common::rom_path(expected_rel);
    if !expected_path.exists() {
        eprintln!(
            "skipping {debug_stem}: missing reference screenshot {}",
            expected_path.display()
        );
        return;
    }

    let gb = run_until_ld_b_b(&rom_path, cgb, dmg_revision, cgb_revision, max_cycles);
    assert_png_match(&gb, &expected_path, debug_stem, cgb);
}

macro_rules! mealybug_test {
    (ignore $name:ident, $rom:literal, $expected:literal, $cgb:expr, $dmg_rev:expr, $cgb_rev:expr) => {
        #[test]
        #[ignore]
        fn $name() {
            run_mealybug_case(
                $rom,
                $expected,
                $cgb,
                $dmg_rev,
                $cgb_rev,
                50_000_000,
                stringify!($name),
            );
        }
    };
    ($name:ident, $rom:literal, $expected:literal, $cgb:expr, $dmg_rev:expr, $cgb_rev:expr) => {
        #[test]
        fn $name() {
            run_mealybug_case(
                $rom,
                $expected,
                $cgb,
                $dmg_rev,
                $cgb_rev,
                50_000_000,
                stringify!($name),
            );
        }
    };
}

// --- PPU suite (expected screenshots provided) ---

mealybug_test!(
    ppu_m2_win_en_toggle_dmg_blob,
    "mealybug-tearoom-tests/ppu/m2_win_en_toggle.gb",
    "mealybug-tearoom-tests/ppu/m2_win_en_toggle_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m2_win_en_toggle_cgb_c,
    "mealybug-tearoom-tests/ppu/m2_win_en_toggle.gb",
    "mealybug-tearoom-tests/ppu/m2_win_en_toggle_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m2_win_en_toggle_cgb_d,
    "mealybug-tearoom-tests/ppu/m2_win_en_toggle.gb",
    "mealybug-tearoom-tests/ppu/m2_win_en_toggle_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_bgp_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_bgp_change.gb",
    "mealybug-tearoom-tests/ppu/m3_bgp_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_bgp_change_cgb_c, "mealybug-tearoom-tests/ppu/m3_bgp_change.gb", "mealybug-tearoom-tests/ppu/m3_bgp_change_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);
mealybug_test!(
    ppu_m3_bgp_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_bgp_change.gb",
    "mealybug-tearoom-tests/ppu/m3_bgp_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_bgp_change_sprites_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_bgp_change_sprites.gb",
    "mealybug-tearoom-tests/ppu/m3_bgp_change_sprites_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_bgp_change_sprites_cgb_c, "mealybug-tearoom-tests/ppu/m3_bgp_change_sprites.gb", "mealybug-tearoom-tests/ppu/m3_bgp_change_sprites_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);
mealybug_test!(
    ppu_m3_bgp_change_sprites_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_bgp_change_sprites.gb",
    "mealybug-tearoom-tests/ppu/m3_bgp_change_sprites_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_lcdc_bg_en_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_lcdc_bg_en_change_dmg_b, "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change.gb", "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change_dmg_b.png", false, DmgRevision::RevB, CgbRevision::default());
mealybug_test!(
    ppu_m3_lcdc_bg_en_change_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_lcdc_bg_en_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(ignore ppu_m3_lcdc_bg_en_change2_cgb_c, "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change2.gb", "mealybug-tearoom-tests/ppu/m3_lcdc_bg_en_change2_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);

mealybug_test!(
    ppu_m3_lcdc_bg_map_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_map_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_map_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_lcdc_bg_map_change_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_map_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_map_change_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_lcdc_bg_map_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_map_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_bg_map_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(ignore ppu_m3_lcdc_bg_map_change2_cgb_c, "mealybug-tearoom-tests/ppu/m3_lcdc_bg_map_change2.gb", "mealybug-tearoom-tests/ppu/m3_lcdc_bg_map_change2_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);

mealybug_test!(
    ppu_m3_lcdc_obj_en_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_lcdc_obj_en_change_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_lcdc_obj_en_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_lcdc_obj_en_change_variant_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change_variant.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change_variant_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_lcdc_obj_en_change_variant_cgb_c, "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change_variant.gb", "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change_variant_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);
mealybug_test!(
    ppu_m3_lcdc_obj_en_change_variant_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change_variant.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_en_change_variant_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_lcdc_obj_size_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_lcdc_obj_size_change_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_lcdc_obj_size_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_lcdc_obj_size_change_scx_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change_scx.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change_scx_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_lcdc_obj_size_change_scx_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change_scx.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change_scx_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_lcdc_obj_size_change_scx_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change_scx.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_obj_size_change_scx_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_lcdc_tile_sel_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_lcdc_tile_sel_change_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_change_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_lcdc_tile_sel_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(ignore ppu_m3_lcdc_tile_sel_change2_cgb_c, "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_change2.gb", "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_change2_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);

mealybug_test!(
    ppu_m3_lcdc_tile_sel_win_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_win_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_win_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_lcdc_tile_sel_win_change_cgb_c, "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_win_change.gb", "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_win_change_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);
mealybug_test!(
    ppu_m3_lcdc_tile_sel_win_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_win_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_win_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(ignore ppu_m3_lcdc_tile_sel_win_change2_cgb_c, "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_win_change2.gb", "mealybug-tearoom-tests/ppu/m3_lcdc_tile_sel_win_change2_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);

mealybug_test!(
    ppu_m3_lcdc_win_en_change_multiple_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_lcdc_win_en_change_multiple_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_lcdc_win_en_change_multiple_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_lcdc_win_en_change_multiple_wx_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple_wx.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple_wx_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_lcdc_win_en_change_multiple_wx_dmg_b, "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple_wx.gb", "mealybug-tearoom-tests/ppu/m3_lcdc_win_en_change_multiple_wx_dmg_b.png", false, DmgRevision::RevB, CgbRevision::default());

mealybug_test!(
    ppu_m3_lcdc_win_map_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_map_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_map_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_lcdc_win_map_change_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_map_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_map_change_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_lcdc_win_map_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_map_change.gb",
    "mealybug-tearoom-tests/ppu/m3_lcdc_win_map_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(ignore ppu_m3_lcdc_win_map_change2_cgb_c, "mealybug-tearoom-tests/ppu/m3_lcdc_win_map_change2.gb", "mealybug-tearoom-tests/ppu/m3_lcdc_win_map_change2_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);

mealybug_test!(
    ppu_m3_obp0_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_obp0_change.gb",
    "mealybug-tearoom-tests/ppu/m3_obp0_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_obp0_change_cgb_c, "mealybug-tearoom-tests/ppu/m3_obp0_change.gb", "mealybug-tearoom-tests/ppu/m3_obp0_change_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);
mealybug_test!(
    ppu_m3_obp0_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_obp0_change.gb",
    "mealybug-tearoom-tests/ppu/m3_obp0_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_scx_high_5_bits_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_scx_high_5_bits.gb",
    "mealybug-tearoom-tests/ppu/m3_scx_high_5_bits_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_scx_high_5_bits_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_scx_high_5_bits.gb",
    "mealybug-tearoom-tests/ppu/m3_scx_high_5_bits_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_scx_high_5_bits_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_scx_high_5_bits.gb",
    "mealybug-tearoom-tests/ppu/m3_scx_high_5_bits_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(ignore ppu_m3_scx_high_5_bits_change2_cgb_c, "mealybug-tearoom-tests/ppu/m3_scx_high_5_bits_change2.gb", "mealybug-tearoom-tests/ppu/m3_scx_high_5_bits_change2_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);

mealybug_test!(
    ppu_m3_scx_low_3_bits_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_scx_low_3_bits.gb",
    "mealybug-tearoom-tests/ppu/m3_scx_low_3_bits_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_scx_low_3_bits_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_scx_low_3_bits.gb",
    "mealybug-tearoom-tests/ppu/m3_scx_low_3_bits_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_scx_low_3_bits_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_scx_low_3_bits.gb",
    "mealybug-tearoom-tests/ppu/m3_scx_low_3_bits_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_scy_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_scy_change.gb",
    "mealybug-tearoom-tests/ppu/m3_scy_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_scy_change_cgb_c, "mealybug-tearoom-tests/ppu/m3_scy_change.gb", "mealybug-tearoom-tests/ppu/m3_scy_change_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);
mealybug_test!(
    ppu_m3_scy_change_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_scy_change.gb",
    "mealybug-tearoom-tests/ppu/m3_scy_change_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(ignore ppu_m3_scy_change2_cgb_c, "mealybug-tearoom-tests/ppu/m3_scy_change2.gb", "mealybug-tearoom-tests/ppu/m3_scy_change2_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);

mealybug_test!(
    ppu_m3_window_timing_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_window_timing.gb",
    "mealybug-tearoom-tests/ppu/m3_window_timing_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_window_timing_cgb_c, "mealybug-tearoom-tests/ppu/m3_window_timing.gb", "mealybug-tearoom-tests/ppu/m3_window_timing_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);
mealybug_test!(
    ppu_m3_window_timing_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_window_timing.gb",
    "mealybug-tearoom-tests/ppu/m3_window_timing_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_window_timing_wx_0_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_window_timing_wx_0.gb",
    "mealybug-tearoom-tests/ppu/m3_window_timing_wx_0_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(ignore ppu_m3_window_timing_wx_0_cgb_c, "mealybug-tearoom-tests/ppu/m3_window_timing_wx_0.gb", "mealybug-tearoom-tests/ppu/m3_window_timing_wx_0_cgb_c.png", true, DmgRevision::default(), CgbRevision::RevC);
mealybug_test!(
    ppu_m3_window_timing_wx_0_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_window_timing_wx_0.gb",
    "mealybug-tearoom-tests/ppu/m3_window_timing_wx_0_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_wx_4_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_wx_4_change.gb",
    "mealybug-tearoom-tests/ppu/m3_wx_4_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);

mealybug_test!(
    ppu_m3_wx_4_change_sprites_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_wx_4_change_sprites.gb",
    "mealybug-tearoom-tests/ppu/m3_wx_4_change_sprites_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_wx_4_change_sprites_cgb_c,
    "mealybug-tearoom-tests/ppu/m3_wx_4_change_sprites.gb",
    "mealybug-tearoom-tests/ppu/m3_wx_4_change_sprites_cgb_c.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevC
);
mealybug_test!(
    ppu_m3_wx_4_change_sprites_cgb_d,
    "mealybug-tearoom-tests/ppu/m3_wx_4_change_sprites.gb",
    "mealybug-tearoom-tests/ppu/m3_wx_4_change_sprites_cgb_d.png",
    true,
    DmgRevision::default(),
    CgbRevision::RevD
);

mealybug_test!(
    ppu_m3_wx_5_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_wx_5_change.gb",
    "mealybug-tearoom-tests/ppu/m3_wx_5_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    ppu_m3_wx_6_change_dmg_blob,
    "mealybug-tearoom-tests/ppu/m3_wx_6_change.gb",
    "mealybug-tearoom-tests/ppu/m3_wx_6_change_dmg_blob.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);

// This ROM is part of the suite, but the bundle doesn't ship reference screenshots.
// Keep it present in the harness but ignored until a reference is available.
mealybug_test!(
    ppu_win_without_bg,
    "mealybug-tearoom-tests/ppu/win_without_bg.gb",
    "mealybug-tearoom-tests/ppu/win_without_bg.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);

// --- DMA/MBC subdirs (ROMs included but no reference screenshots in the bundle) ---
mealybug_test!(
    dma_hdma_during_halt_c,
    "mealybug-tearoom-tests/dma/hdma_during_halt-C.gb",
    "mealybug-tearoom-tests/dma/hdma_during_halt-C.png",
    true,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    dma_hdma_timing_c,
    "mealybug-tearoom-tests/dma/hdma_timing-C.gb",
    "mealybug-tearoom-tests/dma/hdma_timing-C.png",
    true,
    DmgRevision::default(),
    CgbRevision::default()
);
mealybug_test!(
    mbc_mbc3_rtc,
    "mealybug-tearoom-tests/mbc/mbc3_rtc.gb",
    "mealybug-tearoom-tests/mbc/mbc3_rtc.png",
    false,
    DmgRevision::default(),
    CgbRevision::default()
);
