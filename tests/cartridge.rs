use std::fs;
use tempfile::tempdir;
use vibeEmu::cartridge::Cartridge;

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
