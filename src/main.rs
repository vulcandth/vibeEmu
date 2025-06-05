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
use std::time::Instant; // Added for time tracking
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
// Only used in non-headless mode
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

    // --- Argument Parsing ---
    let args: Vec<String> = env::args().collect();
    let mut is_headless = false;
    let mut halt_duration_seconds: Option<u64> = None;
    let mut rom_path = "roms/cpu_instrs.gb".to_string(); // Default ROM path
    let mut rom_path_explicitly_set = false;

    // Manual argument parsing
    let mut i = 1; // Start after program name
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--headless" => {
                is_headless = true;
            }
            "--halt-time" => {
                if i + 1 < args.len() {
                    match args[i + 1].parse::<u64>() {
                        Ok(seconds) => halt_duration_seconds = Some(seconds),
                        Err(_) => {
                            eprintln!("Error: Invalid value for --halt-time. Expected a number of seconds.");
                            // std::process::exit(1); // Or simply proceed without time limit
                        }
                    }
                    i += 1; // Consume the value argument
                } else {
                    eprintln!("Error: --halt-time requires a value (seconds).");
                    // std::process::exit(1); // Or simply proceed without time limit
                }
            }
            _ => {
                // If it's not a known flag, and ROM path hasn't been set yet, assume it's the ROM path.
                if !arg.starts_with("--") && !rom_path_explicitly_set {
                    rom_path = arg.clone();
                    rom_path_explicitly_set = true;
                } else if !arg.starts_with("--") && rom_path_explicitly_set {
                    // If ROM path is already set and we encounter another non-flag argument.
                    println!("Warning: Multiple ROM paths? Using first one: {}", rom_path);
                }
                // Silently ignore unknown flags like --steps for now or print a warning.
            }
        }
        i += 1;
    }

    if is_headless {
        println!("Running in headless mode.");
        if let Some(duration) = halt_duration_seconds {
            println!("Halt time set to {} seconds.", duration);
        }
        if !rom_path_explicitly_set {
            // Default ROM for headless if not specified is fine.
            println!("No ROM path provided for headless mode. Defaulting to: {}", rom_path);
        }
    } else { // GUI Mode
        if !rom_path_explicitly_set {
            // If no ROM path is set explicitly for GUI mode, explain usage.
            // The default rom_path is already "roms/cpu_instrs.gb", so this informs the user.
            println!("Usage: {} <path_to_rom> [--headless] [--halt-time <seconds>]", args[0]);
            println!("Defaulting to ROM: {}", rom_path);
        }
    }
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

    // --- Conditional Window Initialization ---
    let mut window: Option<minifb::Window> = if !is_headless {
        Some(Window::new(
            "GBC Emulator - Press ESC to exit",
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            WindowOptions::default(),
        ).unwrap_or_else(|e| {
            panic!("Failed to create window: {}", e); // Panic remains for GUI mode if window fails
        }))
    } else {
        None // No window in headless mode
    };

    if let Some(w) = &mut window { // Only if window was created
        // Limit window update rate (optional, minifb handles this reasonably well)
        // w.limit_update_rate(Some(std::time::Duration::from_micros(16600))); // Approx 60Hz
    }

    // General emulation settings
    const SERIAL_PRINT_INTERVAL: u64 = 500_000; // Print serial output every N steps
    // TODO: Make MAX_STEPS_HEADLESS configurable via command-line argument --steps <number>
    const MAX_STEPS_HEADLESS: u64 = 20_000_000; // Limit for headless mode

    // Main emulation loop
    let mut emulation_steps: u64 = 0; // Total CPU steps executed
    let mut running = true;
    let start_time = Instant::now(); // Record start time, used if halt_duration_seconds is Some
    let mut has_printed_halt_message = false;

    // PPU timing: Game Boy PPU runs at a fixed speed.
    // Total PPU cycles per frame = Scanlines (154) * Cycles per scanline (456)
    const CYCLES_PER_FRAME: u32 = 456 * 154;
    let mut ppu_cycles_this_frame: u32 = 0;

    while running {
        let m_cycles = cpu.step(); // Execute one CPU step and get M-cycles

        // PPU runs 4 times faster than CPU M-cycles (T-cycles = M-cycles * 4)
        // Each CPU M-cycle corresponds to 4 PPU T-cycles.
        let t_cycles_for_step = m_cycles * 4; // These are PPU T-cycles

        for _ in 0..t_cycles_for_step {
            if let Some(irq_type) = bus.borrow_mut().ppu.tick() {
                bus.borrow_mut().request_interrupt(irq_type);
            }
        }
        ppu_cycles_this_frame += t_cycles_for_step as u32;

        // Frame rendering logic (GUI mode) or PPU cycle accounting (headless)
        if ppu_cycles_this_frame >= CYCLES_PER_FRAME {
            ppu_cycles_this_frame -= CYCLES_PER_FRAME; // Reset for next frame

            if let Some(w) = window.as_mut() { // GUI mode rendering
                let ppu_framebuffer = &bus.borrow().ppu.framebuffer;
                let display_buffer = convert_rgb_to_u32_buffer(
                    ppu_framebuffer,
                    WINDOW_WIDTH,
                    WINDOW_HEIGHT,
                );
                w.update_with_buffer(&display_buffer, WINDOW_WIDTH, WINDOW_HEIGHT)
                    .unwrap_or_else(|e| panic!("Failed to update window buffer: {}", e));
            }
            // In headless mode, we still account for frame timing for PPU events, but don't render.
        }

        // Check for exit conditions
        if let Some(w) = window.as_ref() { // GUI mode exit conditions
            if !w.is_open() || w.is_key_down(Key::Escape) {
                running = false;
            }
        }

        if cpu.is_halted {
            if !has_printed_halt_message {
                println!("CPU Halted at step {}. PC=0x{:04X}", emulation_steps, cpu.pc); // Added PC for context
                has_printed_halt_message = true;
            }
        } else {
            if has_printed_halt_message { // Reset if CPU is no longer halted
                println!("CPU resumed from HALT at step {}.", emulation_steps);
                has_printed_halt_message = false;
            }
        }

        // Periodic logging and headless checks (common to both modes, but some actions are headless-specific)
        if emulation_steps % SERIAL_PRINT_INTERVAL == 0 || !running { // Also print on last step
            let serial_data = bus.borrow().get_serial_output_string();
            if !serial_data.is_empty() {
                println!("Serial Output (step {}):\n{}", emulation_steps, serial_data);
            }
            if emulation_steps % (SERIAL_PRINT_INTERVAL * 10) == 0 || !running { // Less frequent full state print unless exiting
                 println!("Current CPU state (step {}): PC=0x{:04X}, SP=0x{:04X}, A=0x{:02X}, F=0x{:02X}, B=0x{:02X}, C=0x{:02X}, D=0x{:02X}, E=0x{:02X}, H=0x{:02X}, L=0x{:02X}",
                         emulation_steps, cpu.pc, cpu.sp, cpu.a, cpu.f, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l);
            }

            if is_headless {
                if emulation_steps >= MAX_STEPS_HEADLESS {
                    println!("Headless mode: Max steps ({}) reached.", MAX_STEPS_HEADLESS);
                    running = false;
                }
                if let Some(duration_limit_secs) = halt_duration_seconds {
                    let elapsed = start_time.elapsed();
                    if elapsed.as_secs() >= duration_limit_secs {
                        println!("Headless mode: Time limit of {} seconds reached. (Elapsed: {}s)", duration_limit_secs, elapsed.as_secs());
                        running = false;
                    }
                }
            }
        }

        emulation_steps += 1;

        if !running {
            break; // Exit the while loop
        }
    }

    // After loop actions
    println!("\n--- Emulation Loop Ended ---");
    println!("Total emulation steps: {}", emulation_steps);
    if is_headless {
        let elapsed_total = start_time.elapsed();
        println!("Total execution time (headless): {:.3}s", elapsed_total.as_secs_f64());
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
