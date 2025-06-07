// src/ppu_tests.rs
// Unit tests for PPU logic will go here.

// Allow dead code for helper functions that might not be used in every test.
#![allow(dead_code)]

use crate::ppu::{Ppu, MODE_HBLANK, MODE_VBLANK, MODE_OAM_SCAN, MODE_DRAWING, DMG_PALETTE, COLOR_WHITE, SCANLINE_CYCLES, OAM_SCAN_CYCLES, DRAWING_CYCLES, HBLANK_CYCLES, LINES_PER_FRAME};
use crate::bus::SystemMode;
use crate::interrupts::InterruptType;

// Helper to create a PPU instance, optionally with a specific SystemMode
fn setup_ppu(system_mode_override: Option<SystemMode>) -> Ppu {
    let mode = system_mode_override.unwrap_or(SystemMode::DMG);
    Ppu::new(mode)
}

fn setup_ppu_dmg() -> Ppu { Ppu::new(SystemMode::DMG) }
fn setup_ppu_cgb() -> Ppu { Ppu::new(SystemMode::CGB) }

fn compare_pixel(fb: &[u8], x: usize, y: usize, width: usize, expected_rgb: &[u8; 3], message: &str) {
    let idx = (y * width + x) * 3;
    assert!(idx + 2 < fb.len(), "Framebuffer index out of bounds. Fb len: {}, idx+2: {}. At ({},{}): {}", fb.len(), idx+2, x, y, message);
    let pixel = &fb[idx..idx+3];
    assert_eq!(pixel, expected_rgb, "Pixel at ({},{}): {}. Expected {:?}, got {:?}", x, y, message, expected_rgb, pixel);
}

#[test]
fn test_ppu_initialization_values() {
    let ppu_dmg = setup_ppu_dmg();
    assert_eq!(ppu_dmg.lcdc, 0x91, "Initial LCDC");
    assert_eq!(ppu_dmg.stat & 0b0111_1100, MODE_OAM_SCAN & 0b0111_1100, "Initial STAT mode (ignoring LYC and bit 7)");
    assert_eq!(ppu_dmg.ly, 0, "Initial LY");
    assert_eq!(ppu_dmg.bgp, 0xFC, "Initial BGP");
    assert_eq!(ppu_dmg.system_mode, SystemMode::DMG, "System mode should be DMG");
    if ppu_dmg.ly == ppu_dmg.lyc { assert_ne!(ppu_dmg.stat & (1 << 2), 0, "LYC=LY flag if ly==lyc"); }

    let ppu_cgb = setup_ppu_cgb();
    assert_eq!(ppu_cgb.system_mode, SystemMode::CGB, "System mode should be CGB");
    assert_eq!(ppu_cgb.vbk, 0, "Initial VBK for CGB");
    assert_eq!(ppu_cgb.bcps, 0, "Initial BCPS for CGB");
    assert_eq!(ppu_cgb.ocps, 0, "Initial OCPS for CGB");
    assert!(ppu_cgb.cgb_bg_palette_ram.iter().all(|&x| x == 0xFF), "Initial CGB BG Palette RAM");
    assert!(ppu_cgb.cgb_obj_palette_ram.iter().all(|&x| x == 0xFF), "Initial CGB OBJ Palette RAM");
}

#[test]
fn test_lcdc_display_enable_master() {
    let mut ppu = setup_ppu_dmg();
    assert_ne!(ppu.lcdc & (1 << 7), 0, "LCD enabled initially");
    ppu.tick(SCANLINE_CYCLES * 5);

    ppu.write_byte(0xFF40, ppu.lcdc & !(1 << 7)); // Turn off LCDC bit 7
    assert_eq!(ppu.lcdc & (1 << 7), 0, "LCDC bit 7 off");
    ppu.tick(10);
    assert_eq!(ppu.ly, 0, "LY should reset to 0 when LCD disabled");
    assert_eq!(ppu.stat & 0b11, MODE_VBLANK, "PPU mode should be VBLANK when LCD disabled");

    ppu.write_byte(0xFF40, ppu.lcdc | (1 << 7)); // Turn on LCDC bit 7
    assert_ne!(ppu.lcdc & (1 << 7), 0, "LCDC bit 7 on");
    ppu.tick(SCANLINE_CYCLES + 1);
    assert_ne!(ppu.ly, 0, "LY should advance after re-enabling LCD");
}

#[test]
fn test_lcdc_bg_window_enable_dmg() {
    let mut ppu = setup_ppu_dmg();
    ppu.ly = 10;
    for i in 0..16 { ppu.vram[0][i] = 0xFF; } // Tile 0 is all color 3 (black with default BGP)
    ppu.vram[0][(0x9800 - 0x8000)] = 0;
    ppu.bgp = 0b11100100; // 0->0, 1->1, 2->2, 3->3

    ppu.write_byte(0xFF40, ppu.lcdc | (1 << 0)); // BG/Win ON
    ppu.render_scanline();
    compare_pixel(&ppu.framebuffer, 0, 10, 160, &DMG_PALETTE[3], "BG render if LCDC.0 on");

    ppu.write_byte(0xFF40, ppu.lcdc & !(1 << 0)); // BG/Win OFF for DMG
    ppu.render_scanline();
    let bgp_col0_idx = (ppu.bgp >> 0) & 0b11;
    compare_pixel(&ppu.framebuffer, 0, 10, 160, &DMG_PALETTE[bgp_col0_idx as usize], "BG is BGP color 0 if LCDC.0 off (DMG)");

    ppu.write_byte(0xFF40, (ppu.lcdc & !(1 << 0)) | (1 << 1)); // BG off, OBJ on
    ppu.oam[0] = 26; ppu.oam[1] = 8; ppu.oam[2] = 0; ppu.oam[3] = 0;
    ppu.obp0 = 0b11100100;
    // Tile 0 already set to all black (0xFF, 0xFF)
    ppu.render_scanline();
    compare_pixel(&ppu.framebuffer, 5, 10, 160, &DMG_PALETTE[bgp_col0_idx as usize], "BG still BGP color 0 with OBJ enabled");
    compare_pixel(&ppu.framebuffer, 0, 10, 160, &DMG_PALETTE[3], "Sprite renders if LCDC.0 off (DMG)");
}

#[test]
fn test_vram_oam_access_restrictions() {
    let mut ppu = setup_ppu_dmg();
    let vram_addr = 0x8000; let oam_addr = 0xFE00;

    ppu.stat = (ppu.stat & !0b11) | MODE_DRAWING;
    assert_eq!(ppu.read_byte(vram_addr), 0xFF, "VRAM read restricted Mode 3");
    ppu.write_byte(vram_addr, 0xAA); assert_ne!(ppu.vram[0][0], 0xAA, "VRAM write restricted Mode 3");

    ppu.stat = (ppu.stat & !0b11) | MODE_HBLANK;
    ppu.vram[0][0] = 0xBB; assert_eq!(ppu.read_byte(vram_addr), 0xBB, "VRAM read Mode 0");
    ppu.write_byte(vram_addr, 0xCC); assert_eq!(ppu.vram[0][0], 0xCC, "VRAM write Mode 0");

    ppu.stat = (ppu.stat & !0b11) | MODE_OAM_SCAN;
    assert_eq!(ppu.read_byte(oam_addr), 0xFF, "OAM read restricted Mode 2");
    ppu.write_byte(oam_addr, 0xAA); assert_ne!(ppu.oam[0], 0xAA, "OAM write restricted Mode 2");

    ppu.stat = (ppu.stat & !0b11) | MODE_VBLANK;
    ppu.oam[0] = 0xCC; assert_eq!(ppu.read_byte(oam_addr), 0xCC, "OAM read Mode 1");
    ppu.write_byte(oam_addr, 0xDD); assert_eq!(ppu.oam[0], 0xDD, "OAM write Mode 1");
}

#[test]
fn test_scanline_cycle_modes() {
    let mut ppu = setup_ppu_dmg();
    ppu.write_byte(0xFF40, 0x91);

    ppu.ly = 0; ppu.cycles = 0; ppu.set_mode(MODE_OAM_SCAN); // Start of scanline
    assert_eq!(ppu.stat & 0b11, MODE_OAM_SCAN);
    ppu.tick(OAM_SCAN_CYCLES); assert_eq!(ppu.stat & 0b11, MODE_DRAWING);
    ppu.tick(DRAWING_CYCLES); assert_eq!(ppu.stat & 0b11, MODE_HBLANK);
    assert_eq!(ppu.ly, 0);
    ppu.tick(HBLANK_CYCLES);
    assert_eq!(ppu.ly, 1); assert_eq!(ppu.stat & 0b11, MODE_OAM_SCAN);
}

#[test]
fn test_lyc_ly_interrupt() {
    let mut ppu = setup_ppu_dmg();
    ppu.write_byte(0xFF40, 0x80); // LCD On, other things off for simplicity if needed
    ppu.lyc = 5;
    ppu.stat |= 1 << 6; // Enable LYC=LY STAT interrupt

    let mut interrupt: Option<InterruptType> = None;
    for i in 0..5 { // Tick up to LY = 4
        ppu.ly = i; ppu.cycles = 0;
        interrupt = ppu.tick(SCANLINE_CYCLES);
        assert!((ppu.stat & (1 << 2)) == 0, "LYC flag should not be set for LY={}", i);
        if let Some(InterruptType::LcdStat) = interrupt { panic!("Unexpected STAT int for LY={}", i); }
    }

    ppu.ly = 5; ppu.cycles = 0; // LY becomes 5
    // Manually call check_lyc_ly as tick() structure might call it at different times
    // Or simulate the tick part that calls it
    let _ = ppu.check_lyc_ly(); // Call it directly after setting LY
    assert_ne!(ppu.stat & (1 << 2), 0, "LYC flag should be set for LY=5, LYC=5");

    // The interrupt is generated if STAT bit 6 is enabled AND LYC=LY flag (STAT bit 2) is set.
    // The check_lyc_ly itself returns the interrupt type.
    // Resetting LY and ticking one full line.
    ppu.ly = 4; ppu.cycles = 0; ppu.tick(SCANLINE_CYCLES); // LY becomes 5
    interrupt = ppu.tick(1); // LY=5, check for interrupt generated on this boundary.
                             // This is tricky as tick() handles many things.
                             // A more direct test of `check_lyc_ly` and `set_mode` might be better.

    // Test check_lyc_ly directly
    ppu.ly = 5; ppu.lyc = 5; ppu.stat |= (1 << 6); // Enable LYC interrupt
    assert_eq!(ppu.check_lyc_ly(), Some(InterruptType::LcdStat), "check_lyc_ly should signal STAT interrupt");
    assert_ne!(ppu.stat & (1 << 2), 0, "LYC flag should be set");

    ppu.ly = 6;
    assert_eq!(ppu.check_lyc_ly(), None, "check_lyc_ly should not signal if LY != LYC");
    assert_eq!(ppu.stat & (1 << 2), 0, "LYC flag should be clear");
}


#[test]
fn test_stat_mode_interrupts() {
    let mut ppu = setup_ppu_dmg();
    ppu.write_byte(0xFF40, 0x91); // LCD On
    ppu.ly = 0; ppu.cycles = 0;

    // Test Mode 2 (OAM Scan) Interrupt
    ppu.stat = (ppu.stat & !(0b111 << 3)) | (1 << 5); // Enable Mode 2 STAT interrupt, disable others
    assert_eq!(ppu.set_mode(MODE_OAM_SCAN), Some(InterruptType::LcdStat));

    // Test Mode 0 (HBLANK) Interrupt
    ppu.stat = (ppu.stat & !(0b111 << 3)) | (1 << 3); // Enable Mode 0 STAT interrupt
    assert_eq!(ppu.set_mode(MODE_HBLANK), Some(InterruptType::LcdStat));

    // Test Mode 1 (VBLANK) Interrupt
    ppu.stat = (ppu.stat & !(0b111 << 3)) | (1 << 4); // Enable Mode 1 STAT interrupt
    assert_eq!(ppu.set_mode(MODE_VBLANK), Some(InterruptType::LcdStat));

    // Test no interrupt if specific bit not set
    ppu.stat &= !(0b111 << 3); // Disable all mode STAT interrupts
    assert_eq!(ppu.set_mode(MODE_HBLANK), None);
}

#[test]
fn test_sprite_flips_dmg() {
    let mut ppu = setup_ppu_dmg();
    ppu.write_byte(0xFF40, 0x93); // LCD On, BG On (dummy), Sprites On
    ppu.ly = 10;
    ppu.oam[0] = 10 + 16; // Sprite Y
    ppu.oam[1] = 8;       // Sprite X
    ppu.oam[2] = 0;       // Tile index 0
    // VRAM Tile 0: Asymmetric pattern to detect flips
    // Row 0: 0b10000000, 0b00000000 => First pixel is color 1 (Light Gray with default OBP0)
    // Row 7: 0b00000001, 0b00000000 => Last pixel is color 1
    ppu.vram[0][0] = 0x80; ppu.vram[0][1] = 0x00; // Row 0
    for i in 2..14 { ppu.vram[0][i] = 0x00; }    // Middle rows blank
    ppu.vram[0][14] = 0x01; ppu.vram[0][15] = 0x00; // Row 7
    ppu.obp0 = 0xE4; // Default: 00,01,10,11 -> 0,1,2,3

    // No flips
    ppu.oam[3] = 0x00; // No flips, OBP0
    ppu.render_scanline();
    compare_pixel(&ppu.framebuffer, 0, 10, 160, &DMG_PALETTE[1], "No flip, first pixel");

    // X-flip
    ppu.oam[3] = 1 << 5; // X-flip
    ppu.render_scanline();
    compare_pixel(&ppu.framebuffer, 7, 10, 160, &DMG_PALETTE[1], "X-flip, last pixel becomes first");

    // Y-flip (testing row 0 of sprite, which should now fetch data from tile row 7)
    ppu.oam[3] = 1 << 6; // Y-flip
    ppu.render_scanline(); // ly=10, sprite_y_top=10, scanline_row_in_sprite=0. Flipped: 7-0=7.
    compare_pixel(&ppu.framebuffer, 7, 10, 160, &DMG_PALETTE[1], "Y-flip, row 0 uses tile data from row 7, last pixel");

    // X and Y flip
    ppu.oam[3] = (1 << 5) | (1 << 6); // X and Y flip
    ppu.render_scanline();
    compare_pixel(&ppu.framebuffer, 0, 10, 160, &DMG_PALETTE[1], "X & Y-flip, row 0 uses tile data from row 7, first pixel of that row");
}


#[test]
fn test_cgb_palette_io_access() {
    let mut ppu = setup_ppu_cgb();

    // Test BCPS/BCPD
    ppu.write_byte(0xFF68, 0x05); // Select BG palette 0, byte 5
    assert_eq!(ppu.bcps, 0x05);
    ppu.write_byte(0xFF69, 0xAB); // Write to data register
    assert_eq!(ppu.cgb_bg_palette_ram[5], 0xAB);
    assert_eq!(ppu.bcps, 0x05, "BCPS should not auto-increment if bit 7 is not set");

    ppu.write_byte(0xFF68, 0x85); // Select BG palette 0, byte 5, auto-increment enabled
    assert_eq!(ppu.bcps, 0x85);
    ppu.write_byte(0xFF69, 0xCD); // Write to data register
    assert_eq!(ppu.cgb_bg_palette_ram[5], 0xCD);
    assert_eq!(ppu.bcps, 0x80 | ((5 + 1) & 0x3F), "BCPS should auto-increment");
    assert_eq!(ppu.read_byte(0xFF69), ppu.cgb_bg_palette_ram[6], "Reading BCPD after increment");

    // Test OCPS/OCPD
    ppu.write_byte(0xFF6A, 0x8A); // Select OBJ palette 1, byte 2 (index 0xA), auto-increment
    assert_eq!(ppu.ocps, 0x8A);
    ppu.write_byte(0xFF6B, 0xEF);
    assert_eq!(ppu.cgb_obj_palette_ram[0xA], 0xEF);
    assert_eq!(ppu.ocps, 0x80 | ((0xA + 1) & 0x3F), "OCPS should auto-increment");
    assert_eq!(ppu.read_byte(0xFF6B), ppu.cgb_obj_palette_ram[0xB], "Reading OCPD after increment");
}

// Note: Full rendering tests for CGB (BG attributes, sprite attributes, VRAM banking in render)
// are more complex and would build upon these.
// This set of tests provides a good starting point for PPU core functionalities.
