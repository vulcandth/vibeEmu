mod common;

use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

fn run_for_frames(gb: &mut GameBoy, frames: u32) {
    let mut seen = 0u32;
    while seen < frames {
        gb.cpu.step(&mut gb.mmu);

        // In DMG mode, STOP halts the whole system in this emulator model, so
        // we'll never see another completed frame after STOP executes.
        if gb.cpu.stopped && !gb.mmu.is_cgb() {
            break;
        }

        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            seen += 1;
        }
    }
}

fn assert_framebuffer_matches_png(gb: &GameBoy, png_relative_path: &str) {
    let (width, height, expected) = common::load_png_rgb(common::rom_path(png_relative_path));
    assert_eq!(width, 160);
    assert_eq!(height, 144);

    let frame = gb.mmu.ppu.framebuffer();

    // Save actual output for debugging
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test_roms")
        .join("actual_output.png");
    let mut pixels = Vec::with_capacity(160 * 144 * 3);
    for c in frame.iter() {
        pixels.push(((c >> 16) & 0xFF) as u8);
        pixels.push(((c >> 8) & 0xFF) as u8);
        pixels.push((c & 0xFF) as u8);
    }
    if let Ok(file) = std::fs::File::create(&path) {
        let mut encoder = png::Encoder::new(file, 160, 144);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        if let Ok(mut writer) = encoder.write_header() {
            writer.write_image_data(&pixels).ok();
        }
    }

    for (idx, pixel) in expected.iter().enumerate() {
        let &[r, g, b] = pixel;
        let expected_color = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        assert_eq!(frame[idx], expected_color, "pixel mismatch at index {idx}");
    }
}

#[test]
fn daid_speed_switch_timing_div() {
    // Validates correctness by comparing the framebuffer against the reference PNG
    // from GBEmulatorShootout.
    let mut gb = GameBoy::new_with_mode(true);
    let rom =
        std::fs::read(common::rom_path("daid/speed_switch_timing_div.gbc")).expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    run_for_frames(&mut gb, 120);
    assert_framebuffer_matches_png(&gb, "daid/speed_switch_timing_div.png");
}

#[test]
fn daid_speed_switch_timing_ly() {
    // The ROM samples `rLY` 128 times after a speed switch and stores results into
    // WRAM0 at $C000..$C07F. Validate that buffer against Daid's published `expect`
    // table from the upstream ASM.
    let mut gb = GameBoy::new_with_mode(true);
    let rom =
        std::fs::read(common::rom_path("daid/speed_switch_timing_ly.gbc")).expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    const EXPECT: [u8; 128] = [
        0x85, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86,
        0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86,
        0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86, 0x86,
        0x86, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87,
        0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87,
        0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87, 0x87,
        0x87, 0x87, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88,
        0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88,
        0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88,
    ];

    run_for_frames(&mut gb, 120);

    let actual = &gb.mmu.wram[0][0..EXPECT.len()];
    if let Some((idx, (&got, &exp))) = actual
        .iter()
        .zip(EXPECT.iter())
        .enumerate()
        .find(|(_, (got, exp))| got != exp)
    {
        let window_start = idx.saturating_sub(8);
        let window_end = (idx + 9).min(EXPECT.len());
        panic!(
            "speed_switch_timing_ly mismatch at sample {idx}: got 0x{got:02X}, expected 0x{exp:02X}. window[{window_start}..{window_end}]: got={:?} expected={:?}",
            &actual[window_start..window_end],
            &EXPECT[window_start..window_end]
        );
    }
}

#[test]
fn daid_speed_switch_timing_stat() {
    let mut gb = GameBoy::new_with_mode(true);
    let rom = std::fs::read(common::rom_path("daid/speed_switch_timing_stat.gbc"))
        .expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    const EXPECT: [u8; 64] = [
        0x80, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x83, 0x83, 0x83, 0x83, 0x83, 0x83,
        0x83, 0x83, 0x83, 0x83, 0x83, 0x83, 0x83, 0x83, 0x83, 0x83, 0x83, 0x80, 0x80, 0x80, 0x80,
        0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
        0x80, 0x80, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x83, 0x83, 0x83, 0x83, 0x83,
        0x83, 0x83, 0x83, 0x83,
    ];

    run_for_frames(&mut gb, 120);

    let actual = &gb.mmu.wram[0][0..EXPECT.len()];
    if let Some((idx, (&got, &exp))) = actual
        .iter()
        .zip(EXPECT.iter())
        .enumerate()
        .find(|(_, (got, exp))| got != exp)
    {
        let window_start = idx.saturating_sub(8);
        let window_end = (idx + 9).min(EXPECT.len());
        panic!(
            "speed_switch_timing_stat mismatch at sample {idx}: got 0x{got:02X}, expected 0x{exp:02X}. window[{window_start}..{window_end}]: got={:?} expected={:?}",
            &actual[window_start..window_end],
            &EXPECT[window_start..window_end]
        );
    }
}

#[test]
fn daid_stop_instr_dmg() {
    let mut gb = GameBoy::new_with_mode(false);
    let rom = std::fs::read(common::rom_path("daid/stop_instr.gb")).expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    // The ROM prints a failure message and then executes STOP with the LCD on.
    // If STOP returns, it will print "STOP not stopping..." (failure).
    // In DMG mode, validate that we do enter STOP and remain stopped.
    let mut steps = 0u64;
    while !gb.cpu.stopped && steps < 5_000_000 {
        gb.cpu.step(&mut gb.mmu);
        steps += 1;
    }
    assert!(gb.cpu.stopped, "CPU never entered STOP in DMG mode");

    let stopped_pc = gb.cpu.pc;
    for _ in 0..10_000 {
        gb.cpu.step(&mut gb.mmu);
    }
    assert!(gb.cpu.stopped, "CPU unexpectedly left STOP in DMG mode");
    assert_eq!(gb.cpu.pc, stopped_pc, "PC changed while stopped");
}

#[test]
fn daid_stop_instr_cgb() {
    // In CGB mode, STOP should keep the PPU running but prevent it from accessing
    // VRAM, resulting in a black screen.
    let mut gb = GameBoy::new_with_mode(true);
    let rom = std::fs::read(common::rom_path("daid/stop_instr.gb")).expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    run_for_frames(&mut gb, 120);
    assert_framebuffer_matches_png(&gb, "daid/stop_instr.gbc.png");
}

#[test]
fn daid_stop_instr_cgb_mode3() {
    // STOP during mode 3 on CGB should keep the already-displayed frame stable
    // (the PPU continues running and can access VRAM during mode 3).
    let mut gb = GameBoy::new_with_mode(true);
    let rom =
        std::fs::read(common::rom_path("daid/stop_instr_gbc_mode3.gb")).expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    run_for_frames(&mut gb, 120);
    assert_framebuffer_matches_png(&gb, "daid/stop_instr_gbc_mode3.png");
}

#[test]
fn daid_ppu_scanline_bgp_gbc() {
    // Mid-scanline BGP changes should take effect immediately on GBC in DMG-compat mode.
    // The test ROM writes different BGP values during mode 3 to create colored bands.
    let rom = std::fs::read(common::rom_path("daid/ppu_scanline_bgp.gb")).expect("rom not found");

    let mut gb = GameBoy::new_with_mode(true);
    gb.mmu.load_cart(Cartridge::load(rom));

    run_for_frames(&mut gb, 60);
    assert_framebuffer_matches_png(&gb, "daid/ppu_scanline_bgp.gbc.png");
}

#[test]
fn daid_ppu_scanline_bgp_dmg() {
    // Mid-scanline BGP changes on DMG. The ROM writes different BGP values
    // during mode 3 to create colored bands.
    let rom = std::fs::read(common::rom_path("daid/ppu_scanline_bgp.gb")).expect("rom not found");

    let mut gb = GameBoy::new_with_mode(false);
    gb.mmu.load_cart(Cartridge::load(rom));

    // Use grayscale palette to match the reference screenshots
    gb.mmu
        .ppu
        .set_dmg_palette([0xFFFFFF, 0xAAAAAA, 0x555555, 0x000000]);

    run_for_frames(&mut gb, 60);
    assert_framebuffer_matches_png(&gb, "daid/ppu_scanline_bgp_0.dmg.png");
}
