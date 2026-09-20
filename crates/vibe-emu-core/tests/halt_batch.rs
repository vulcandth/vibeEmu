//! Compare the bounded runner with the unchanged instruction/M-cycle runner.
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

fn machine(model: Model, double_speed: bool, ime: bool) -> GameBoy {
    let mut gb = GameBoy::new(model);
    let mut rom = vec![0; 0x8000];
    // Successive HALTs also tolerate the HALT bug when IME is disabled.
    // Keep the cartridge header zeroed so this stays a ROM-only mapper.
    rom[0x150..].fill(0x76);
    // Each interrupt returns to the HALTs; IME-off cases wake without service.
    for vector in [0x40, 0x48, 0x50, 0x58, 0x60] {
        rom[vector] = 0xd9; // RETI
    }
    gb.cpu.pc = 0x150;
    gb.mmu.load_cart(Cartridge::from_bytes(rom));
    gb.cpu.double_speed = double_speed;
    gb.mmu.key1 = if double_speed { 0x80 } else { 0 };
    gb.cpu.ime = ime;
    gb.mmu.if_reg = 0;
    gb.mmu.ie_reg = 0x1f;
    gb.mmu.write_byte(0xff26, 0x80);
    gb.mmu.write_byte(0xff41, 0x78);
    gb.mmu.write_byte(0xff45, 153);
    gb
}

#[test]
fn halt_batches_preserve_wakeup_dma_serial_and_frame_boundaries() {
    for model in [Model::default(), Model::Cgb(Default::default())] {
        for double_speed in [false, true] {
            if double_speed && model.is_dmg() {
                continue;
            }
            for ime in [false, true] {
                let mut actual = machine(model, double_speed, ime);
                let mut expected = machine(model, double_speed, ime);
                let audio = actual.mmu.apu.enable_output(48_000);
                let reference_audio = expected.mmu.apu.enable_output(48_000);
                let mut batches = 0;
                for iteration in 0..16_000 {
                    if !ime && iteration % 97 == 0 {
                        actual.mmu.if_reg = 0;
                        expected.mmu.if_reg = 0;
                    }
                    // Interleave external writes at the same observation boundary.
                    if iteration % 997 == 0 {
                        let (addr, value) = [
                            (0xff07, 5), // fastest TIMA clock
                            (0xff06, 0xfc),
                            (0xff04, 0),
                            (0xff05, 255),
                            (0xff07, 0),
                            (0xff46, 0xc0), // OAM DMA
                            (0xff02, 0x81), // internal serial clock
                            (0xff02, 0x80), // external serial clock
                            (0xff40, 0),
                            (0xff40, 0x93),
                            (0xff26, 0),
                            (0xff26, 0x80),
                            (0xff55, 0),    // CGB GDMA
                            (0xff55, 0x81), // CGB HBlank DMA
                            (0xff0f, 0x10),
                            (0xff41, 0),
                            (0xff45, 0),
                        ][iteration / 997];
                        actual.mmu.write_byte(addr, value);
                        expected.mmu.write_byte(addr, value);
                    }
                    if iteration % 103 == 0 {
                        actual
                            .mmu
                            .serial
                            .external_clock_pulse(1, &mut actual.mmu.if_reg);
                        expected
                            .mmu
                            .serial
                            .external_clock_pulse(1, &mut expected.mmu.if_reg);
                    }
                    let before = actual.cpu.cycles;
                    let was_halted = actual.cpu.halted;
                    let budget = [0, 1, 3, 4, 8, 17, 256, 1024, u16::MAX][iteration % 9];
                    actual.cpu.step_with_halt_batch(&mut actual.mmu, budget);
                    let mut calls = 0;
                    while expected.cpu.cycles < actual.cpu.cycles {
                        expected.cpu.step(&mut expected.mmu);
                        calls += 1;
                        // No batch is allowed to hide an interrupt or frame event.
                        if expected.cpu.cycles < actual.cpu.cycles {
                            assert!(expected.cpu.halted);
                            assert_eq!(expected.mmu.if_reg, actual.mmu.if_reg);
                            assert_eq!(
                                expected.mmu.ppu.frame_ready(),
                                actual.mmu.ppu.frame_ready()
                            );
                        }
                    }
                    if calls > 1 {
                        assert!(was_halted);
                        assert!(actual.cpu.cycles - before <= u64::from(budget));
                        batches += 1;
                    }
                    assert_eq!(format!("{:?}", actual.cpu), format!("{:?}", expected.cpu));
                    assert_eq!(
                        format!("{:?}", actual.mmu.timer),
                        format!("{:?}", expected.mmu.timer)
                    );
                    assert_eq!(actual.mmu.dot_div, expected.mmu.dot_div);
                    assert_eq!(actual.mmu.if_reg, expected.mmu.if_reg);
                    assert_eq!(actual.mmu.ppu.mode(), expected.mmu.ppu.mode());
                    assert_eq!(actual.mmu.ppu.mode_clock(), expected.mmu.ppu.mode_clock());
                    assert_eq!(actual.mmu.ppu.ly(), expected.mmu.ppu.ly());
                    assert_eq!(actual.mmu.ppu.frame_ready(), expected.mmu.ppu.frame_ready());
                    assert_eq!(
                        format!("{:?}", actual.mmu.ppu),
                        format!("{:?}", expected.mmu.ppu)
                    );
                    assert_eq!(actual.mmu.ppu.oam, expected.mmu.ppu.oam);
                    loop {
                        let sample = audio.pop_stereo();
                        assert_eq!(sample, reference_audio.pop_stereo());
                        if sample.is_none() {
                            break;
                        }
                    }
                    if iteration % 113 == 0 {
                        assert_eq!(actual.mmu.ppu.framebuffer(), expected.mmu.ppu.framebuffer());
                        assert_eq!(
                            actual.capture_boot_handoff_snapshot(),
                            expected.capture_boot_handoff_snapshot()
                        );
                        actual.mmu.ppu.clear_frame_flag();
                        expected.mmu.ppu.clear_frame_flag();
                    }
                }
                assert!(
                    batches > 100,
                    "batching was not exercised: {model:?} {double_speed} {ime}: {batches}"
                );
            }
        }
    }
}

#[test]
fn bounded_runner_matches_instructions_with_sound_mmio() {
    for model in [Model::default(), Model::Cgb(Default::default())] {
        for double_speed in [false, true] {
            if model.is_dmg() && double_speed {
                continue;
            }
            let make = || {
                let mut gb = machine(model, double_speed, true);
                let mut rom = vec![0; 0x8000];
                for vector in [0x40, 0x48, 0x50, 0x58, 0x60] {
                    rom[vector] = 0xd9;
                }
                let setup = [
                    0x3e, 0x80, 0xe0, 0x26, 0x3e, 0x77, 0xe0, 0x24, 0x3e, 0xff, 0xe0, 0x25, 0x3e,
                    0xf3, 0xe0, 0x12, 0x3e, 0x87, 0xe0, 0x14, 0x3e, 0x80, 0xe0, 0x1a, 0x3e, 0x20,
                    0xe0, 0x1c, 0x3e, 0x87, 0xe0, 0x1e,
                ];
                rom[0x150..0x150 + setup.len()].copy_from_slice(&setup);
                let loop_addr = 0x150 + setup.len();
                let program = [
                    0x3c,
                    0xe0,
                    0x13,
                    0xf0,
                    0x76,
                    0xea,
                    0x00,
                    0xc0,
                    0xf0,
                    0x26,
                    0xe0,
                    0x22,
                    0xe0,
                    0x80,
                    0xf0,
                    0x30,
                    0xcb,
                    0x37,
                    0x06,
                    0x08,
                    0x05,
                    0x20,
                    0xfd,
                    0xe0,
                    0x04,
                    0xc3,
                    loop_addr as u8,
                    (loop_addr >> 8) as u8,
                ];
                rom[loop_addr..loop_addr + program.len()].copy_from_slice(&program);
                gb.mmu.load_cart(Cartridge::from_bytes(rom));
                gb
            };
            let mut actual = make();
            let mut expected = make();
            let audio = actual.mmu.apu.enable_output(48_000);
            let reference_audio = expected.mmu.apu.enable_output(48_000);
            for iteration in 0..2000 {
                let budget = [0, 1, 4, 17, 256, 4096][iteration % 6];
                actual.cpu.run_for_dots(&mut actual.mmu, budget);
                while expected.cpu.cycles < actual.cpu.cycles {
                    expected.cpu.step(&mut expected.mmu);
                }
                assert_eq!(format!("{:?}", actual.cpu), format!("{:?}", expected.cpu));
                assert_eq!(
                    actual.capture_boot_handoff_snapshot(),
                    expected.capture_boot_handoff_snapshot()
                );
                assert_eq!(actual.mmu.ppu.framebuffer(), expected.mmu.ppu.framebuffer());
                assert_eq!(actual.mmu.ppu.frame_ready(), expected.mmu.ppu.frame_ready());
                loop {
                    let sample = audio.pop_stereo();
                    assert_eq!(sample, reference_audio.pop_stereo());
                    if sample.is_none() {
                        break;
                    }
                }
                actual.mmu.ppu.clear_frame_flag();
                expected.mmu.ppu.clear_frame_flag();
            }
        }
    }
}

#[test]
fn bounded_runner_preserves_external_link_polling() {
    use vibe_emu_core::serial::{LinkPort, NullLinkPort};
    struct ExternalPort;
    impl LinkPort for ExternalPort {
        fn transfer(&mut self, _: u8) -> u8 {
            0xff
        }
    }
    let mut gb = machine(Model::default(), false, false);
    // Run NOPs rather than HALTs so multiple instructions could be combined.
    gb.cpu.pc = 0x100;
    gb.mmu.serial.connect(Box::new(ExternalPort));
    gb.cpu.run_for_dots(&mut gb.mmu, 4096);
    assert_eq!(gb.cpu.pc, 0x101);
    assert_eq!(gb.cpu.cycles, 4);
    gb.mmu.serial.connect(Box::new(NullLinkPort::default()));
    gb.cpu.run_for_dots(&mut gb.mmu, 64);
    assert!(gb.cpu.pc > 0x102);
    let cycles = gb.cpu.cycles;
    gb.cpu.run_for_dots(&mut gb.mmu, 0);
    assert_eq!(gb.cpu.cycles, cycles);
}

#[test]
fn bounded_runner_preserves_ppu_accesses_dma_and_interrupts() {
    use vibe_emu_core::hardware::{CgbRevision, DmgRevision};
    let models = [
        Model::Dmg(DmgRevision::Rev0),
        Model::Dmg(DmgRevision::RevB),
        Model::Cgb(CgbRevision::Rev0),
        Model::Cgb(CgbRevision::RevC),
        Model::Cgb(CgbRevision::RevE),
    ];
    for model in models {
        for config in 0..if model.is_cgb() { 3 } else { 1 } {
            let make = || {
                let mut gb = machine(model, config == 1, true);
                let mut rom = vec![0; 0x8000];
                for vector in [0x40, 0x48, 0x50, 0x58, 0x60] {
                    rom[vector] = 0xd9;
                }
                if model.is_cgb() {
                    rom[0x143] = 0x80;
                }
                // CPU-visible VRAM/OAM accesses, IDU corruption, LY/STAT polls,
                // scroll/palette writes, and ordinary RAM traffic in one loop.
                let program = [
                    0x7e, 0x22, 0x23, 0x2b, 0xf0, 0x44, 0xea, 0x00, 0xc0, 0xf0, 0x41, 0xea, 0x01,
                    0xc0, 0x78, 0xe0, 0x43, 0x04, 0xe0, 0x69, 0x06, 0x20, 0x05, 0x20, 0xfd, 0xc3,
                    0x50, 0x01,
                ];
                rom[0x150..0x150 + program.len()].copy_from_slice(&program);
                gb.mmu.load_cart(Cartridge::from_bytes(rom));
                if config == 2 {
                    gb.mmu.ppu.set_dmg_compat_mode(true);
                }
                for (i, byte) in gb.mmu.ppu.vram.iter_mut().flatten().enumerate() {
                    *byte = (i as u8).wrapping_mul(37).wrapping_add((i >> 8) as u8);
                }
                for (i, sprite) in gb.mmu.ppu.oam.chunks_exact_mut(4).enumerate() {
                    sprite.copy_from_slice(&[
                        16 + (i as u8 % 18) * 8,
                        (i as u8).wrapping_mul(17),
                        i as u8,
                        (i as u8).wrapping_mul(29),
                    ]);
                }
                gb
            };
            let mut actual = make();
            let mut expected = make();
            let audio = actual.mmu.apu.enable_output(48_000);
            let reference_audio = expected.mmu.apu.enable_output(48_000);
            for iteration in 0..2500usize {
                for gb in [&mut actual, &mut expected] {
                    if iteration % 71 == 0 {
                        let addr =
                            [0x8000u16, 0x9ff0, 0xfe00, 0xfe98, 0xfea0, 0xc100][iteration / 71 % 6];
                        gb.cpu.h = (addr >> 8) as u8;
                        gb.cpu.l = addr as u8;
                    }
                    if iteration % 53 == 0 {
                        let (addr, val) = [
                            (0xff41, 0x78),
                            (0xff45, 0),
                            (0xff40, 0),
                            (0xff40, 0xf7),
                            (0xff46, 0xc0),
                            (0xff4f, 1),
                            (0xff51, 0xc1),
                            (0xff52, 0),
                            (0xff53, 0x80),
                            (0xff54, 0),
                            (0xff55, 0x82),
                            (0xff55, 0),
                            (0xff4a, 0),
                            (0xff4b, 4),
                            (0xff68, 0x80),
                            (0xff6c, 1),
                            (0xff07, 5),
                            (0xff05, 0xff),
                            (0xff06, 0xf0),
                            (0xff07, 0),
                        ][iteration / 53 % 20];
                        gb.mmu.write_byte(addr, val);
                    }
                    if iteration % 127 == 0 {
                        gb.mmu.ppu.queue_reg_write(0xff43, iteration as u8, 3);
                    }
                }
                let budget = [1, 2, 4, 17, 128, 4096][iteration % 6];
                actual.cpu.run_for_dots(&mut actual.mmu, budget);
                while expected.cpu.cycles < actual.cpu.cycles {
                    expected.cpu.step(&mut expected.mmu);
                }
                assert_eq!(
                    format!("{:?}", actual.cpu),
                    format!("{:?}", expected.cpu),
                    "{model:?} config={config} iteration={iteration}"
                );
                assert_eq!(
                    format!("{:?}", actual.mmu.ppu),
                    format!("{:?}", expected.mmu.ppu)
                );
                assert!(!actual.cpu.faulted);
                assert_eq!(actual.mmu.if_reg, expected.mmu.if_reg);
                assert_eq!(actual.mmu.dot_div, expected.mmu.dot_div);
                assert_eq!(
                    format!("{:?}", actual.mmu.timer),
                    format!("{:?}", expected.mmu.timer)
                );
                assert_eq!(actual.mmu.ppu.mode_clock(), expected.mmu.ppu.mode_clock());
                assert_eq!(actual.mmu.ppu.vram, expected.mmu.ppu.vram);
                assert_eq!(actual.mmu.ppu.oam, expected.mmu.ppu.oam);
                if iteration % 31 == 0 {
                    assert_eq!(
                        actual.capture_boot_handoff_snapshot(),
                        expected.capture_boot_handoff_snapshot()
                    );
                    assert_eq!(actual.mmu.ppu.framebuffer(), expected.mmu.ppu.framebuffer());
                }
                loop {
                    let sample = audio.pop_stereo();
                    assert_eq!(sample, reference_audio.pop_stereo());
                    if sample.is_none() {
                        break;
                    }
                }
                actual.mmu.ppu.clear_frame_flag();
                expected.mmu.ppu.clear_frame_flag();
            }
        }
    }
}
