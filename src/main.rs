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

use std::env; // For command-line arguments
use std::fs::File; // Added for file operations
use std::io::{Read, Result}; // Added for file operations and Result type
use std::path::Path; // For path manipulation
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

    // Parse command-line arguments for ROM path
    let args: Vec<String> = env::args().collect();
    let default_rom_path = "roms/cpu_instrs.gb".to_string();
    let rom_path = if args.len() > 1 {
        args[1].clone()
    } else {
        println!("Usage: {} <path_to_rom>", args[0]);
        println!("Defaulting to ROM: {}", default_rom_path);
        // std::process::exit(1); // Optionally exit if no ROM is provided
        default_rom_path
    };
    println!("Loading ROM from: {}", rom_path);


    // Load the ROM data from the file
    let rom_data = match load_rom_file(&rom_path) {
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

    // Test ROM specific configurations
    let test_check_addr = 0x6000; // For Blargg's test status
    let blargg_test_running_magic_val = 0x80; // Blargg's tests write 0x80 here while running.
    // let blargg_test_string_addr = 0x6004; // Start of Blargg's test result string
    // let newer_blargg_test_string_addr = 0xA004; // Some newer tests use this

    const MAX_STEPS: u64 = 30_000_000; // Increased max steps for longer tests
    const SERIAL_PRINT_INTERVAL: u64 = 500_000; // Adjusted for potentially longer tests
    let mut blargg_test_passed = false;
    let mut blargg_test_failed_code: Option<u8> = None;

    for i in 0..MAX_STEPS {
        let m_cycles = cpu.step(); // Execute one CPU step and get M-cycles

        // PPU runs 4 times faster than CPU M-cycles (T-cycles = M-cycles * 4)
        let t_cycles = m_cycles * 4;
        for _ in 0..t_cycles {
            bus.borrow_mut().ppu.tick();
            // TODO: PPU could request VBlank/STAT interrupts here
            // Example: if bus.borrow().ppu.vblank_interrupt_requested() {
            //              bus.borrow_mut().request_interrupt(Interrupt::VBlank);
            //          }
        }

        // Check for Blargg test completion via memory mapped status
        let status = bus.borrow().read_byte(test_check_addr);
        if status != 0x00 && status != blargg_test_running_magic_val {
            println!(
                "Test status at {:#06X}: {:#04X}. Assuming test complete.",
                test_check_addr, status
            );
            if status == 0x01 { // Typical pass code for Blargg's tests
                blargg_test_passed = true;
                println!("Blargg Test: PASSED (Code 0x01 at {:#06X})", test_check_addr);
            } else {
                blargg_test_failed_code = Some(status);
                println!("Blargg Test: FAILED (Code {:#04X} at {:#06X})", status, test_check_addr);
            }
            // TODO: Read optional string output from 0x6004 or 0xA004
            break;
        }

        // Check for halt condition (could be end of non-Blargg test or other issue)
        if cpu.is_halted {
            println!("CPU Halted at step {}.", i + 1);
            // Check if it's the typical Blargg infinite loop: JR -2 (0x18, 0xFE)
            // This usually means the test is done and waiting.
            let current_opcode = bus.borrow().read_byte(cpu.pc.wrapping_sub(2)); // Opcode of JR
            let operand = bus.borrow().read_byte(cpu.pc.wrapping_sub(1));     // Operand of JR
            if current_opcode == 0x18 && operand == 0xFE {
                 println!("CPU Halted in typical Blargg test infinite loop (JR -2).");
                 // If status at 0x6000 is still 0x80 (running), it means test might rely on serial output
                 // or visual check. For now, just break.
            }
            break;
        }

        if (i + 1) % SERIAL_PRINT_INTERVAL == 0 {
            let serial_data = bus.borrow().get_serial_output_string();
            if !serial_data.is_empty() {
                println!("Serial Output (step {}):\n{}", i + 1, serial_data);
            }
            println!("Current CPU state (step {}): PC=0x{:04X}, SP=0x{:04X}, A=0x{:02X}, F=0x{:02X}, B=0x{:02X}, C=0x{:02X}, D=0x{:02X}, E=0x{:02X}, H=0x{:02X}, L=0x{:02X}",
                     i + 1, cpu.pc, cpu.sp, cpu.a, cpu.f, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l);
        }

        if i == MAX_STEPS - 1 {
            println!("Emulation finished after {} steps (max steps reached).", MAX_STEPS);
        }
    }

    // After loop actions
    println!("\n--- Emulation Loop Ended ---");

    if blargg_test_passed {
        println!("Overall Test Result: PASSED");
    } else if let Some(code) = blargg_test_failed_code {
        println!("Overall Test Result: FAILED (Code: {:#04X})", code);
    } else {
        println!("Overall Test Result: UNKNOWN (Test did not write a conclusive status or was interrupted).");
    }

    // Save screenshot
    let rom_filename_stem = Path::new(&rom_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown_rom");
    let screenshot_path = format!("test_output_{}.png", rom_filename_stem);

    println!("Saving screenshot to {}...", screenshot_path);
    match bus.borrow().ppu.save_framebuffer_to_png(&bus.borrow().ppu.framebuffer, 160, 144, &screenshot_path) {
        Ok(_) => println!("Screenshot saved successfully."),
        Err(e) => eprintln!("Failed to save screenshot: {}", e),
    }

    // Final serial output check
    let serial_data = bus.borrow().get_serial_output_string();
    if !serial_data.is_empty() {
        println!("\nFinal Serial Output:\n{}", serial_data);
    }

    println!("\nFinal CPU state: PC=0x{:04X}, SP=0x{:04X}, A=0x{:02X}, F=0x{:02X}, B=0x{:02X}, C=0x{:02X}, D=0x{:02X}, E=0x{:02X}, H=0x{:02X}, L=0x{:02X}",
             cpu.pc, cpu.sp, cpu.a, cpu.f, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l);
    println!("CPU is_halted: {}", cpu.is_halted);
    println!("GBC Emulator finished.");
}
