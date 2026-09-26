use vibe_emu_core::{
    cartridge::Cartridge,
    cpu::Cpu,
    hardware::{CgbRevision, Model},
    mmu::Mmu,
};

#[test]
fn ei_halt_dispatches_without_waiting_for_a_second_interrupt() {
    for model in [Model::default(), Model::Cgb(CgbRevision::RevE)] {
        for pending in [2, 3] {
            let mut rom = vec![0; 0x8000];
            rom[0x100..0x103].copy_from_slice(&[0xFB, 0x76, 0x07]); // EI; HALT; RLCA
            for vector in [0x40, 0x48] {
                rom[vector..vector + 2].copy_from_slice(&[0x3C, 0xC9]); // INC A; RET
            }
            let mut cpu = Cpu::new(model);
            let mut mmu = Mmu::new(model);
            mmu.load_cart(Cartridge::from_bytes(rom));
            mmu.write_byte(0xFF40, 0); // Only explicitly requested interrupts.
            mmu.ie_reg = 3;
            mmu.if_reg = pending;
            cpu.a = 0x20;

            cpu.step(&mut mmu); // EI
            cpu.step(&mut mmu); // HALT and interrupt dispatch
            assert!(
                !cpu.halted,
                "the interrupt handler must execute immediately"
            );
            assert_eq!(cpu.pc, if pending == 2 { 0x48 } else { 0x40 });
            assert_eq!(mmu.read_byte(cpu.sp), 1); // Return to HALT at $0101.
            cpu.step(&mut mmu); // INC A
            assert_eq!(cpu.a, 0x21);
            cpu.step(&mut mmu); // RET
            assert_eq!(cpu.pc, 0x101);
            cpu.step(&mut mmu); // Re-execute HALT.
            if pending == 2 {
                assert!(cpu.halted);
                mmu.if_reg = 1;
                cpu.step(&mut mmu); // Wake without dispatch (IME is off).
                cpu.step(&mut mmu); // RLCA, once.
                assert_eq!(cpu.a, 0x42);
            } else {
                assert!(!cpu.halted);
                cpu.step(&mut mmu); // Remaining IRQ causes the HALT bug.
                cpu.step(&mut mmu); // RLCA is fetched twice.
                assert_eq!(cpu.a, 0x84);
            }
        }
    }
}

#[test]
fn simple_program() {
    // Program that loads values and stores to RAM then jumps
    let program = vec![
        0x06, 0x12, // LD B,0x12
        0x0E, 0x34, // LD C,0x34
        0x26, 0xC0, // LD H,0xC0
        0x2E, 0x00, // LD L,0x00
        0x3E, 0x56, // LD A,0x56
        0x77, // LD (HL),A
        0xAF, // XOR A
        0xC3, 0x10, 0x00, // JP 0x0010
        0x00, // padding
        0x00, // 0x0010: NOP
    ];

    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0; // start executing at 0
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program));

    for _ in 0..8 {
        cpu.step(&mut mmu);
    }

    assert_eq!(cpu.b, 0x12);
    assert_eq!(cpu.c, 0x34);
    assert_eq!(cpu.a, 0x00); // XOR A cleared A
    assert_eq!(mmu.read_byte(0xC000), 0x56);
    assert_eq!(cpu.pc, 0x0010);
    assert_eq!(cpu.cycles, 68);
}

#[test]
fn interrupt_handling() {
    let program = vec![0x00]; // NOP

    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    cpu.sp = 0xC100;
    cpu.ime = true;
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program));
    mmu.if_reg = 0x01;
    mmu.ie_reg = 0x01;

    cpu.step(&mut mmu);

    assert_eq!(cpu.pc, 0x0040);
    assert_eq!(mmu.if_reg & 0x01, 0);
    assert_eq!(cpu.sp, 0xC0FE);
    assert_eq!(mmu.read_byte(0xC0FF), 0x00);
    assert_eq!(mmu.read_byte(0xC0FE), 0x01);
    assert_eq!(cpu.cycles, 24); // 4 for NOP + 20 for interrupt
}

#[test]
fn jr_nz_cycles() {
    // JR NZ should take 12 cycles when branch taken and 8 when not
    let program = vec![0x20, 0x01, 0x00];

    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    cpu.f = 0x00; // Z flag cleared
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program.clone()));
    cpu.step(&mut mmu);

    assert_eq!(cpu.pc, 3);
    assert_eq!(cpu.cycles, 12);

    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    cpu.f = 0x80; // Z flag set
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program));
    cpu.step(&mut mmu);

    assert_eq!(cpu.pc, 2);
    assert_eq!(cpu.cycles, 8);
}

#[test]
fn ei_delay() {
    let program = vec![0xFB, 0x00]; // EI; NOP

    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program));

    cpu.step(&mut mmu); // EI
    assert!(!cpu.ime);
    cpu.step(&mut mmu); // NOP
    assert!(cpu.ime);
}

#[test]
fn ld_rr_instructions() {
    let program = vec![
        0x01, 0x00, 0xC0, // LD BC,0xC000
        0x11, 0x00, 0xC1, // LD DE,0xC100
        0x21, 0x00, 0xC0, // LD HL,0xC000
        0x31, 0xFE, 0xFF, // LD SP,0xFFFE
        0x3E, 0x11, // LD A,0x11
        0x02, // LD (BC),A
        0x0A, // LD A,(BC)
        0x12, // LD (DE),A
        0x1A, // LD A,(DE)
        0x22, // LDI (HL),A
        0x2A, // LDI A,(HL)
        0x32, // LDD (HL),A
        0x3A, // LDD A,(HL)
        0x7E, // LD A,(HL)
        0x70, // LD (HL),B
    ];

    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program));

    for _ in 0..15 {
        cpu.step(&mut mmu);
    }

    assert_eq!(mmu.read_byte(0xC000), cpu.b);
    assert_eq!(mmu.read_byte(0xC100), 0x11);
    assert_eq!(cpu.a, 0x11);
    assert_eq!(cpu.sp, 0xFFFE);
    assert_eq!(cpu.get_hl(), 0xC000);
}

#[test]
fn alu_immediate_ops() {
    let program = vec![
        0x3E, 0x0F, // LD A,0x0F
        0xC6, 0x01, // ADD A,0x01 -> A=0x10
        0xD6, 0x10, // SUB 0x10 -> A=0x00
        0xEE, 0xFF, // XOR 0xFF -> A=0xFF
    ];

    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program));

    for _ in 0..4 {
        cpu.step(&mut mmu);
    }

    assert_eq!(cpu.a, 0xFF);
    assert_eq!(cpu.f, 0x00);
}

#[test]
fn alu_register_ops() {
    let program = vec![
        0x3E, 0x10, // LD A,0x10
        0x06, 0x05, // LD B,0x05
        0x80, // ADD A,B -> 0x15
        0x90, // SUB B -> 0x10
        0xA0, // AND B -> 0x00
        0x3E, 0x0F, // LD A,0x0F
        0xA8, // XOR B -> 0x0A
        0xB0, // OR B -> 0x0F
        0xB8, // CP B
        0x21, 0x00, 0xC0, // LD HL,0xC000
        0x36, 0x12, // LD (HL),0x12
        0x00, // NOP
    ];

    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program));

    for _ in 0..12 {
        cpu.step(&mut mmu);
    }

    assert_eq!(cpu.a, 0x0F);
    assert_eq!(cpu.b, 0x05);
    assert_eq!(cpu.f, 0x40);
    assert_eq!(mmu.read_byte(0xC000), 0x12);
    assert_eq!(cpu.cycles, 76);
}

#[test]
fn halt_bug() {
    // DI; HALT; LD A,0x12
    let program = vec![0xF3, 0x76, 0x3E, 0x12];
    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program));
    mmu.if_reg = 0x01;
    mmu.ie_reg = 0x01;

    cpu.step(&mut mmu); // DI
    cpu.step(&mut mmu); // HALT -> triggers halt bug
    cpu.step(&mut mmu); // LD A,(bugged immediate)

    assert_eq!(cpu.a, 0x3E); // immediate read again
    assert_eq!(cpu.pc, 3);
}

#[test]
fn stop_speed_switch() {
    // STOP 0x00 ; NOP
    let program = vec![0x10, 0x00, 0x00];
    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    let mut mmu = Mmu::new(Model::Cgb(CgbRevision::default()));
    mmu.load_cart(Cartridge::from_bytes(program));
    mmu.key1 = 0x01; // request speed switch

    cpu.step(&mut mmu); // STOP

    assert_eq!(mmu.key1 & 0x81, 0x80);
    assert!(cpu.double_speed);
    // A speed-switching STOP prefetches the following byte as an opcode.
    assert_eq!(cpu.pc, 1);
    cpu.step(&mut mmu);
    assert_eq!(cpu.pc, 2);
}

#[test]
fn speed_switch_stall_runs_timer_with_lcd_disabled() {
    for double_speed in [false, true] {
        let model = Model::Cgb(CgbRevision::RevE);
        let mut cpu = Cpu::new(model);
        let mut mmu = Mmu::new(model);
        cpu.pc = 0;
        cpu.double_speed = double_speed;
        mmu.load_cart(Cartridge::from_bytes(vec![0x10, 0x00, 0x00]));
        mmu.write_byte(0xFF40, 0);
        mmu.ie_reg = 0;
        mmu.key1 = if double_speed { 0x81 } else { 0x01 };
        mmu.timer.div = 0;
        mmu.timer.tima = 0;
        mmu.timer.tac = 4; // 4096 Hz: one increment per 1024 CPU clocks.

        cpu.step(&mut mmu);

        assert_eq!(cpu.double_speed, !double_speed);
        // AGE's spsw-tima measures 128 increments during the wait. DIV
        // returns to zero because it wraps twice, rather than being frozen.
        assert_eq!(mmu.timer.tima, 128);
        assert_eq!(mmu.timer.div, 0);
        assert_eq!(
            cpu.cycles,
            if double_speed {
                0x20000 + 4
            } else {
                0x10000 + 8
            }
        );
    }
}

#[test]
fn pending_speed_switch_interrupt_uses_revision_specific_entry_timing() {
    for revision in [CgbRevision::RevB, CgbRevision::RevC, CgbRevision::RevE] {
        for double_speed in [false, true] {
            for bit in 0..5 {
                let model = Model::Cgb(revision);
                let mut cpu = Cpu::new(model);
                let mut mmu = Mmu::new(model);
                let mut rom = vec![0; 0x8000];
                rom[..3].copy_from_slice(&[0xfb, 0x10, 0]); // EI; STOP padding
                let vector = 0x40 + bit * 8;
                rom[vector] = 0xc9; // RET
                mmu.load_cart(Cartridge::from_bytes(rom));
                mmu.write_byte(0xff40, 0);
                cpu.pc = 0;
                cpu.sp = 0xd000;
                cpu.ime = false;
                cpu.double_speed = double_speed;
                mmu.key1 = if double_speed { 0x81 } else { 1 };
                mmu.ie_reg = 1 << bit;
                mmu.if_reg = 1 << bit;

                cpu.step(&mut mmu); // EI defers dispatch through STOP.
                assert_eq!(cpu.pc, 1);
                cpu.step(&mut mmu);

                assert_eq!(cpu.pc, vector as u16);
                assert_eq!(cpu.double_speed, !double_speed);
                assert_eq!(
                    mmu.timer.div,
                    if revision == CgbRevision::RevE {
                        20
                    } else {
                        16
                    }
                );
                assert_eq!(
                    mmu.read_byte(cpu.sp),
                    2,
                    "pending IRQ returns to the prefetched byte"
                );
                assert_eq!(mmu.if_reg & mmu.ie_reg & 0x1f, 0);
                cpu.step(&mut mmu); // RET
                assert_eq!(cpu.pc, 2);
                cpu.step(&mut mmu); // The padding byte is fetched as NOP.
                assert_eq!(cpu.pc, 3);
            }
        }
    }
}

#[test]
fn timer_interrupt_wakes_speed_switch_and_returns_after_stop_padding() {
    let model = Model::Cgb(CgbRevision::RevE);
    let mut cpu = Cpu::new(model);
    let mut mmu = Mmu::new(model);
    let mut rom = vec![0; 0x8000];
    rom[0] = 0x10; // STOP, with its second byte at address 1.
    rom[0x50] = 0xC9; // Timer handler: RET.
    mmu.load_cart(Cartridge::from_bytes(rom));
    cpu.pc = 0;
    cpu.sp = 0xD000;
    cpu.ime = true;
    mmu.write_byte(0xFF40, 0);
    mmu.write_byte(0xFF04, 0);
    mmu.write_byte(0xFF05, 0xFF);
    mmu.write_byte(0xFF06, 0);
    mmu.write_byte(0xFF07, 5);
    mmu.ie_reg = 4;
    mmu.if_reg = 0;
    mmu.key1 = 1;

    cpu.step(&mut mmu);

    assert!(cpu.double_speed);
    assert_eq!(cpu.pc, 0x50);
    assert_eq!(
        mmu.timer.div, 28,
        "4 reload clocks, 4 wake clocks, 20 entry clocks"
    );
    assert_eq!(
        mmu.read_byte(cpu.sp),
        2,
        "the return address skips STOP padding"
    );
    assert_eq!(mmu.read_byte(cpu.sp + 1), 0);
    cpu.step(&mut mmu);
    assert_eq!(cpu.pc, 2);
}

#[test]
fn gdma_stall_advances_cpu_div() {
    // While a CGB GDMA stall is active, the CPU is blocked from executing
    // instructions, but time still advances (including the CPU divider).
    let program = vec![0x00]; // NOP (won't execute during the stall)
    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    let mut mmu = Mmu::new(Model::Cgb(CgbRevision::default()));
    mmu.load_cart(Cartridge::from_bytes(program));

    // Point GDMA at WRAM0 -> VRAM.
    mmu.wram[0][0] = 0xAB;
    mmu.write_byte(0xFF51, 0xC0); // src hi
    mmu.write_byte(0xFF52, 0x00); // src lo (low nibble ignored)
    mmu.write_byte(0xFF53, 0x80); // dst hi
    mmu.write_byte(0xFF54, 0x00); // dst lo (low nibble ignored)

    mmu.timer.div = 0;
    let div_before = mmu.timer.div;

    // Start GDMA for 1 block (0x10 bytes). Bit7=0 => GDMA.
    mmu.write_byte(0xFF55, 0x00);
    assert!(mmu.gdma_active());

    // In normal speed, each block stalls for 8 M-cycles. Each M-cycle
    // advances the CPU divider by 4.
    for _ in 0..8 {
        cpu.step(&mut mmu);
    }
    assert!(!mmu.gdma_active());
    assert_eq!(mmu.timer.div.wrapping_sub(div_before), 8 * 4);
}

#[test]
fn double_speed_timer_scaling() {
    // STOP to switch speed, then NOP
    let program = vec![0x10, 0x00, 0x00];
    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    let mut mmu = Mmu::new(Model::Cgb(CgbRevision::default()));
    mmu.load_cart(Cartridge::from_bytes(program));
    mmu.key1 = 0x01;

    cpu.step(&mut mmu); // STOP
    let cpu_div_before = mmu.timer.div;
    let dot_div_before = mmu.dot_div;
    cpu.step(&mut mmu); // NOP

    assert!(cpu.double_speed);
    // In double speed, the dot clock advances half as many cycles per M-cycle, but
    // DIV/TIMA remain derived from the CPU clock domain.
    assert_eq!(mmu.timer.div.wrapping_sub(cpu_div_before), 4);
    // Dot clock advances by 2 cycles for a 1 M-cycle instruction.
    assert_eq!(mmu.dot_div.wrapping_sub(dot_div_before), 2);
}

#[test]
fn stop_resets_div_and_pauses() {
    // STOP; NOP
    let program = vec![0x10, 0x00, 0x00];
    let mut cpu = Cpu::new(Model::default());
    cpu.pc = 0;
    let mut mmu = Mmu::new(Model::default());
    mmu.load_cart(Cartridge::from_bytes(program));
    mmu.timer.div = 0x1234;

    cpu.step(&mut mmu); // STOP
    assert!(cpu.stopped);
    assert_eq!(mmu.timer.div, 0);
    let div_after = mmu.timer.div;

    // Stepping while stopped should not advance divider
    cpu.step(&mut mmu);
    assert_eq!(mmu.timer.div, div_after);

    // Resume and execute NOP
    cpu.stopped = false;
    cpu.step(&mut mmu);
    assert_eq!(mmu.timer.div, div_after.wrapping_add(4));
}
