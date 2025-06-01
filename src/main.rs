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

use minifb::{Key, Window, WindowOptions}; // Added for minifb

// Use crate:: if gbc_emulator is a library and main.rs is an example or bin.
// If main.rs is part of the library itself (e.g. src/main.rs in a binary crate),
// then `crate::` is appropriate.
use crate::cpu::Cpu;
use crate::bus::Bus;

// Define window dimensions
const WINDOW_WIDTH: usize = 160;
const WINDOW_HEIGHT: usize = 144;

// Function to load ROM data from a file
fn load_rom_file(path: &str) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    Ok(data)
}

// Function to convert PPU's RGB888 framebuffer to minifb's U32 (ARGB) buffer
fn convert_rgb_to_u32_buffer(rgb_buffer: &[u8], width: usize, height: usize) -> Vec<u32> {
    let mut u32_buffer = vec![0u32; width * height];
    for y in 0..height {
        for x in 0..width {
            let idx_rgb = (y * width + x) * 3;
            // Ensure we don't read out of bounds if rgb_buffer is unexpectedly short
            if idx_rgb + 2 < rgb_buffer.len() {
                let r = rgb_buffer[idx_rgb] as u32;
                let g = rgb_buffer[idx_rgb + 1] as u32;
                let b = rgb_buffer[idx_rgb + 2] as u32;
                // minifb expects ARGB format where Alpha is the highest byte,
                // but we don't have alpha from PPU, so set it to 0xFF (opaque) or 0x00.
                // Or, more simply, just pack RGB: 0xRRGGBB.
                // Let's assume 0xRRGGBB is fine for minifb if not specified otherwise.
                u32_buffer[y * width + x] = (r << 16) | (g << 8) | b;
            }
        }
    }
    u32_buffer
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

    // Create minifb window
    let mut window = Window::new(
        "GBC Emulator - Press ESC to exit",
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        WindowOptions::default(),
    )
    .unwrap_or_else(|e| {
        panic!("{}", e);
    });

    // Limit window update rate (optional, minifb handles this reasonably well)
    // window.limit_update_rate(Some(std::time::Duration::from_micros(16600))); // Approx 60Hz

    // General emulation settings
    const SERIAL_PRINT_INTERVAL: u64 = 500_000; // Print serial output every N steps

    // Main emulation loop using minifb window
    let mut emulation_steps: u64 = 0; // Total CPU steps executed

    // PPU timing: Game Boy PPU runs at a fixed speed.
    // Total PPU cycles per frame = Scanlines (154) * Cycles per scanline (456)
    const CYCLES_PER_FRAME: u32 = 456 * 154;
    let mut ppu_cycles_this_frame: u32 = 0;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let m_cycles = cpu.step(); // Execute one CPU step and get M-cycles

        // PPU runs 4 times faster than CPU M-cycles (T-cycles = M-cycles * 4)
        // Each CPU M-cycle corresponds to 4 PPU T-cycles.
        let t_cycles_for_step = m_cycles * 4; // These are PPU T-cycles

        for _ in 0..t_cycles_for_step {
            bus.borrow_mut().ppu.tick();
            // TODO: PPU could request VBlank/STAT interrupts here
            // Example: if bus.borrow().ppu.vblank_interrupt_requested() {
            //              bus.borrow_mut().request_interrupt(Interrupt::VBlank);
            //          }
        }
        ppu_cycles_this_frame += t_cycles_for_step as u32;

        // Frame rendering logic
        if ppu_cycles_this_frame >= CYCLES_PER_FRAME {
            ppu_cycles_this_frame -= CYCLES_PER_FRAME; // Reset for next frame, handling overflow

            let ppu_framebuffer = &bus.borrow().ppu.framebuffer; // This is Vec<u8> RGB
            let display_buffer = convert_rgb_to_u32_buffer(
                ppu_framebuffer,
                WINDOW_WIDTH,
                WINDOW_HEIGHT,
            );
            window.update_with_buffer(&display_buffer, WINDOW_WIDTH, WINDOW_HEIGHT)
                .unwrap_or_else(|e| panic!("Failed to update window buffer: {}", e));
        }

        // Check for halt condition
        if cpu.is_halted {
            println!("CPU Halted at step {}.", emulation_steps);
            // Break the loop if CPU is halted. User can see final state and serial output.
            break;
        }

        // Periodic logging
        if emulation_steps % SERIAL_PRINT_INTERVAL == 0 {
            let serial_data = bus.borrow().get_serial_output_string();
            if !serial_data.is_empty() {
                println!("Serial Output (step {}):\n{}", emulation_steps, serial_data);
            }
            println!("Current CPU state (step {}): PC=0x{:04X}, SP=0x{:04X}, A=0x{:02X}, F=0x{:02X}, B=0x{:02X}, C=0x{:02X}, D=0x{:02X}, E=0x{:02X}, H=0x{:02X}, L=0x{:02X}",
                     emulation_steps, cpu.pc, cpu.sp, cpu.a, cpu.f, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l);
        }

        emulation_steps += 1;
    }

    // After loop actions
    println!("\n--- Emulation Loop Ended ---");

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
