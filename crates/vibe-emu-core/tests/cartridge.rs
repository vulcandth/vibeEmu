use std::fs;
use tempfile::tempdir;
use vibe_emu_core::cartridge::{Cartridge, MbcType};

#[test]
fn battery_ram_saved_to_disk() {
    let dir = tempdir().unwrap();
    let rom_path = dir.path().join("game.gb");

    let mut rom = vec![0u8; 0x8000];
    rom[0x0147] = 0x03; // MBC1 + RAM + Battery
    rom[0x0149] = 0x03; // 32KB RAM
    fs::write(&rom_path, &rom).unwrap();

    let mut cart = Cartridge::from_file(&rom_path).unwrap();
    cart.ram[0] = 0xAA;
    cart.save_ram().unwrap();

    let save_path = rom_path.with_extension("sav");
    let data = fs::read(save_path).unwrap();
    assert_eq!(data[0], 0xAA);
}

#[test]
fn mbc30_header_detection() {
    let mut rom = vec![0u8; 0x8000];
    rom[0x0147] = 0x13; // MBC3 + RAM + Battery
    rom[0x0149] = 0x05; // 64KB RAM -> MBC30

    let cart = Cartridge::from_bytes(rom);
    assert_eq!(cart.mbc, MbcType::Mbc30);
}

#[test]
fn mbc3_rtc_state_roundtrips_to_disk() {
    let dir = tempdir().unwrap();
    let rom_path = dir.path().join("rtc.gb");

    let mut rom = vec![0u8; 0x8000];
    rom[0x0147] = 0x10; // MBC3 + Timer + RAM + Battery
    rom[0x0149] = 0x03; // 32KB RAM
    fs::write(&rom_path, &rom).unwrap();

    let mut cart = Cartridge::from_file(&rom_path).unwrap();
    cart.write(0x0000, 0x0A); // enable RAM/RTC
    cart.write(0x4000, 0x08); // seconds
    cart.write(0xA000, 12);
    cart.write(0x4000, 0x09); // minutes
    cart.write(0xA000, 34);
    cart.write(0x4000, 0x0C); // control
    cart.write(0xA000, 0x40); // halt so it doesn't advance between saves
    cart.save_ram().unwrap();

    let mut cart = Cartridge::from_file(&rom_path).unwrap();
    cart.write(0x0000, 0x0A);
    cart.write(0x6000, 0x00);
    cart.write(0x6000, 0x01); // latch

    cart.write(0x4000, 0x08);
    let seconds = cart.read(0xA000);
    cart.write(0x4000, 0x09);
    let minutes = cart.read(0xA000);
    cart.write(0x4000, 0x0C);
    let control = cart.read(0xA000);

    assert_eq!(seconds, 12);
    assert_eq!(minutes, 34);
    assert_eq!(control & 0x40, 0x40);
}
