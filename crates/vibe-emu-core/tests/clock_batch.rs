//! Shared clock queues must stop at CPU bus and interrupt observations.
use vibe_emu_core::{
    audio_queue::AudioConsumer,
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, Model},
};

const ENTRY: usize = 0x200;

fn machine(model: Model, double_speed: bool, program: &[u8]) -> GameBoy {
    let mut rom = vec![0; 0x10000];
    rom[0x143] = if model.is_cgb() { 0x80 } else { 0 };
    rom[0x147] = 3; // MBC1 with RAM, allowing execution from cartridge RAM.
    rom[0x148] = 1;
    rom[0x149] = 2;
    rom[ENTRY..ENTRY + program.len()].copy_from_slice(program);
    for vector in [0x40, 0x48, 0x50, 0x58, 0x60] {
        rom[vector..vector + 3].copy_from_slice(&[0xc3, 0x80, 0x01]);
    }
    // Preserve AF while counting serviced interrupts in WRAM.
    rom[0x180..0x18a]
        .copy_from_slice(&[0xf5, 0xfa, 0x80, 0xc0, 0x3c, 0xea, 0x80, 0xc0, 0xf1, 0xd9]);
    let mut gb = GameBoy::new(model);
    gb.mmu.load_cart(Cartridge::from_bytes(rom));
    gb.cpu.pc = ENTRY as u16;
    gb.cpu.sp = 0xdffe;
    gb.cpu.double_speed = double_speed;
    gb.cpu.ime = false;
    gb.mmu.key1 = if double_speed { 0x80 } else { 0 };
    gb.mmu.ie_reg = 0;
    gb.mmu.if_reg = 0;
    gb.mmu.write_byte(0x0000, 0x0a);
    gb.mmu.write_byte(0xff40, 0);
    gb.mmu.wram[0].fill(0);
    for (i, byte) in gb.mmu.ppu.oam.iter_mut().enumerate() {
        *byte = (i as u8).wrapping_mul(37) ^ 0xa5;
    }
    for (addr, value) in [
        (0xff26, 0x80),
        (0xff24, 0x77),
        (0xff25, 0xff),
        (0xff12, 0xf3),
        (0xff13, 0xf8),
        (0xff14, 0x87),
        (0xff21, 0xf2),
        (0xff22, 0x05),
        (0xff23, 0x80),
        (0xff40, 0x93),
    ] {
        gb.mmu.write_byte(addr, value);
    }
    gb.mmu.ppu.skip_startup_for_test();
    gb.mmu.if_reg = 0;
    gb
}

fn models() -> impl Iterator<Item = (Model, bool)> {
    [
        Model::default(),
        Model::Mgb,
        Model::Cgb(CgbRevision::RevB),
        Model::Cgb(CgbRevision::RevE),
    ]
    .into_iter()
    .flat_map(|model| {
        [false, true]
            .into_iter()
            .filter(move |double_speed| !double_speed || model.is_cgb())
            .map(move |double_speed| (model, double_speed))
    })
}

fn compare_run(actual: &mut GameBoy, expected: &mut GameBoy, budget: u16) {
    actual.cpu.run_for_dots(&mut actual.mmu, budget);
    while expected.cpu.cycles < actual.cpu.cycles {
        let before = expected.cpu.cycles;
        expected.cpu.step(&mut expected.mmu);
        assert!(
            expected.cpu.cycles > before,
            "reference stopped unexpectedly"
        );
    }
    assert_eq!(format!("{:?}", actual.cpu), format!("{:?}", expected.cpu));
    assert_eq!(
        format!("{:?}", actual.mmu.timer),
        format!("{:?}", expected.mmu.timer)
    );
    assert_eq!(actual.mmu.dot_div, expected.mmu.dot_div);
    assert_eq!(actual.mmu.if_reg, expected.mmu.if_reg);
    assert_eq!(actual.mmu.ie_reg, expected.mmu.ie_reg);
    assert_eq!(actual.mmu.wram, expected.mmu.wram);
    assert_eq!(actual.mmu.hram, expected.mmu.hram);
    assert_eq!(actual.mmu.ppu.oam, expected.mmu.ppu.oam);
    assert_eq!(actual.mmu.ppu.vram, expected.mmu.ppu.vram);
    assert_eq!(
        format!("{:?}", actual.mmu.ppu),
        format!("{:?}", expected.mmu.ppu)
    );
    assert_eq!(actual.mmu.ppu.framebuffer(), expected.mmu.ppu.framebuffer());
    assert_eq!(
        actual.mmu.apu.debug_boot_snapshot(),
        expected.mmu.apu.debug_boot_snapshot()
    );
    assert_eq!(
        serde_json::to_value(&actual.mmu.serial).unwrap(),
        serde_json::to_value(&expected.mmu.serial).unwrap()
    );
}

fn compare_saved_observations(actual: &GameBoy, expected: &GameBoy) {
    let actual = serde_json::to_value(&actual.mmu).unwrap();
    let expected = serde_json::to_value(&expected.mmu).unwrap();
    // Include private bus latches, complete PPU timing/event state and DMA
    // state. APU equivalence is checked through its snapshot and every sample;
    // its derived batching caches need not have identical representations.
    for field in [
        "data_bus",
        "main_bus",
        "dma_source",
        "dma_cycles",
        "pending_dma",
        "pending_delay",
        "gdma_cycles",
        "hdma",
        "ppu",
        "timer",
        "serial",
        "cart",
    ] {
        assert!(
            actual.get(field).is_some(),
            "missing serialized field {field}"
        );
        assert!(
            actual.get(field) == expected.get(field),
            "different {field}"
        );
    }
}

fn drain_equal(actual: &AudioConsumer, expected: &AudioConsumer) -> usize {
    let mut count = 0;
    loop {
        let sample = actual.pop_stereo();
        assert_eq!(sample, expected.pop_stereo());
        if sample.is_none() {
            return count;
        }
        count += 1;
    }
}

fn run_to(actual: &mut GameBoy, expected: &mut GameBoy, target: u64, split: bool) {
    let budgets = if split {
        [0, 1, 3, 7, 17, 64]
    } else {
        [u16::MAX; 6]
    };
    let mut iteration = 0;
    while actual.cpu.cycles < target {
        let budget = budgets[iteration % budgets.len()].min((target - actual.cpu.cycles) as u16);
        compare_run(actual, expected, budget);
        iteration += 1;
        assert!(
            iteration < 100,
            "fixture failed to reach its instruction boundary"
        );
    }
    assert_eq!(actual.cpu.cycles, target);
    compare_saved_observations(actual, expected);
}

#[test]
fn opcode_and_operand_fetches_synchronize_device_boundaries() {
    for (model, double_speed) in models() {
        for case in 0..5 {
            for split in [false, true] {
                let make = || {
                    let mut program = vec![0; 8];
                    program.extend([0xc3, 0x04, 0xff]); // JP DIV: execute LD A,TIMA.
                    let mut gb = machine(model, double_speed, &program);
                    let cpu_clocks = match case {
                        0 => {
                            gb.mmu.timer.div = 0x3df0;
                            gb.mmu.write_byte(0xff05, 0xa7);
                            56 // Eight NOPs, JP, then opcode+operand from DIV/TIMA.
                        }
                        1 => {
                            gb.cpu.pc = 0x7ff8;
                            gb.mmu.cart.as_mut().unwrap().rom[0x7fff] = 0x21;
                            gb.mmu.ppu.vram[0][0] = 0x34;
                            gb.mmu.ppu.vram[0][1] = 0x12;
                            // First operand just before transfer starts; the
                            // second is fetched after VRAM ownership changes.
                            gb.mmu.ppu.step(
                                79 - 8 * if double_speed { 2 } else { 4 },
                                &mut gb.mmu.if_reg,
                            );
                            40
                        }
                        2 => {
                            gb.mmu.cart.as_mut().unwrap().rom[ENTRY + 9..ENTRY + 11]
                                .copy_from_slice(&[0xff, 0x9f]);
                            gb.mmu.ppu.vram[0][0x1fff] = 0x21; // LD HL,d16.
                            gb.mmu.write_byte(0xa000, 0x34);
                            gb.mmu.write_byte(0xa001, 0x12);
                            60 // Opcode in VRAM, both operands in cartridge RAM.
                        }
                        3 => {
                            gb.mmu.cart.as_mut().unwrap().rom[ENTRY + 9..ENTRY + 11]
                                .copy_from_slice(&[0x00, 0xa0]);
                            gb.mmu.write_byte(0xa000, 0x3e);
                            gb.mmu.write_byte(0xa001, 0x6b);
                            56 // Both opcode and operand in cartridge RAM.
                        }
                        _ => {
                            gb.cpu.pc = 0xfffc;
                            gb.mmu.hram[0x7c..0x7f].copy_from_slice(&[0, 0, 0x06]);
                            gb.mmu.ie_reg = 0xa7;
                            16 // LD B,d8's operand crosses HRAM into IE.
                        }
                    };
                    (gb, cpu_clocks >> u32::from(double_speed))
                };
                let (mut actual, target) = make();
                let (mut expected, _) = make();
                run_to(&mut actual, &mut expected, target, split);
                match case {
                    0 => assert_eq!((actual.cpu.a, actual.cpu.pc), (0xa7, 0xff06)),
                    1 => assert_eq!(actual.cpu.pc, 0x8002),
                    2 => assert_eq!(actual.cpu.get_hl(), 0x1234),
                    3 => assert_eq!(actual.cpu.a, 0x6b),
                    _ => assert_eq!((actual.cpu.b, actual.cpu.pc), (0xa7, 0)),
                }
            }
        }
    }
}

#[test]
fn queued_clocks_precede_oam_incrementer_and_stack_bus_effects() {
    for (model, double_speed) in models() {
        let mut corruptions = 0;
        for (opcode, clocks) in [
            (0x03, 8),
            (0x0b, 8),
            (0x13, 8),
            (0x1b, 8),
            (0x23, 8),
            (0x2b, 8),
            (0x33, 8),
            (0x3b, 8),
            (0xc5, 16),
            (0xd5, 16),
            (0xe5, 16),
            (0xf5, 16),
            (0xc1, 12),
            (0xd1, 12),
            (0xe1, 12),
            (0xf1, 12),
            (0x22, 8),
            (0x32, 8),
            (0x2a, 8),
            (0x3a, 8),
        ] {
            let make = || {
                let mut gb = machine(model, double_speed, &[0, 0, 0, opcode]);
                gb.cpu.b = 0xfe;
                gb.cpu.c = 0x2e;
                gb.cpu.d = 0xfe;
                gb.cpu.e = 0x2e;
                gb.cpu.h = 0xfe;
                gb.cpu.l = 0x2e;
                gb.cpu.sp = 0xfe2e;
                gb
            };
            let mut actual = make();
            let mut expected = make();
            let before = actual.mmu.ppu.oam;
            run_to(
                &mut actual,
                &mut expected,
                (12 + clocks) >> u32::from(double_speed),
                false,
            );
            corruptions += usize::from(actual.mmu.ppu.oam != before);
        }
        if model == Model::default() {
            assert!(corruptions != 0, "the fixtures must trigger OAM corruption");
        }
    }
}

#[test]
fn ei_halt_interrupt_entry_keeps_ie_stack_write_order() {
    for (model, double_speed) in models() {
        for pending in [4, 6] {
            for after_ei in [0x00, 0x76] {
                for split in [false, true] {
                    let mut program = vec![0; 8];
                    program.extend([0xfb, after_ei]);
                    let make = || {
                        let mut gb = machine(model, double_speed, &program);
                        gb.cpu.sp = 0;
                        gb.mmu.ie_reg = 4;
                        gb.mmu.if_reg = pending;
                        gb
                    };
                    let mut actual = make();
                    let mut expected = make();
                    run_to(
                        &mut actual,
                        &mut expected,
                        60 >> u32::from(double_speed),
                        split,
                    );
                    // The return-address high byte changes IE from timer to
                    // STAT between stack writes, cancelling or hijacking entry.
                    assert_eq!(actual.mmu.ie_reg, 2);
                    assert_eq!(actual.cpu.sp, 0xfffe);
                    assert_eq!(actual.cpu.pc, if pending == 4 { 0 } else { 0x48 });
                    assert!(!actual.cpu.halted);
                }
            }
        }
    }
}

#[test]
fn timer_halt_and_hram_dma_interleave_with_queued_audio() {
    for (model, double_speed) in models() {
        let make = || {
            let mut prefix = vec![0; 10];
            prefix.extend([0xc3, 0x80, 0xff]);
            let mut gb = machine(model, double_speed, &prefix);
            let mut code = vec![0xf3, 0xaf, 0xe0, 0x0f]; // DI; clear IF.
            code.extend([0; 12]);
            code.extend([
                0xfb, 0x76, 0xf3, // EI; HALT; DI after the timer handler.
                0x3e, 0xc0, 0xe0, 0x46, // OAM DMA starts from HRAM.
                0x06, 0x60, 0x05, 0x20, 0xfd, // Wait past the DMA endpoint.
                0xf0, 0x04, 0xea, 0x82, 0xc0, // Observe DIV after the wait.
                0x3e, 0x42, 0xe0, 0x13, // Observe pending APU clocks.
                0xc3, 0x80, 0xff,
            ]);
            gb.mmu.hram[..code.len()].copy_from_slice(&code);
            gb.mmu.ie_reg = 4;
            gb.mmu.write_byte(0xff06, 0xf0);
            gb.mmu.write_byte(0xff05, 0xfc);
            gb.mmu.write_byte(0xff07, 5);
            gb
        };
        let mut actual = make();
        let mut expected = make();
        let before = actual.mmu.ppu.oam;
        let audio = actual.mmu.apu.enable_output(48_000);
        let reference_audio = expected.mmu.apu.enable_output(48_000);
        let mut samples = 0;
        for iteration in 0..64 {
            compare_run(
                &mut actual,
                &mut expected,
                [0, 1, 3, 17, 255, 4096][iteration % 6],
            );
            samples += drain_equal(&audio, &reference_audio);
            actual.mmu.ppu.clear_frame_flag();
            expected.mmu.ppu.clear_frame_flag();
        }
        compare_saved_observations(&actual, &expected);
        assert_ne!(actual.mmu.wram[0][0x80], 0, "timer handler was not reached");
        assert_ne!(actual.mmu.wram[0][0x82], 0, "DMA wait did not finish");
        assert_ne!(actual.mmu.ppu.oam, before, "DMA was not exercised");
        assert!(samples > 0, "audio observations were not exercised");
    }
}
