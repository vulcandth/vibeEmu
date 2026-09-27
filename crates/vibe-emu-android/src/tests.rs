use super::*;

#[path = "../../vibe-emu-core/tests/common/sgb_rom.rs"]
mod sgb_rom;

fn rom(cgb: bool, sgb: bool) -> Vec<u8> {
    let mut rom = vec![0; 0x8000];
    rom[0x100..0x102].copy_from_slice(&[0x18, 0xfe]);
    rom[0x143] = if cgb { 0x80 } else { 0 };
    rom[0x146] = if sgb { 3 } else { 0 };
    rom[0x14b] = 0x33;
    rom
}

#[test]
fn model_selection_preserves_ids_and_checks_both_header_flags() {
    let dmg = Model::from_cgb_flag(false);
    let cgb = Model::from_cgb_flag(true);
    for (id, model) in [
        (1, dmg),
        (2, cgb),
        (3, Model::Mgb),
        (4, Model::Sgb),
        (5, Model::Sgb2),
        (6, Model::Agb0),
        (7, Model::Agb),
        (8, cgb),
        (9, cgb),
    ] {
        assert_eq!(EmulationMode::from_jint(id).model(&[]), model);
    }
    for color in [false, true] {
        for enhanced in [false, true] {
            let mut rom = rom(color, enhanced);
            let native = Model::from_cgb_flag(color);
            assert_eq!(EmulationMode::from_jint(0).model(&rom), native);
            assert_eq!(
                EmulationMode::from_jint(10).model(&rom),
                if enhanced && !color {
                    Model::Sgb
                } else {
                    native
                }
            );
            assert_eq!(
                EmulationMode::from_jint(11).model(&rom),
                if enhanced { Model::Sgb } else { native }
            );
            rom[0x14b] = 0;
            assert_eq!(EmulationMode::AutoPreferSgb.model(&rom), native);
        }
    }
    assert_eq!(EmulationMode::from_jint(-1), EmulationMode::Auto);
}

#[test]
fn every_model_uses_its_own_validated_boot_slot() {
    for id in 1..=7 {
        let mut handle = EmulatorHandle::new(EmulationMode::from_jint(id));
        let model = handle.emulation_mode.model(&[]);
        let slot = bootrom_index(model);
        let size = if model.is_cgb() { 0x800 } else { 0x100 };
        let mut boot = vec![id as u8; size];
        boot[..3].copy_from_slice(&[0xc3, 0xfc, 0]);
        boot[252..256].copy_from_slice(&[0x3e, 1, 0xe0, 0x50]);
        assert!(handle.set_boot_rom(slot, boot));
        assert!(!handle.set_boot_rom(slot, vec![0; 123]));
        if model.is_cgb() {
            let expanded = handle.bootroms[slot].as_ref().unwrap();
            assert_eq!(expanded.len(), 0x900);
            assert_eq!(&expanded[0x100..0x200], &[0; 0x100]);
            assert_eq!(expanded[0x200], id as u8);
        }
        assert!(handle.load_rom(rom(model.is_cgb(), model.is_sgb())));
        assert_eq!(handle.gb.model, model);
        assert_eq!(handle.gb.cpu.pc, 0);
        assert_eq!(handle.gb.mmu.read_byte(4), id as u8);
        assert!(handle.run_frame());
        handle.reset();
        assert_eq!(handle.gb.cpu.pc, 0);
        handle.clear_boot_rom(slot);
        assert!(handle.load_rom(rom(model.is_cgb(), model.is_sgb())));
        assert_eq!(handle.gb.cpu.pc, 0x100);
    }
}

#[test]
fn native_frames_preserve_sgb_viewport_when_border_hidden() {
    for mode in [
        EmulationMode::ForceSgb,
        EmulationMode::ForceSgb2,
        EmulationMode::CgbSgb,
    ] {
        let mut handle = EmulatorHandle::new(mode);
        assert!(handle.load_rom(rom(mode == EmulationMode::CgbSgb, true)));
        assert!(handle.run_frame());
        assert_eq!(handle.dimensions(), (256, 224));
        let full = handle.frame.clone();
        handle.show_border = false;
        handle.copy_frame();
        assert_eq!(handle.dimensions(), (160, 144));
        assert_eq!(handle.frame.len(), FB_PIXELS);
        for y in 0..144 {
            assert_eq!(
                &handle.frame[y * 160..(y + 1) * 160],
                &full[(y + 40) * 256 + 48..(y + 40) * 256 + 208]
            );
        }
        for player in 0..4 {
            handle.set_player_input(player, !(1 << player));
        }
        handle.gb.mmu.input.write(0x20);
        for player in 0..4 {
            assert_eq!(
                handle.gb.mmu.input.read_player(player) & 15,
                15 ^ (1 << player)
            );
        }
    }
}

#[test]
fn initial_border_uses_separate_boots_and_survives_android_reset() {
    let mut handle = EmulatorHandle::new(EmulationMode::CgbInitialBorder);
    let mut sgb_boot = vec![0; 256];
    sgb_boot[..3].copy_from_slice(&[0xc3, 0xfc, 0]);
    sgb_boot[252..].copy_from_slice(&[0x3e, 1, 0xe0, 0x50]);
    assert!(handle.set_boot_rom(3, sgb_boot));
    let mut cgb_boot = vec![0; 0x900];
    cgb_boot[..3].copy_from_slice(&[0xc3, 0xfc, 0]);
    cgb_boot[252..256].copy_from_slice(&[0x3e, 0x11, 0xe0, 0x50]);
    assert!(handle.set_boot_rom(1, cgb_boot));
    let mut cart = Cartridge::from_bytes(sgb_rom::initial_border_rom());
    cart.ram[0] = 0x57;
    assert!(handle.load_cart(cart));
    assert_eq!(handle.dimensions(), (256, 224));
    assert_eq!(handle.frame[0], 0x00ff00);
    assert_eq!(handle.gb.cpu.pc, 0);
    assert!(!handle.gb.mmu.ppu.sgb.as_ref().unwrap().is_command_host());
    assert_eq!(handle.gb.mmu.cart.as_ref().unwrap().ram[0], 0x57);
    assert!(handle.run_frame());
    assert_eq!(handle.gb.mmu.wram[0][0], 0x42);
    handle.reset();
    assert_eq!(handle.gb.cpu.pc, 0);
    assert_eq!(handle.frame[0], 0x00ff00);
    assert!(handle.run_frame());
    assert_eq!(handle.gb.mmu.wram[0][0], 0x42);
}

#[test]
fn faulty_rom_returns_without_hanging_android_frame_loop() {
    let mut handle = EmulatorHandle::new(EmulationMode::ForceDmg);
    let mut rom = rom(false, false);
    rom[0x100] = 0xd3;
    handle.load_rom(rom);
    assert!(!handle.run_frame());
}
