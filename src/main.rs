// Declare modules if they are in separate files in the same directory (e.g., src/)
// and not part of a library crate already.
mod apu;
mod bus;
mod cpu;
mod memory;
mod ppu;

// Assuming these other modules exist from the initial problem description context
// and might be needed for a complete build, though not directly used in this step's main()
mod interrupts;
mod joypad;
mod serial;
mod timer;


use std::rc::Rc;
use std::cell::RefCell;

// Use crate:: if gbc_emulator is a library and main.rs is an example or bin.
// If main.rs is part of the library itself (e.g. src/main.rs in a binary crate),
// then `crate::` is appropriate.
use crate::cpu::Cpu;
use crate::bus::Bus;

fn main() {
    println!("GBC Emulator starting...");

    // Create the Bus, wrapped in Rc and RefCell for shared mutable access
    let bus = Rc::new(RefCell::new(Bus::new()));

    // Create the Cpu, passing a clone of the Rc-wrapped bus
    let mut cpu = Cpu::new(bus.clone());

    // --- Test execution ---
    // Manually put some opcodes in WRAM via the bus for the CPU to fetch.
    // WRAM starts at 0xC000.
    // NOP (0x00) at 0xC000
    // HALT (0x76) at 0xC001
    bus.borrow_mut().write_byte(0xC000, 0x00); // NOP
    bus.borrow_mut().write_byte(0xC001, 0x76); // HALT

    // Set CPU's program counter to start execution from WRAM for this test
    cpu.pc = 0xC000;

    println!("Initial CPU state: PC=0x{:04X}, A=0x{:02X}", cpu.pc, cpu.a);
    println!("Memory at 0xC000 (WRAM): 0x{:02X}", bus.borrow().read_byte(0xC000));
    println!("Memory at 0xC001 (WRAM): 0x{:02X}", bus.borrow().read_byte(0xC001));

    // Simplified fetch-decode-execute cycle for testing
    // We'll manually call the CPU methods for NOP and HALT for this test.
    // A full emulator would have a loop and a large match statement for opcodes.

    if !cpu.is_halted {
        // 1. Fetch first opcode (NOP)
        // The CPU's internal fetch mechanism isn't used here yet; we're driving manually.
        // In a real scenario, cpu.tick() or cpu.step() would handle this.
        let opcode1 = bus.borrow().read_byte(cpu.pc);
        println!("Fetched opcode 0x{:02X} from PC=0x{:04X}", opcode1, cpu.pc);

        if opcode1 == 0x00 { // NOP
            cpu.nop(); // This should advance PC to 0xC001
            println!("Executed NOP. New CPU state: PC=0x{:04X}", cpu.pc);

            // 2. Fetch second opcode (HALT)
            if cpu.pc == 0xC001 && !cpu.is_halted { // Check if PC advanced correctly
                let opcode2 = bus.borrow().read_byte(cpu.pc);
                println!("Fetched opcode 0x{:02X} from PC=0x{:04X}", opcode2, cpu.pc);

                if opcode2 == 0x76 { // HALT
                    cpu.halt(); // This should advance PC to 0xC002 and set is_halted
                     println!("Executed HALT. New CPU state: PC=0x{:04X}, is_halted: {}", cpu.pc, cpu.is_halted);
                } else {
                    println!("Expected HALT (0x76) but got 0x{:02X}", opcode2);
                }
            }
        } else if opcode1 == 0x76 { // HALT (if PC somehow started at 0xC001)
            cpu.halt();
            println!("Executed HALT directly. CPU state: PC=0x{:04X}, is_halted: {}", cpu.pc, cpu.is_halted);
        } else {
            println!("Expected NOP (0x00) but got 0x{:02X}", opcode1);
        }
    }

    println!("Final CPU state: PC=0x{:04X}, is_halted: {}", cpu.pc, cpu.is_halted);
    println!("Memory at 0xC000 (WRAM) after execution: 0x{:02X}", bus.borrow().read_byte(0xC000));
    println!("Memory at 0xC001 (WRAM) after execution: 0x{:02X}", bus.borrow().read_byte(0xC001));

    // Example: Test reading from HRAM (FF80-FFFE) via Bus then Memory
    bus.borrow_mut().write_byte(0xFF80, 0xAA);
    bus.borrow_mut().write_byte(0xFFFE, 0xBB);
    println!("Value at HRAM 0xFF80: 0x{:02X}", bus.borrow().read_byte(0xFF80));
    println!("Value at HRAM 0xFFFE: 0x{:02X}", bus.borrow().read_byte(0xFFFE));

    // Example: Test PPU/APU placeholder calls via Bus
    println!("Attempting PPU read at 0x8000: 0x{:02X}", bus.borrow().read_byte(0x8000)); // VRAM
    bus.borrow_mut().write_byte(0x8000, 0xCC);
    println!("Attempting APU read at 0xFF10: 0x{:02X}", bus.borrow().read_byte(0xFF10)); // APU Reg
    bus.borrow_mut().write_byte(0xFF10, 0xDD);

    println!("GBC Emulator finished basic test.");
}
