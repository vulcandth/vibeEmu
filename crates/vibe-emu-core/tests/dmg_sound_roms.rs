mod common;
use image::ImageReader;
use std::path::Path;
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

fn run_rom<P: AsRef<Path>, Q: AsRef<Path>>(rom_path: P, screenshot_path: Q) {
    let mut gb = GameBoy::new();
    let rom = std::fs::read(rom_path).expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    let mut frames = 0u32;
    while frames < 120 {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
        }
    }

    let expected = ImageReader::open(screenshot_path)
        .unwrap()
        .decode()
        .unwrap()
        .to_rgb8();
    assert_eq!(expected.width(), 160);
    assert_eq!(expected.height(), 144);

    let frame = gb.mmu.ppu.framebuffer();
    for (idx, pixel) in expected.pixels().enumerate() {
        let expected_color = match pixel.0 {
            [0xE0, 0xF8, 0xD0] => DMG_PALETTE[0],
            [0x08, 0x18, 0x20] => DMG_PALETTE[3],
            _ => panic!("unexpected color {:?}", pixel),
        };
        assert_eq!(frame[idx], expected_color, "pixel mismatch at index {idx}");
    }
}

fn run_single(name: &str) {
    let rom = common::roms_dir()
        .join("blargg/dmg_sound/rom_singles")
        .join(name);
    let screenshot_name = name.replace(' ', "_").replace(".gb", ".png");
    let screenshot = common::workspace_root()
        .join("extra_screenshots/blargg/dmg_sound/rom_singles")
        .join(screenshot_name);
    run_rom(rom, screenshot);
}

#[test]
fn dmg_sound_01_registers() {
    run_single("01-registers.gb");
}

#[test]
#[ignore]
fn dmg_sound_02_len_ctr() {
    run_single("02-len ctr.gb");
}

#[test]
#[ignore]
fn dmg_sound_03_trigger() {
    run_single("03-trigger.gb");
}

#[test]
#[ignore]
fn dmg_sound_04_sweep() {
    run_single("04-sweep.gb");
}

#[test]
#[ignore]
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
#[ignore]
fn dmg_sound_08_len_ctr_during_power() {
    run_single("08-len ctr during power.gb");
}

#[test]
#[ignore]
fn dmg_sound_09_wave_read_while_on() {
    run_single("09-wave read while on.gb");
}

#[test]
#[ignore]
fn dmg_sound_10_wave_trigger_while_on() {
    run_single("10-wave trigger while on.gb");
}

#[test]
#[ignore]
fn dmg_sound_11_regs_after_power() {
    run_single("11-regs after power.gb");
}

#[test]
#[ignore]
fn dmg_sound_12_wave_write_while_on() {
    run_single("12-wave write while on.gb");
}
