//! Peripheral observations must agree with ordinary instruction stepping.
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, Model},
};

const ENTRY: usize = 0x200;
const RTC_SECOND: u32 = 4_194_304;

fn machine(model: Model, double_speed: bool, mapper: u8, ram: u8, program: &[u8]) -> GameBoy {
    let mut rom = vec![0; 0x10000];
    rom[0x143] = if model.is_cgb() { 0x80 } else { 0 };
    rom[0x147] = mapper;
    rom[0x149] = ram;
    if mapper == 0xbc {
        rom[0x149] = 0xc1;
        rom[0x14a] = 0x65;
        rom[0x150..0x154].copy_from_slice(&[1, 0, 1, 0x0c]);
    }
    rom[ENTRY..ENTRY + program.len()].copy_from_slice(program);
    for vector in [0x40, 0x48, 0x50, 0x58, 0x60] {
        rom[vector..vector + 3].copy_from_slice(&[0xc3, 0x80, 0x01]);
    }
    // Count delivered interrupts without changing the interrupted registers.
    rom[0x180..0x18a]
        .copy_from_slice(&[0xf5, 0xfa, 0x80, 0xc0, 0x3c, 0xea, 0x80, 0xc0, 0xf1, 0xd9]);
    let mut gb = GameBoy::new(model);
    gb.mmu.load_cart(Cartridge::from_bytes(rom));
    gb.cpu.pc = ENTRY as u16;
    gb.cpu.sp = 0xdffe;
    gb.cpu.double_speed = double_speed;
    gb.mmu.key1 = if double_speed { 0x80 } else { 0 };
    gb.cpu.ime = true;
    gb.mmu.ie_reg = 0;
    gb.mmu.if_reg = 0;
    gb.mmu.write_byte(0xff40, 0);
    gb.mmu.wram[0].fill(0);
    gb
}

fn write(program: &mut Vec<u8>, address: u16, value: u8) {
    program.extend([0x3e, value, 0xea, address as u8, (address >> 8) as u8]);
}

fn read_into(program: &mut Vec<u8>, address: u16, destination: u16) {
    program.extend([
        0xfa,
        address as u8,
        (address >> 8) as u8,
        0xea,
        destination as u8,
        (destination >> 8) as u8,
    ]);
}

fn repeat(program: &mut Vec<u8>) {
    program.extend([0xc3, ENTRY as u8, (ENTRY >> 8) as u8]);
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
    assert_eq!(actual.mmu.wram, expected.mmu.wram);
    assert_eq!(actual.mmu.hram, expected.mmu.hram);
    assert_eq!(actual.mmu.ppu.oam, expected.mmu.ppu.oam);
    assert_eq!(actual.mmu.ppu.vram, expected.mmu.ppu.vram);
    assert_eq!(
        serde_json::to_value(&actual.mmu.serial).unwrap(),
        serde_json::to_value(&expected.mmu.serial).unwrap()
    );
    assert_eq!(
        actual.mmu.apu.debug_boot_snapshot(),
        expected.mmu.apu.debug_boot_snapshot()
    );
}

#[test]
fn timer_batches_preserve_mmio_interrupts_div_audio_and_serial() {
    for model in [
        Model::default(),
        Model::Cgb(CgbRevision::RevB),
        Model::Cgb(CgbRevision::RevE),
    ] {
        for double_speed in [false, true] {
            if !model.is_cgb() && double_speed {
                continue;
            }
            for selection in 0..4 {
                let mut program = vec![0; 12];
                write(&mut program, 0xff07, 4 | selection);
                write(&mut program, 0xff05, 0xfe);
                write(&mut program, 0xff06, 0xfc);
                program.extend([0; 11]);
                read_into(&mut program, 0xff04, 0xc000);
                read_into(&mut program, 0xff05, 0xc001);
                write(&mut program, 0xff14, 0x87);
                write(&mut program, 0xff04, 0);
                write(&mut program, 0xff01, 0x69);
                write(
                    &mut program,
                    0xff02,
                    if model.is_cgb() { 0x83 } else { 0x81 },
                );
                program.push(0x76); // HALT until a timer or serial interrupt.
                read_into(&mut program, 0xff02, 0xc002);
                read_into(&mut program, 0xff0f, 0xc003);
                write(&mut program, 0xff07, 0);
                write(&mut program, 0xff06, 0xfd);
                repeat(&mut program);
                let make = || {
                    let mut gb = machine(model, double_speed, 0, 0, &program);
                    gb.mmu.ie_reg = 0x0c;
                    gb.mmu.timer.div = 0xfff4;
                    gb.mmu.write_byte(0xff26, 0x80);
                    gb.mmu.write_byte(0xff24, 0x77);
                    gb.mmu.write_byte(0xff25, 0x11);
                    gb.mmu.write_byte(0xff12, 0xf3);
                    gb
                };
                let mut actual = make();
                let mut expected = make();
                let audio = actual.mmu.apu.enable_output(48_000);
                let reference_audio = expected.mmu.apu.enable_output(48_000);
                for iteration in 0..4096 {
                    let budget = [0, 1, 3, 17, 63, 256, 1024, 4096][iteration % 8];
                    compare_run(&mut actual, &mut expected, budget);
                    loop {
                        let sample = audio.pop_stereo();
                        assert_eq!(sample, reference_audio.pop_stereo());
                        if sample.is_none() {
                            break;
                        }
                    }
                    // Active serial transfers deliberately limit a bounded
                    // call to one instruction, so the slow timer selection
                    // needs more calls before its first interrupt arrives.
                    if iteration >= 95 && actual.mmu.wram[0][0x80] != 0 {
                        break;
                    }
                }
                assert_ne!(actual.mmu.wram[0][0x80], 0, "interrupts were not exercised");
            }
        }
    }
}

fn prime_rtc(gb: &mut GameBoy, tpp1: bool, until_second: u32) {
    if tpp1 {
        gb.mmu.write_byte(0x0003, 0x19);
        gb.mmu.write_byte(0x0003, 0x05);
    } else {
        gb.mmu.write_byte(0x0000, 0x0a);
        gb.mmu.write_byte(0x4000, 8);
    }
    let cart = gb.mmu.cart.as_mut().unwrap();
    let mut cycles = RTC_SECOND - until_second;
    while cycles != 0 {
        let batch = cycles.min(u32::from(u16::MAX)) as u16;
        cart.step_rtc(batch);
        cycles -= u32::from(batch);
    }
}

#[test]
fn rtc_batches_preserve_latch_halt_register_reads_and_state_restore() {
    for (mapper, ram) in [(0x10, 3), (0x10, 4), (0xbc, 1)] {
        let tpp1 = mapper == 0xbc;
        for double_speed in [false, true] {
            let mut program = vec![0; 16];
            if tpp1 {
                write(&mut program, 0x0003, 0x10); // Latch the running clock.
                read_into(&mut program, 0xa003, 0xc010);
                write(&mut program, 0x0003, 0x18); // Stop.
                program.extend([0; 16]);
                write(&mut program, 0xa002, 59);
                write(&mut program, 0x0003, 0x11); // Install the edited latch.
                write(&mut program, 0x0003, 0x19); // Resume.
            } else {
                write(&mut program, 0x6000, 0);
                read_into(&mut program, 0xa000, 0xc010);
                write(&mut program, 0x4000, 0x0c);
                write(&mut program, 0xa000, 0x40); // Halt.
                program.extend([0; 16]);
                write(&mut program, 0xa000, 0); // Resume.
                write(&mut program, 0x4000, 8);
            }
            repeat(&mut program);
            let make = || {
                let mut gb = machine(
                    Model::Cgb(CgbRevision::RevE),
                    double_speed,
                    mapper,
                    ram,
                    &program,
                );
                prime_rtc(&mut gb, tpp1, 32);
                gb
            };
            let mut actual = make();
            let mut expected = make();
            for iteration in 0..24 {
                compare_run(
                    &mut actual,
                    &mut expected,
                    [1, 17, 256, 4096][iteration % 4],
                );
                assert_eq!(
                    serde_json::to_value(actual.mmu.cart.as_ref().unwrap()).unwrap(),
                    serde_json::to_value(expected.mmu.cart.as_ref().unwrap()).unwrap(),
                    "mapper={mapper:x}, double_speed={double_speed}, iteration={iteration}"
                );
                if iteration == 11 {
                    let state = actual.save_state().unwrap();
                    let mut restored = make();
                    restored.load_state(&state).unwrap();
                    actual = restored;
                }
            }
            assert_eq!(
                actual.mmu.wram[0][0x10], 1,
                "RTC rollover/latch was not exercised"
            );
        }
    }
}

#[test]
fn rtc_batches_are_visible_to_dma_and_large_double_speed_scopes() {
    // Execute from HRAM so OAM DMA cannot replace instruction fetches with bus data.
    let hram_program = [
        0x3e, 0xa0, 0xe0, 0x46, // LD A,$A0; LDH [$46],A
        0x06, 96, 0x05, 0x20, 0xfd, // Wait for DMA in either CPU speed.
        0xc3, 0x80, 0xff,
    ];
    for model in [Model::default(), Model::Cgb(CgbRevision::RevE)] {
        for double_speed in [false, true] {
            if !model.is_cgb() && double_speed {
                continue;
            }
            let make = || {
                let mut gb = machine(model, double_speed, 0x10, 3, &[0xc3, 0x00, 0x02]);
                prime_rtc(&mut gb, false, 8);
                gb.mmu.hram[..hram_program.len()].copy_from_slice(&hram_program);
                gb.cpu.pc = 0xff80;
                gb
            };
            let mut actual = make();
            let mut expected = make();
            for budget in [0, 1, 17, 256, 4096, u16::MAX] {
                compare_run(&mut actual, &mut expected, budget);
                assert_eq!(
                    serde_json::to_value(actual.mmu.cart.as_ref().unwrap()).unwrap(),
                    serde_json::to_value(expected.mmu.cart.as_ref().unwrap()).unwrap()
                );
            }
            assert!(actual.mmu.ppu.oam.iter().all(|&byte| byte == 1));
            if double_speed {
                // A full runner scope must retain more than u16::MAX CPU clocks.
                // Let an in-flight transfer finish, without starting another.
                for gb in [&mut actual, &mut expected] {
                    gb.mmu.hram[..3].copy_from_slice(&[0xc3, 0x80, 0xff]);
                    gb.cpu.pc = 0xff80;
                }
                compare_run(&mut actual, &mut expected, u16::MAX);
                assert_eq!(
                    serde_json::to_value(actual.mmu.cart.as_ref().unwrap()).unwrap(),
                    serde_json::to_value(expected.mmu.cart.as_ref().unwrap()).unwrap()
                );
            }
        }
    }
}

#[test]
fn speed_switch_div_resets_and_vram_dma_keep_pending_clocks_ordered() {
    for revision in [CgbRevision::RevB, CgbRevision::RevE] {
        let mut program = vec![0; 12];
        write(&mut program, 0xff4d, 1);
        program.extend([0x10, 0]); // STOP switches CPU speed and resets DIV.
        read_into(&mut program, 0xff04, 0xc020);
        write(&mut program, 0xff14, 0x87);
        for (address, value) in [
            (0xff51, 0xa0),
            (0xff52, 0),
            (0xff53, 0),
            (0xff54, 0),
            (0xff55, 1), // Copy two VRAM DMA blocks from the live RTC window.
        ] {
            write(&mut program, address, value);
        }
        read_into(&mut program, 0xff05, 0xc021);
        write(&mut program, 0xff04, 0);
        repeat(&mut program);
        let make = || {
            let mut gb = machine(Model::Cgb(revision), false, 0x10, 3, &program);
            prime_rtc(&mut gb, false, 8);
            gb.mmu.write_byte(0xff07, 5);
            gb.mmu.write_byte(0xff06, 0xfc);
            gb.mmu.write_byte(0xff12, 0xf3);
            gb
        };
        let mut actual = make();
        let mut expected = make();
        let mut switched = false;
        for iteration in 0..48 {
            compare_run(
                &mut actual,
                &mut expected,
                [1, 17, 256, 1024][iteration % 4],
            );
            switched |= actual.cpu.double_speed;
            assert_eq!(
                serde_json::to_value(actual.mmu.cart.as_ref().unwrap()).unwrap(),
                serde_json::to_value(expected.mmu.cart.as_ref().unwrap()).unwrap()
            );
        }
        assert!(switched, "STOP did not switch CPU speed");
        assert!(actual.mmu.ppu.vram[0][..32].iter().all(|&byte| byte != 0));
    }
}
