// src/bus.rs

use crate::apu::Apu;
use crate::memory::Memory;
use crate::ppu::Ppu;

pub struct Bus {
    pub memory: Memory,
    pub ppu: Ppu,
    pub apu: Apu,
    // TODO: Add interrupt enable (IE) and interrupt flag (IF) registers here later
    // For now, IF is 0xFF0F and IE is 0xFFFF
    // pub interrupt_enable_register: u8,
    // pub interrupt_flag: u8,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            memory: Memory::new(),
            ppu: Ppu::new(),
            apu: Apu::new(),
            // interrupt_enable_register: 0, // Default value
            // interrupt_flag: 0,            // Default value
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        println!("Bus read at 0x{:04X}", addr);
        match addr {
            0x0000..=0x7FFF => {
                // ROM - Not implemented yet
                // For now, panic or return a fixed value.
                // panic!("Read from ROM not implemented. Address: {:#04X}", addr);
                0xFF
            }
            0x8000..=0x9FFF => self.ppu.read_byte(addr),
            0xA000..=0xBFFF => {
                // External RAM (Cartridge RAM) - Not implemented yet
                // panic!("Read from External RAM not implemented. Address: {:#04X}", addr);
                0xFF
            }
            0xC000..=0xDFFF => self.memory.read_byte(addr), // WRAM
            0xE000..=0xFDFF => {
                // Echo RAM (mirror of 0xC000 - 0xDDFF)
                // The last 0x200 bytes of this range (0xFE00 - 0xFDFF) actually mirror 0xC000-0xDFFF
                // So 0xFDFF mirrors 0xDDFF
                let mirrored_addr = addr - 0x2000;
                self.memory.read_byte(mirrored_addr)
            }
            0xFE00..=0xFE9F => self.ppu.read_byte(addr), // OAM
            0xFEA0..=0xFEFF => {
                // Unusable memory
                0xFF
            }
            0xFF00..=0xFF7F => {
                // I/O Registers
                match addr {
                    0xFF00 => {
                        // Joypad - Placeholder
                        0xFF
                    }
                    0xFF01..=0xFF02 => {
                        // Serial - Placeholder
                        0xFF
                    }
                    0xFF04..=0xFF07 => {
                        // Timer - Placeholder
                        0xFF
                    }
                    // 0xFF0F => self.interrupt_flag, // IF (Interrupt Flag) - Placeholder
                    0xFF0F => 0xFF, // IF (Interrupt Flag) - Placeholder
                    0xFF10..=0xFF3F => self.apu.read_byte(addr), // APU registers
                    0xFF40..=0xFF4F => self.ppu.read_byte(addr), // PPU registers
                    // Other I/O registers - Placeholder
                    _ => 0xFF,
                }
            }
            0xFF80..=0xFFFE => self.memory.read_byte(addr), // HRAM
            // 0xFFFF => self.interrupt_enable_register, // IE (Interrupt Enable Register)
            0xFFFF => 0xFF, // IE (Interrupt Enable Register) - Placeholder
            // _ => {
            //     // This should ideally not be reached if all ranges are covered
            //     // panic!("Read from unhandled Bus address: {:#04X}", addr);
            //     0xFF // Default for any unmapped reads not explicitly handled
            // }
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        println!("Bus write at 0x{:04X} with value 0x{:02X}", addr, value);
        match addr {
            0x0000..=0x7FFF => {
                // ROM - Writes are generally ignored or have special behavior
                // For now, do nothing.
            }
            0x8000..=0x9FFF => self.ppu.write_byte(addr, value), // VRAM
            0xA000..=0xBFFF => {
                // External RAM (Cartridge RAM) - Not implemented yet
                // For now, do nothing.
            }
            0xC000..=0xDFFF => self.memory.write_byte(addr, value), // WRAM
            0xE000..=0xFDFF => {
                // Echo RAM (mirror of 0xC000 - 0xDDFF)
                let mirrored_addr = addr - 0x2000;
                self.memory.write_byte(mirrored_addr, value)
            }
            0xFE00..=0xFE9F => self.ppu.write_byte(addr, value), // OAM
            0xFEA0..=0xFEFF => {
                // Unusable memory - Do nothing
            }
            0xFF00..=0xFF7F => {
                // I/O Registers
                match addr {
                    0xFF00 => {
                        // Joypad - Placeholder
                    }
                    0xFF01..=0xFF02 => {
                        // Serial - Placeholder
                    }
                    0xFF04..=0xFF07 => {
                        // Timer - Placeholder
                    }
                    // 0xFF0F => self.interrupt_flag = value, // IF (Interrupt Flag)
                    0xFF0F => {} // IF (Interrupt Flag) - Placeholder
                    0xFF10..=0xFF3F => self.apu.write_byte(addr, value), // APU registers
                    0xFF40..=0xFF4F => self.ppu.write_byte(addr, value), // PPU registers
                    // Other I/O registers - Placeholder
                    _ => {}
                }
            }
            0xFF80..=0xFFFE => self.memory.write_byte(addr, value), // HRAM
            // 0xFFFF => self.interrupt_enable_register = value, // IE (Interrupt Enable Register)
            0xFFFF => {} // IE (Interrupt Enable Register) - Placeholder
            // _ => {
            //     // This should ideally not be reached if all ranges are covered
            //     // panic!("Write to unhandled Bus address: {:#04X}", addr);
            // }
        }
    }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu; // Assuming cpu.rs is in crate root
    use std::rc::Rc;
    use std::cell::RefCell;

    fn setup_test_env() -> (Cpu, Rc<RefCell<Bus>>) {
        let bus = Rc::new(RefCell::new(Bus::new()));
        let cpu = Cpu::new(bus.clone());
        (cpu, bus)
    }

    #[test]
    fn test_cpu_write_to_wram_via_bus() {
        let (mut cpu, bus) = setup_test_env();

        // Test LD A, n
        // cpu.ld_a_n(0xAB) is not a method in the current cpu.rs, it takes parameters from opcode
        // Direct CPU method call for LD A, 0xAB (0x3E, 0xAB)
        // For this test, we'll just set cpu.a directly.
        cpu.a = 0xAB;
        assert_eq!(cpu.a, 0xAB);

        // Test LD (nn), A where nn is a WRAM address
        // Let nn = 0xC100
        let wram_addr = 0xC100;
        cpu.pc = 0x0100; // Dummy PC for the instruction itself for PC increment logic

        // Simulate LD (0xC100), A
        // The method ld_nn_mem_a takes addr_lo, addr_hi
        cpu.ld_nn_mem_a((wram_addr & 0xFF) as u8, (wram_addr >> 8) as u8);

        assert_eq!(bus.borrow().read_byte(wram_addr), 0xAB, "Value in WRAM via bus is incorrect");
        assert_eq!(cpu.pc, 0x0100 + 3, "PC increment for LD (nn),A is wrong");
    }

    #[test]
    fn test_cpu_read_from_wram_via_bus() {
        let (mut cpu, bus) = setup_test_env();

        let wram_addr = 0xC200;
        let expected_val = 0xCD;
        bus.borrow_mut().write_byte(wram_addr, expected_val);

        // Test LD A, (nn) where nn is a WRAM address
        cpu.pc = 0x0150; // Dummy PC
        cpu.ld_a_nn_mem((wram_addr & 0xFF) as u8, (wram_addr >> 8) as u8);

        assert_eq!(cpu.a, expected_val, "Value read into A from WRAM via bus is incorrect");
        assert_eq!(cpu.pc, 0x0150 + 3, "PC increment for LD A,(nn) is wrong");
    }

    #[test]
    fn test_cpu_write_to_hram_via_bus() {
        let (mut cpu, bus) = setup_test_env();
        cpu.a = 0xBE;

        // Test LD (HL), A where HL points to HRAM
        // Let HL = 0xFF80 (start of HRAM)
        cpu.h = 0xFF;
        cpu.l = 0x80;
        let hram_addr = 0xFF80;
        cpu.pc = 0x0200;

        cpu.ld_hl_mem_a(); // This uses the write_hl_mem helper

        assert_eq!(bus.borrow().read_byte(hram_addr), 0xBE, "Value in HRAM via bus is incorrect");
        assert_eq!(cpu.pc, 0x0200 + 1, "PC increment for LD (HL),A is wrong");
    }

    #[test]
    fn test_cpu_read_from_hram_via_bus() {
        let (mut cpu, bus) = setup_test_env();

        let hram_addr = 0xFF8A;
        let expected_val = 0xEF;
        bus.borrow_mut().write_byte(hram_addr, expected_val);

        // Test LD A, (HL) where HL points to HRAM
        cpu.h = (hram_addr >> 8) as u8;
        cpu.l = (hram_addr & 0xFF) as u8;
        cpu.pc = 0x0250;

        cpu.ld_a_hl_mem(); // This uses the read_hl_mem helper

        assert_eq!(cpu.a, expected_val, "Value read into A from HRAM via bus is incorrect");
        assert_eq!(cpu.pc, 0x0250 + 1, "PC increment for LD A,(HL) is wrong");
    }

    #[test]
    fn test_cpu_stack_operations_on_wram_via_bus() {
        let (mut cpu, bus) = setup_test_env();

        cpu.sp = 0xDFFF; // Top of WRAM
        cpu.b = 0x12;
        cpu.c = 0x34;
        cpu.pc = 0x0300;

        cpu.push_bc(); // Pushes B then C. SP becomes 0xDFFD.
                       // Memory at 0xDFFE should be B (0x12)
                       // Memory at 0xDFFD should be C (0x34)

        assert_eq!(cpu.sp, 0xDFFD, "SP after PUSH BC is wrong");
        assert_eq!(bus.borrow().read_byte(0xDFFE), 0x12, "Value for B on stack (WRAM) is incorrect");
        assert_eq!(bus.borrow().read_byte(0xDFFD), 0x34, "Value for C on stack (WRAM) is incorrect");
        assert_eq!(cpu.pc, 0x0300 + 1);

        // Now POP DE (values should be what was pushed for BC)
        cpu.pc = 0x0301;
        cpu.pop_de(); // D should get value from stack (0x12), E from (0x34)

        assert_eq!(cpu.d, 0x12, "D after POP DE is incorrect");
        assert_eq!(cpu.e, 0x34, "E after POP DE is incorrect");
        assert_eq!(cpu.sp, 0xDFFF, "SP after POP DE is wrong");
        assert_eq!(cpu.pc, 0x0301 + 1);
    }

    #[test]
    fn test_ppu_io_read_placeholder() {
        // This test just checks if the bus routes to the PPU placeholder
        // It doesn't check for correct PPU behavior, only that the PPU's read_byte is called.
        let (mut cpu, _bus) = setup_test_env(); // bus is not directly used for assert here

        let ppu_lcdc_addr = 0xFF40; // LCDC register

        // LD A, (HL) where HL = 0xFF40
        cpu.h = (ppu_lcdc_addr >> 8) as u8;
        cpu.l = (ppu_lcdc_addr & 0xFF) as u8;

        // The PPU placeholder read_byte returns 0xFF and prints a message.
        // We can't easily check the println! here without more complex test setup.
        // So we'll just check the returned value.
        cpu.ld_a_hl_mem();
        assert_eq!(cpu.a, 0xFF, "Reading from PPU placeholder address should return dummy value");
    }

    #[test]
    fn test_apu_io_write_placeholder() {
        // Similar to PPU, checks routing to APU placeholder.
        let (mut cpu, bus) = setup_test_env();

        let apu_ch1_vol_addr = 0xFF12; // NR12 - Channel 1 Volume & Envelope
        cpu.a = 0xF3; // Value to write

        // LD (HL), A where HL = 0xFF12
        cpu.h = (apu_ch1_vol_addr >> 8) as u8;
        cpu.l = (apu_ch1_vol_addr & 0xFF) as u8;

        // The APU placeholder write_byte prints a message.
        // We can't check the println! easily. This test mainly ensures no panic and it completes.
        // A more advanced test would involve a mock APU or capturing stdout.
        cpu.ld_hl_mem_a();

        // To make the test somewhat useful, we can try reading back.
        // The placeholder APU read should return 0xFF, not what was written.
        let read_back_val = bus.borrow().read_byte(apu_ch1_vol_addr);
        assert_eq!(read_back_val, 0xFF, "Reading from APU placeholder after write should return dummy value");
    }
}
}
