#[path = "common/sgb_rom.rs"]
mod sgb_rom;

use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision, Model},
    save_state,
    state_store::{Slot, StateStore},
};

fn machine(model: Model, mapper: u8, ram: u8) -> GameBoy {
    let mut rom = vec![0; 0x10000];
    rom[0x100..0x103].copy_from_slice(&[0xc3, 0x60, 0x01]);
    rom[0x160..0x168].copy_from_slice(&[0x3c, 0xea, 0x00, 0xc0, 0xc3, 0x60, 0x01, 0]);
    rom[0x143] = 0x80;
    rom[0x146] = 3;
    rom[0x14b] = 0x33;
    rom[0x147] = mapper;
    rom[0x149] = ram;
    if mapper == 0xbc {
        rom[0x149] = 0xc1;
        rom[0x14a] = 0x65;
        rom[0x150] = 1;
        rom[0x151] = 0;
        rom[0x152] = 3;
        rom[0x153] = 0x0c;
    }
    let mut gb = GameBoy::new(model);
    gb.load_cart(Cartridge::from_bytes(rom));
    gb
}

fn run(gb: &mut GameBoy, count: usize) {
    for _ in 0..count {
        gb.cpu.step(&mut gb.mmu);
    }
}

#[test]
fn every_model_and_mapper_roundtrips_and_continues_deterministically() {
    let models = [
        Model::Dmg(DmgRevision::Rev0),
        Model::Dmg(DmgRevision::RevA),
        Model::Dmg(DmgRevision::RevB),
        Model::Dmg(DmgRevision::RevC),
        Model::Mgb,
        Model::Cgb(CgbRevision::Rev0),
        Model::Cgb(CgbRevision::RevA),
        Model::Cgb(CgbRevision::RevB),
        Model::Cgb(CgbRevision::RevC),
        Model::Cgb(CgbRevision::RevD),
        Model::Cgb(CgbRevision::RevE),
        Model::Agb0,
        Model::Agb,
        Model::Sgb,
        Model::Sgb2,
    ];
    for model in models {
        for (mapper, ram) in [
            (0, 0),
            (3, 3),
            (6, 0),
            (0x10, 3),
            (0x10, 4),
            (0x1b, 4),
            (0xbc, 3),
            (0xff, 0),
        ] {
            let mut gb = machine(model, mapper, ram);
            run(&mut gb, 77);
            let bytes = gb.save_state().unwrap();
            let mut restored = machine(model, mapper, ram);
            restored
                .load_state(&bytes)
                .unwrap_or_else(|e| panic!("{model:?}/{mapper:x}: {e}"));
            run(&mut gb, 12_000);
            run(&mut restored, 12_000);
            // Ignore wall time in metadata; compare every serialized machine field.
            let state = |g: &GameBoy| -> serde_json::Value {
                serde_json::from_slice(&g.save_state().unwrap()[44..]).unwrap()
            };
            assert_eq!(
                state(&gb)["machine"],
                state(&restored)["machine"],
                "{model:?}/{mapper:x}"
            );
        }
    }
}

#[test]
fn load_restores_all_sram_and_recovery_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut gb = machine(Model::Cgb(CgbRevision::RevE), 0x1b, 4);
    gb.mmu
        .cart
        .as_mut()
        .unwrap()
        .ram
        .iter_mut()
        .enumerate()
        .for_each(|(i, b)| *b = (i / 0x2000) as u8);
    let saved = gb.mmu.cart.as_ref().unwrap().ram.clone();
    let store = StateStore::new(dir.path(), &gb).unwrap();
    store.save(&gb, Slot::Quick).unwrap();
    gb.mmu.cart.as_mut().unwrap().ram.fill(0x99);
    store.load(&mut gb, Slot::Quick).unwrap();
    assert_eq!(gb.mmu.cart.as_ref().unwrap().ram, saved);
    let mut restarted = machine(gb.model, 0x1b, 4);
    store.load(&mut restarted, Slot::Recovery).unwrap();
    assert!(
        restarted
            .mmu
            .cart
            .as_ref()
            .unwrap()
            .ram
            .iter()
            .all(|&b| b == 0x99)
    );
    store.load(&mut gb, Slot::Quick).unwrap();
    store.load(&mut gb, Slot::Recovery).unwrap();
    assert_eq!(gb.mmu.cart.as_ref().unwrap().ram, saved);
}

#[test]
fn invalid_files_and_wrong_machine_leave_state_untouched() {
    let mut gb = machine(Model::Dmg(DmgRevision::RevC), 3, 3);
    let bytes = gb.save_state().unwrap();
    for bad in [
        vec![],
        bytes[..100].to_vec(),
        {
            let mut b = bytes.clone();
            b[8] = 2;
            b
        },
        {
            let mut b = bytes.clone();
            b[100] ^= 1;
            b
        },
    ] {
        assert!(gb.load_state(&bad).is_err());
        assert_eq!(gb.cpu.pc, 0x100);
    }
    let other = machine(Model::Mgb, 3, 3).save_state().unwrap();
    assert!(gb.load_state(&other).is_err());
    gb.mmu.cart.as_mut().unwrap().rom[0x300] ^= 1;
    assert!(gb.load_state(&bytes).is_err());
    assert_eq!(save_state::metadata(&bytes).unwrap().version, 1);
}

#[test]
fn external_sessions_are_rejected_and_loopback_is_preserved() {
    use vibe_emu_core::serial::{LinkPort, NullLinkPort};
    struct External;
    impl LinkPort for External {
        fn transfer(&mut self, b: u8) -> u8 {
            b
        }
    }
    let mut gb = machine(Model::Mgb, 0, 0);
    let bytes = gb.save_state().unwrap();
    gb.mmu.serial.connect(Box::new(External));
    assert!(gb.save_state().is_err());
    assert!(gb.load_state(&bytes).is_err());
    gb.mmu.serial.connect(Box::new(NullLinkPort::new(true)));
    let bytes = gb.save_state().unwrap();
    gb.load_state(&bytes).unwrap();
    assert!(gb.save_state().is_ok());
}

#[test]
fn sram_restore_is_flushed_to_original_battery_file_and_undo_recovers_it() {
    for (mapper, ram_code) in [(3, 3), (6, 0), (0x10, 3), (0x10, 4), (0x1b, 4), (0xbc, 3)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game.gb");
        let mut rom = machine(Model::Mgb, mapper, ram_code).mmu.cart.unwrap().rom;
        if mapper == 0xbc {
            rom[0x149] = 0xc1;
            rom[0x14a] = 0x65;
            rom[0x150] = 1;
            rom[0x151] = 0;
            rom[0x152] = 3;
            rom[0x153] = 0x0c;
        }
        std::fs::write(&path, rom).unwrap();
        let mut gb = GameBoy::new(Model::Mgb);
        gb.load_cart(Cartridge::from_file(&path).unwrap());
        let cart = gb.mmu.cart.as_mut().unwrap();
        for (i, b) in cart.ram.iter_mut().enumerate() {
            *b = (i / 512 % 16) as u8;
        }
        let expected = cart.ram.clone();
        let store = StateStore::new(dir.path(), &gb).unwrap();
        store.save(&gb, Slot::Number(10)).unwrap();
        gb.mmu.cart.as_mut().unwrap().ram.fill(7);
        store.load(&mut gb, Slot::Number(10)).unwrap();
        gb.mmu.cart.as_mut().unwrap().save_ram().unwrap();
        assert_eq!(
            std::fs::read(path.with_extension("sav")).unwrap(),
            expected,
            "mapper {mapper:x}"
        );
        store.load(&mut gb, Slot::Recovery).unwrap();
        gb.mmu.cart.as_mut().unwrap().save_ram().unwrap();
        assert!(
            std::fs::read(path.with_extension("sav"))
                .unwrap()
                .iter()
                .all(|&b| b == 7)
        );
    }
}

#[test]
fn failed_recovery_write_and_atomic_replace_do_not_destroy_existing_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut gb = machine(Model::Mgb, 3, 3);
    let store = StateStore::new(dir.path(), &gb).unwrap();
    store.save(&gb, Slot::Number(1)).unwrap();
    let old = std::fs::read(store.path(Slot::Number(1)).unwrap()).unwrap();
    gb.mmu.cart.as_mut().unwrap().ram.fill(3);
    let recovery = store.path(Slot::Recovery).unwrap();
    std::fs::create_dir_all(&recovery).unwrap();
    assert!(store.load(&mut gb, Slot::Number(1)).is_err());
    assert!(gb.mmu.cart.as_ref().unwrap().ram.iter().all(|&b| b == 3));
    assert_eq!(
        std::fs::read(store.path(Slot::Number(1)).unwrap()).unwrap(),
        old
    );
    assert!(store.save(&gb, Slot::Number(0)).is_err());
    assert!(vibe_emu_core::state_store::write_atomic(&recovery, b"failed").is_err());
    assert_eq!(
        std::fs::read_dir(recovery.parent().unwrap())
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn hybrid_and_initial_border_roundtrip() {
    for initial in [false, true] {
        let mut gb = GameBoy::new(Model::from_cgb_flag(true));
        if !initial {
            gb.enable_sgb_extensions();
        }
        gb.load_cart(Cartridge::from_bytes(sgb_rom::initial_border_rom()));
        if initial {
            assert!(gb.borrow_sgb_border(None, 600));
        }
        run(&mut gb, 1000);
        let bytes = gb.save_state().unwrap();
        let before = gb.mmu.ppu.display_framebuffer().to_vec();
        run(&mut gb, 1000);
        gb.load_state(&bytes).unwrap();
        assert_eq!(gb.mmu.ppu.display_framebuffer(), before);
    }
}

#[test]
fn checksummed_invalid_indices_are_rejected_without_mutation() {
    use sha2::{Digest, Sha256};
    let mut gb = machine(Model::Mgb, 3, 3);
    let bytes = gb.save_state().unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&bytes[44..]).unwrap();
    for field in ["wram_bank", "dma_cycles"] {
        let saved = json["machine"]["mmu"][field].clone();
        json["machine"]["mmu"][field] = 65535.into();
        let payload = serde_json::to_vec(&json).unwrap();
        let mut invalid = bytes[..12].to_vec();
        invalid.extend_from_slice(&Sha256::digest(&payload));
        invalid.extend(payload);
        assert!(gb.load_state(&invalid).is_err());
        assert_eq!(gb.cpu.pc, 0x100);
        json["machine"]["mmu"][field] = saved;
    }
}

#[test]
fn pending_dma_timer_serial_audio_and_rtc_continue_exactly() {
    use vibe_emu_core::serial::NullLinkPort;
    for model in [Model::Mgb, Model::from_cgb_flag(true), Model::Sgb2] {
        let mut gb = machine(model, 0x10, 4);
        let audio = gb.mmu.apu.enable_output(44_100);
        gb.mmu.serial.connect(Box::new(NullLinkPort::new(true)));
        for (addr, value) in [
            (0xff26, 0x80),
            (0xff24, 0x77),
            (0xff25, 0xff),
            (0xff11, 0x80),
            (0xff12, 0xf3),
            (0xff13, 0xff),
            (0xff14, 0x87),
            (0xff1a, 0x80),
            (0xff1c, 0x20),
            (0xff1e, 0x87),
            (0xff21, 0xf2),
            (0xff22, 0x25),
            (0xff23, 0x80),
            (0xff05, 0xfe),
            (0xff06, 0x73),
            (0xff07, 5),
            (0xff01, 0x5a),
            (0xff02, 0x81),
            (0xff46, 0xc0),
            (0x0000, 0x0a),
            (0x4000, 8),
            (0xa000, 37),
        ] {
            gb.mmu.write_byte(addr, value);
        }
        // Execute from HRAM while OAM DMA owns the cartridge bus.
        gb.mmu.hram[..3].copy_from_slice(&[0x00, 0x18, 0xfd]);
        gb.cpu.pc = 0xff80;
        for steps in [1, 3, 17, 99, 777] {
            run(&mut gb, steps);
            while audio.pop_stereo().is_some() {}
            let saved = gb.save_state().unwrap();
            let mut restored = machine(model, 0x10, 4);
            let restored_audio = restored.mmu.apu.enable_output(44_100);
            restored.load_state(&saved).unwrap();
            run(&mut gb, 1000);
            run(&mut restored, 1000);
            let a: serde_json::Value =
                serde_json::from_slice(&gb.save_state().unwrap()[44..]).unwrap();
            let b: serde_json::Value =
                serde_json::from_slice(&restored.save_state().unwrap()[44..]).unwrap();
            assert_eq!(a["machine"], b["machine"]);
            let samples = |q: &vibe_emu_core::audio_queue::AudioConsumer| {
                let mut v = vec![];
                while let Some(s) = q.pop_stereo() {
                    v.push(s)
                }
                v
            };
            assert_eq!(samples(&audio), samples(&restored_audio));
        }
    }
}

#[test]
fn pending_sgb_tile_transfer_and_packet_bits_survive_restore() {
    let mut gb = machine(Model::Sgb2, 0, 0);
    let mut packet = [0; 16];
    packet[0] = (0x13 << 3) | 1;
    packet[1] = 1;
    gb.mmu.write_byte(0xff00, 0);
    gb.mmu.write_byte(0xff00, 0x30);
    for (i, byte) in packet.into_iter().enumerate() {
        for bit in 0..8 {
            gb.mmu
                .write_byte(0xff00, if byte & (1 << bit) == 0 { 0x20 } else { 0x10 });
            gb.mmu.write_byte(0xff00, 0x30);
        }
        if i == 8 {
            let saved = gb.save_state().unwrap();
            gb.load_state(&saved).unwrap();
        }
    }
    gb.mmu.write_byte(0xff00, 0x20);
    gb.mmu.write_byte(0xff00, 0x30);
    let saved = gb.save_state().unwrap();
    gb.load_state(&saved).unwrap();
    run(&mut gb, 80000);
}

#[test]
fn lcd_restart_and_boot_mapped_states_roundtrip() {
    for model in [Model::default(), Model::Mgb, Model::from_cgb_flag(true)] {
        let mut gb = machine(model, 3, 3);
        gb.mmu.write_byte(0xff40, 0);
        gb.mmu.write_byte(0xff40, 0x91);
        for _ in 0..30 {
            run(&mut gb, 7);
            let saved = gb.save_state().unwrap();
            gb.load_state(&saved).unwrap();
        }
        gb.mmu
            .load_boot_rom(vec![0; if model.is_cgb() { 0x900 } else { 0x100 }]);
        let bytes = gb.save_state().unwrap();
        gb.load_state(&bytes).unwrap();
        gb.mmu.boot_rom.as_mut().unwrap()[0] = 1;
        assert!(gb.load_state(&bytes).is_err());
    }
}
