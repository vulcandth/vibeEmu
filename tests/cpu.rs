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
