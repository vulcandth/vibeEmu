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
fn latch_rtc_png() {
    let mut gb = GameBoy::new(Model::default());

    let rom = std::fs::read(common::rom_path("gbeshootout/cpp/latch-rtc-test.gb"))
        .expect("rom not found");
    gb.mmu.load_cart(Cartridge::from_bytes(rom));

    run_for_frames(&mut gb, 600);

    let (width, height, expected) =
        common::load_png_rgb(common::rom_path("gbeshootout/cpp/latch-rtc-test.png"));
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
            "latch-rtc-test mismatch: {mismatches} pixels differ (bbox x={min_x}..={max_x}, y={min_y}..={max_y})"
        );

        let actual_path = std::path::Path::new("target/tmp/latch-rtc-test.actual.png");
        write_png_rgb(actual_path, 160, 144, &frame_to_rgb(frame));

        let expected_path = std::path::Path::new("target/tmp/latch-rtc-test.expected.png");
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
        let diff_path = std::path::Path::new("target/tmp/latch-rtc-test.diff.png");
        let diff_flat: Vec<u8> = diff.into_iter().flatten().collect();
        write_png_rgb(diff_path, 160, 144, &diff_flat);

        println!(
            "serial={} ",
            String::from_utf8_lossy(gb.mmu.serial.peek_output())
        );
    }

    assert_eq!(mismatches, 0, "latch-rtc-test PNG mismatch");
}
