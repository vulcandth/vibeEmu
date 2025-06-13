use vibeEmu::{cartridge::Cartridge, mmu::Mmu};

#[test]
fn wram_echo_and_bank_switch() {
    let mut mmu = Mmu::new();
    mmu.write_byte(0xC000, 0xAA);
    assert_eq!(mmu.read_byte(0xC000), 0xAA);
    mmu.write_byte(0xE000, 0xBB);
    assert_eq!(mmu.read_byte(0xC000), 0xBB);

    mmu.write_byte(0xFF70, 0x02);
    mmu.write_byte(0xD000, 0xCC);
    assert_eq!(mmu.read_byte(0xD000), 0xCC);

    mmu.write_byte(0xFF70, 0x03);
    assert_eq!(mmu.read_byte(0xD000), 0x00);
    mmu.write_byte(0xD000, 0xDD);
    assert_eq!(mmu.read_byte(0xD000), 0xDD);

    mmu.write_byte(0xFF70, 0x02);
    assert_eq!(mmu.read_byte(0xD000), 0xCC);
}

#[test]
fn vram_bank_switch() {
    let mut mmu = Mmu::new();
    mmu.write_byte(0x8000, 0x11);
    assert_eq!(mmu.read_byte(0x8000), 0x11);

    mmu.write_byte(0xFF4F, 0x01);
    assert_eq!(mmu.read_byte(0x8000), 0x00);
    mmu.write_byte(0x8000, 0x22);
    assert_eq!(mmu.read_byte(0x8000), 0x22);

    mmu.write_byte(0xFF4F, 0x00);
    assert_eq!(mmu.read_byte(0x8000), 0x11);
}

#[test]
fn boot_rom_disable() {
    let mut mmu = Mmu::new();
    mmu.load_boot_rom(vec![0xAA; 0x100]);
    mmu.load_cart(Cartridge::from_bytes_with_ram(vec![0xBB; 0x200], 0x2000));
    assert_eq!(mmu.read_byte(0x00), 0xAA);
    mmu.write_byte(0xFF50, 1);
    assert_eq!(mmu.read_byte(0x00), 0xBB);
}

#[test]
fn cartridge_ram_access() {
    let mut mmu = Mmu::new();
    mmu.load_cart(Cartridge::from_bytes_with_ram(vec![0; 0x200], 0x2000));

    mmu.write_byte(0xA000, 0x55);
    assert_eq!(mmu.read_byte(0xA000), 0x55);

    mmu.write_byte(0xBFFF, 0xAA);
    assert_eq!(mmu.read_byte(0xBFFF), 0xAA);
}

#[test]
fn mbc1_rom_bank_switching() {
    let mut rom = vec![0u8; 35 * 0x4000];
    rom[0x0147] = 0x01; // MBC1
    for i in 0..35 {
        rom[i * 0x4000] = i as u8;
    }

    let cart = Cartridge::load(rom);
    let mut mmu = Mmu::new();
    mmu.load_cart(cart);

    // default bank 1 at 0x4000
    assert_eq!(mmu.read_byte(0x4000), 1);

    mmu.write_byte(0x2000, 0x02); // select bank 2
    assert_eq!(mmu.read_byte(0x4000), 2);

    mmu.write_byte(0x4000, 0x01); // high bits 1 -> bank 0x22
    assert_eq!(mmu.read_byte(0x4000), 34);

    mmu.write_byte(0x6000, 0x01); // mode 1
    assert_eq!(mmu.read_byte(0x0000), 32);
}

#[test]
fn mbc1_ram_enable() {
    let mut rom = vec![0u8; 0x8000];
    rom[0x0147] = 0x03; // MBC1 + RAM + Battery
    rom[0x0149] = 0x03; // 32KB RAM
    let cart = Cartridge::from_bytes_with_ram(rom, 0x8000);

    let mut mmu = Mmu::new();
    mmu.load_cart(cart);

    mmu.write_byte(0xA000, 0x55);
    assert_eq!(mmu.read_byte(0xA000), 0xFF);

    mmu.write_byte(0x0000, 0x0A); // enable RAM
    mmu.write_byte(0xA000, 0x55);
    assert_eq!(mmu.read_byte(0xA000), 0x55);

    mmu.write_byte(0x0000, 0x00); // disable RAM
    assert_eq!(mmu.read_byte(0xA000), 0xFF);
}
