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

use std::fs::File; // Added for file operations
use std::io::{Read, Result}; // Added for file operations and Result type
use std::rc::Rc;
use std::cell::RefCell;

// Use crate:: if gbc_emulator is a library and main.rs is an example or bin.
// If main.rs is part of the library itself (e.g. src/main.rs in a binary crate),
// then `crate::` is appropriate.
use crate::cpu::Cpu;
use crate::bus::Bus;

// Function to load ROM data from a file
fn load_rom_file(path: &str) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    Ok(data)
}

fn main() {
    println!("GBC Emulator starting...");

    // Define the path to the ROM
    let rom_path = "roms/cpu_instrs.gb"; // Path to the ROM file

    // Load the ROM data from the file
    let rom_data = match load_rom_file(rom_path) {
        Ok(data) => data,
        Err(e) => {
            // If the user needs to supply the ROM, it's better to panic here.
            // For automated testing where the ROM might not be present,
            // providing a default empty ROM might be an alternative,
            // but for actual emulation, ROM is essential.
            panic!("Failed to load ROM file '{}': {}", rom_path, e);
        }
    };

    // Create the Bus, wrapped in Rc and RefCell for shared mutable access
    let bus = Rc::new(RefCell::new(Bus::new(rom_data)));

    // Create the Cpu, passing a clone of the Rc-wrapped bus
    let mut cpu = Cpu::new(bus.clone());

    // Initialize CPU state if needed (e.g., PC to ROM start 0x0100 or 0x0000 for some test ROMs)
    // For cpu_instrs.gb, execution typically starts at 0x0100 after the Nintendo logo scroll.
    // However, the boot ROM (if emulated) would handle setting PC to 0x0100.
    // If no boot ROM, we might need to set PC manually.
    // For now, let's assume the ROM itself starts execution correctly or a boot ROM would set PC.
    // Default PC is 0x0000 from Cpu::new(). Some test ROMs might start at 0x0000.
    // cpu_instrs.gb expects to start at 0x0100 if no bootrom is run.
    // Let's set PC to 0x0100 for cpu_instrs.gb compatibility.
    cpu.pc = 0x0100;
    // TODO: Implement proper boot ROM behavior or make this configurable.

    println!("Initial CPU state: PC=0x{:04X}, SP=0x{:04X}, A=0x{:02X}, F=0x{:02X}", cpu.pc, cpu.sp, cpu.a, cpu.f);

    const MAX_STEPS: u64 = 5_000_000; // Define a maximum number of steps for the emulation loop
    const SERIAL_PRINT_INTERVAL: u64 = 100_000;

    for i in 0..MAX_STEPS {
        cpu.step(); // Execute one CPU step

        if cpu.is_halted {
            println!("CPU Halted at step {}.", i + 1);
            break;
        }

        if (i + 1) % SERIAL_PRINT_INTERVAL == 0 {
            let serial_data = bus.borrow().get_serial_output_string();
            if !serial_data.is_empty() {
                println!("Serial Output (step {}):\n{}", i + 1, serial_data);
                // Optionally clear serial_output after printing if desired
                // bus.borrow_mut().serial_output.clear();
            }
            println!("Current CPU state (step {}): PC=0x{:04X}, SP=0x{:04X}, A=0x{:02X}, F=0x{:02X}, B=0x{:02X}, C=0x{:02X}, D=0x{:02X}, E=0x{:02X}, H=0x{:02X}, L=0x{:02X}",
                     i + 1, cpu.pc, cpu.sp, cpu.a, cpu.f, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l);
        }

        if i == MAX_STEPS - 1 {
            println!("Emulation finished after {} steps (max steps reached).", MAX_STEPS);
        }
    }

    // Final serial output check
    let serial_data = bus.borrow().get_serial_output_string();
    if !serial_data.is_empty() {
        println!("Final Serial Output:\n{}", serial_data);
    }

    println!("Final CPU state: PC=0x{:04X}, is_halted: {}", cpu.pc, cpu.is_halted);
    println!("GBC Emulator finished.");
}
