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
    mmu.load_cart(Cartridge {
        rom: vec![0xBB; 0x200],
    });
    assert_eq!(mmu.read_byte(0x00), 0xAA);
    mmu.write_byte(0xFF50, 1);
    assert_eq!(mmu.read_byte(0x00), 0xBB);
}
