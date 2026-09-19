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
