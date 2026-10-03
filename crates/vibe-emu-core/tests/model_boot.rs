mod common;
use vibe_emu_core::{apu::Apu, cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

#[test]
fn sgb_skipped_boot_tracks_header_packet_timing() {
    let base = std::fs::read(common::rom_path(
        "mooneye-test-suite/acceptance/boot_div-S.gb",
    ))
    .unwrap();
    for model in [Model::Sgb, Model::Sgb2] {
        let boot = std::fs::read(common::model_boot_rom_path(model)).unwrap();
        for fill in [0u8, 0x17, 0x80, 0xff] {
            let mut rom = base.clone();
            // Change bytes in several packets, including their computed sums.
            rom[0x134..0x144].fill(fill);
            rom[0x14e] = fill;
            rom[0x14f] = !fill;
            let mut actual = GameBoy::new_power_on(model);
            actual.mmu.load_boot_rom(boot.clone());
            actual.load_cart(Cartridge::from_bytes(rom.clone()));
            while actual.mmu.boot_mapped && actual.cpu.cycles < 3_000_000 {
                actual.cpu.step(&mut actual.mmu);
            }
            assert!(!actual.mmu.boot_mapped);
            let mut skipped = GameBoy::new(model);
            skipped.load_cart(Cartridge::from_bytes(rom));
            assert_eq!(
                skipped.mmu.timer.div, actual.mmu.timer.div,
                "{model:?} header={fill:02X}"
            );
            assert_eq!(skipped.mmu.dot_div, actual.mmu.dot_div);
            assert_eq!(skipped.mmu.ppu.ly(), actual.mmu.ppu.ly());
            assert_eq!(skipped.mmu.ppu.mode_clock(), actual.mmu.ppu.mode_clock());
            assert_eq!(&skipped.mmu.wram[0][..96], &actual.mmu.wram[0][..96]);
            skipped.reset();
            assert_eq!(skipped.mmu.timer.div, actual.mmu.timer.div);
            assert_eq!(skipped.model, model);
            assert_eq!(skipped.mmu.model(), model);
        }
    }
}

#[test]
fn agb_active_wave_ram_is_disconnected() {
    for model in [Model::Agb0, Model::Agb] {
        let mut apu = Apu::new(model);
        apu.write_reg(0xff26, 0x80);
        apu.write_reg(0xff30, 0x12);
        apu.write_reg(0xff1a, 0x80);
        apu.write_reg(0xff1c, 0x20);
        apu.write_reg(0xff1e, 0x80);
        assert_eq!(apu.read_reg(0xff30), 0xff);
        apu.write_reg(0xff30, 0x34);
        apu.write_reg(0xff1a, 0);
        assert_eq!(apu.read_reg(0xff30), 0x12);
    }
}

#[test]
fn added_models_keep_identity_and_boot_mapping_on_reset() {
    for model in [Model::Mgb, Model::Sgb, Model::Sgb2, Model::Agb0, Model::Agb] {
        let mut gb = GameBoy::new_power_on(model);
        gb.mmu
            .load_boot_rom(vec![0x42; if model.is_cgb() { 0x900 } else { 0x100 }]);
        gb.load_cart(Cartridge::from_bytes(vec![0; 0x8000]));
        assert_eq!(gb.mmu.read_byte(0), 0x42);
        assert_eq!(gb.mmu.read_byte(0x100), 0);
        assert_eq!(
            gb.mmu.read_byte(0x200),
            if model.is_cgb() { 0x42 } else { 0 }
        );
        gb.mmu.write_byte(0xff50, 1);
        assert_eq!(gb.mmu.read_byte(0), 0);
        gb.reset_power_on();
        assert_eq!(gb.mmu.read_byte(0), 0x42);
        assert_eq!(gb.mmu.model(), model);
        gb.reset();
        assert_eq!(gb.mmu.model(), model);
        assert_eq!(gb.model, model);
        assert_eq!(gb.cpu.pc, 0x100);
    }
}

#[test]
fn mgb_halted_dma_resumes_after_interrupt() {
    let rom = std::fs::read(common::rom_path(
        "mooneye-test-suite/madness/mgb_oam_dma_halt_sprites.gb",
    ))
    .unwrap();
    let mut gb = GameBoy::new(Model::Mgb);
    gb.load_cart(Cartridge::from_bytes(rom));
    while !gb.cpu.halted && gb.cpu.cycles < 2_000_000 {
        gb.cpu.step(&mut gb.mmu);
    }
    assert!(gb.cpu.halted);
    let remaining = gb.mmu.dma_cycles;
    let oam = gb.mmu.ppu.oam;
    assert!(remaining > 0);
    for _ in 0..200 {
        gb.cpu.step(&mut gb.mmu);
    }
    assert_eq!(gb.mmu.dma_cycles, remaining);
    assert_eq!(gb.mmu.ppu.oam, oam);
    gb.mmu.ie_reg = 4;
    gb.mmu.if_reg |= 4;
    gb.cpu.step(&mut gb.mmu);
    assert!(!gb.cpu.halted);
    // The wake step is still a HALT idle cycle. DMA resumes with the next
    // instruction's clock, after the CPU has left HALT.
    gb.cpu.step(&mut gb.mmu);
    assert!(gb.mmu.dma_cycles < remaining);
}

#[test]
fn cgb_skipped_boot_div_phase_follows_cartridge_mode() {
    use vibe_emu_core::hardware::CgbRevision;
    for revision in [
        CgbRevision::RevA,
        CgbRevision::RevB,
        CgbRevision::RevC,
        CgbRevision::RevD,
        CgbRevision::RevE,
    ] {
        // Native timing is the Gambatte start_inc pair. Compatibility timing
        // is the first Mooneye boot_div-cgbABCDE read and its preceding cycle.
        for (native, nops, expected) in [
            (true, 13, 0x1e),
            (true, 14, 0x1f),
            (false, 26, 0x26),
            (false, 27, 0x27),
        ] {
            let mut rom = vec![0; 0x8000];
            rom[0x143] = if native { 0xc0 } else { 0 };
            let read_base = if native {
                rom[0x100..0x103].copy_from_slice(&[0xc3, 0x50, 0x01]);
                rom[0x150..0x153].copy_from_slice(&[0xc3, 0x00, 0x10]);
                0x1000
            } else {
                rom[0x101..0x104].copy_from_slice(&[0xc3, 0x50, 0x01]);
                0x150
            };
            rom[read_base + nops..read_base + nops + 2].copy_from_slice(&[0xf0, 0x04]);
            let mut gb = GameBoy::new(Model::Cgb(revision));
            gb.load_cart(Cartridge::from_bytes(rom));
            while usize::from(gb.cpu.pc) != read_base + nops + 2 {
                assert!(gb.cpu.cycles < 512);
                gb.cpu.step(&mut gb.mmu);
            }
            assert_eq!(
                gb.cpu.a, expected,
                "{revision:?}, native={native}, nops={nops}"
            );
        }
    }
}
