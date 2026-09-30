use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

#[test]
fn rom_edits_follow_both_mbc1_windows_without_writing_the_file() {
    let path = std::env::temp_dir().join(format!("vibeemu-debug-edit-{}.gb", std::process::id()));
    let mut rom = vec![0; 64 * 0x4000];
    rom[0x147] = 1; // MBC1
    rom[0x148] = 5;
    std::fs::write(&path, &rom).unwrap();
    let mut cart = Cartridge::from_file(&path).unwrap();
    cart.write(0x6000, 1);
    cart.write(0x4000, 1);
    cart.write(0x2000, 2);
    assert_eq!(cart.debug_patch_rom(0x100, 0x76), Some(0));
    assert_eq!(cart.debug_patch_rom(0x4100, 0xc9), Some(0));
    assert_eq!(cart.rom[32 * 0x4000 + 0x100], 0x76);
    assert_eq!(cart.rom[34 * 0x4000 + 0x100], 0xc9);
    assert_eq!(cart.read(0x4100), 0xc9);
    assert_eq!(cart.debug_patch_rom(0x8000, 1), None);
    assert_eq!(std::fs::read(&path).unwrap(), rom);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn debugger_edits_boot_rom_ram_and_video_without_advancing_time() {
    let mut gb = GameBoy::new(Model::Cgb(Default::default()));
    gb.load_cart(Cartridge::from_bytes(vec![0; 0x8000]));
    gb.mmu.load_boot_rom(vec![0; 0x900]);
    let cycles = gb.cpu.cycles;
    for (addr, value) in [
        (0, 0xaa),
        (0x200, 0xbb),
        (0x100, 0xcc),
        (0xc100, 0x42),
        (0xd123, 0x45),
        (0x8000, 0x12),
        (0xfe00, 0x34),
        (0xff80, 0x56),
    ] {
        gb.mmu.debug_write_byte(addr, value).unwrap();
        assert_eq!(gb.mmu.peek_byte(addr), value);
    }
    assert_eq!(gb.mmu.cart.as_ref().unwrap().rom[0], 0);
    assert_eq!(gb.mmu.cart.as_ref().unwrap().rom[0x100], 0xcc);
    assert_eq!(gb.mmu.peek_byte(0xe100), 0x42);
    assert_eq!(gb.mmu.peek_byte(0xf123), 0x45);
    assert!(gb.mmu.debug_write_byte(0xfea0, 0).is_err());
    assert_eq!(gb.cpu.cycles, cycles);
}
