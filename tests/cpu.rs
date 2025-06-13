use vibeEmu::{cartridge::Cartridge, cpu::Cpu, mmu::Mmu};

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

    let mut cpu = Cpu::new();
    cpu.pc = 0; // start executing at 0
    let mut mmu = Mmu::new();
    mmu.load_cart(Cartridge { rom: program });

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

    let mut cpu = Cpu::new();
    cpu.pc = 0;
    cpu.sp = 0xC100;
    cpu.ime = true;
    let mut mmu = Mmu::new();
    mmu.load_cart(Cartridge { rom: program });
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

    let mut cpu = Cpu::new();
    cpu.pc = 0;
    cpu.f = 0x00; // Z flag cleared
    let mut mmu = Mmu::new();
    mmu.load_cart(Cartridge {
        rom: program.clone(),
    });
    cpu.step(&mut mmu);

    assert_eq!(cpu.pc, 3);
    assert_eq!(cpu.cycles, 12);

    let mut cpu = Cpu::new();
    cpu.pc = 0;
    cpu.f = 0x80; // Z flag set
    let mut mmu = Mmu::new();
    mmu.load_cart(Cartridge { rom: program });
    cpu.step(&mut mmu);

    assert_eq!(cpu.pc, 2);
    assert_eq!(cpu.cycles, 8);
}

#[test]
fn ei_delay() {
    let program = vec![0xFB, 0x00]; // EI; NOP

    let mut cpu = Cpu::new();
    cpu.pc = 0;
    let mut mmu = Mmu::new();
    mmu.load_cart(Cartridge { rom: program });

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

    let mut cpu = Cpu::new();
    cpu.pc = 0;
    let mut mmu = Mmu::new();
    mmu.load_cart(Cartridge { rom: program });

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

    let mut cpu = Cpu::new();
    cpu.pc = 0;
    let mut mmu = Mmu::new();
    mmu.load_cart(Cartridge { rom: program });

    for _ in 0..4 {
        cpu.step(&mut mmu);
    }

    assert_eq!(cpu.a, 0xFF);
    assert_eq!(cpu.f, 0x00);
}
