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
mod mbc; // Added MBC module
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
use crate::interrupts::InterruptType;
use crate::bus::Bus;
use crate::joypad::JoypadButton; // Added for joypad input

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

// Returns: (is_headless, halt_duration_seconds, halt_cycles_count, rom_path, rom_path_explicitly_set, program_name)
fn parse_args(args_vec: &[String]) -> (bool, Option<u64>, Option<u64>, String, bool, String) {
    let mut is_headless = false;
    let mut halt_duration_seconds: Option<u64> = None;
    let mut halt_cycles_count: Option<u64> = None;
    let mut rom_path = "roms/cpu_instrs.gb".to_string(); // Default ROM path
    let mut rom_path_explicitly_set = false;
    let program_name = args_vec.get(0).cloned().unwrap_or_else(|| "gbc_emulator".to_string());


    let mut i = 1; // Start after program name
    while i < args_vec.len() {
        let arg = &args_vec[i];
        match arg.as_str() {
            "--headless" => {
                is_headless = true;
            }
            "--halt-time" => {
                if i + 1 < args_vec.len() {
                    match args_vec[i + 1].parse::<u64>() {
                        Ok(seconds) => halt_duration_seconds = Some(seconds),
                        Err(_) => {
                            eprintln!("Error: Invalid value for --halt-time. Expected a number of seconds.");
                        }
                    }
                    i += 1; // Consume the value argument
                } else {
                    eprintln!("Error: --halt-time requires a value (seconds).");
                }
            }
            "--halt-cycles" => {
                if i + 1 < args_vec.len() {
                    match args_vec[i + 1].parse::<u64>() {
                        Ok(cycles) => halt_cycles_count = Some(cycles),
                        Err(_) => {
                            eprintln!("Error: Invalid value for --halt-cycles. Expected a number of cycles.");
                        }
                    }
                    i += 1; // Consume the value argument
                } else {
                    eprintln!("Error: --halt-cycles requires a value (number of cycles).");
                }
            }
            _ => {
                if !arg.starts_with("--") && !rom_path_explicitly_set {
                    rom_path = arg.clone();
                    rom_path_explicitly_set = true;
                } else if !arg.starts_with("--") && rom_path_explicitly_set {
                    println!("Warning: Multiple ROM paths? Using first one: {}", rom_path);
                }
            }
        }
        i += 1;
    }

    // Apply default headless halt time if --headless was specified and no --halt-time was given.
    if is_headless && halt_duration_seconds.is_none() {
        halt_duration_seconds = Some(30);
    }

    (is_headless, halt_duration_seconds, halt_cycles_count, rom_path, rom_path_explicitly_set, program_name)
}

fn main() {
    println!("GBC Emulator starting...");

    let args_vec: Vec<String> = env::args().collect();
    let (
        mut is_headless, // Make mutable to allow fallback
        mut halt_duration_seconds, // Make mutable for fallback
        halt_cycles_count,
        rom_path,
        rom_path_explicitly_set,
        program_name
    ) = parse_args(&args_vec);

    // Initial message about ROM and potential GUI mode usage.
    if !is_headless && !rom_path_explicitly_set {
        println!("Usage: {} <path_to_rom> [--headless] [--halt-time <seconds>] [--halt-cycles <cycles>]", program_name);
        println!("Defaulting to ROM: {}", rom_path);
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
    println!("Determined System Mode: {:?}", bus.borrow().get_system_mode()); // Added logging

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

    // --- Conditional Window Initialization with Fallback ---
    let mut window_attempt: Option<minifb::Window> = None;
    let mut fell_back_to_headless = false; // Track if fallback occurred

    if !is_headless {
        match Window::new(
            "GBC Emulator - Press ESC to exit",
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            WindowOptions::default(),
        ) {
            Ok(w) => window_attempt = Some(w),
            Err(e) => {
                eprintln!("Failed to create window: {}. Falling back to headless mode.", e);
                is_headless = true; // Switch to headless
                fell_back_to_headless = true; // Mark that fallback happened
                // Apply default headless halt time if no other time is set
                if halt_duration_seconds.is_none() {
                    halt_duration_seconds = Some(30);
                }
                // window_attempt remains None
            }
        }
    }
    // Assign to the final window variable
    let mut window: Option<minifb::Window> = window_attempt;

    // Print headless mode status AFTER window initialization attempt
    if is_headless {
        if fell_back_to_headless {
            // Specific message for fallback
            println!("Now running in headless mode due to window creation failure.");
        } else {
            // Standard message if started in headless mode via argument
            println!("Running in headless mode.");
        }
        if let Some(duration) = halt_duration_seconds {
            println!("Halt time set to {} seconds.", duration);
        }
        if let Some(cycles) = halt_cycles_count {
            println!("Halt cycles set to {}.", cycles);
        }
        if !rom_path_explicitly_set {
             // This message might be redundant if already printed, but context dependent.
            println!("No ROM path provided for headless mode. Defaulting to: {}", rom_path);
        }
    }


    if let Some(_w) = &mut window { // Only if window was created
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

        if !(cpu.is_halted && cpu.in_stop_mode) {
            for _ in 0..t_cycles_for_step {
                // First, call ppu.tick() and release the borrow on bus
                let ppu_interrupt_request_type: Option<crate::interrupts::InterruptType> = bus.borrow_mut().ppu.tick();
                // Then, if an interrupt was requested, borrow bus again to set the flag
                if let Some(irq_type) = ppu_interrupt_request_type {
                    bus.borrow_mut().request_interrupt(irq_type);
                }
                // TODO: Add bus.borrow_mut().timer.tick() and bus.borrow_mut().apu.tick() here as well
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
        } else {
            // In STOP mode, peripherals (PPU, APU, Timer) do not tick.
            // Minimal PPU cycle update might be needed if STOP has specific PPU interactions,
            // but Pandocs implies system clock is off. Joypad input is still handled below.
            // If headless, ppu_cycles_this_frame won't advance, which is correct.
            // To prevent busy-looping in headless STOP mode without a timer/interrupt to wake it,
            // a short sleep could be added here, but active polling for joypad/interrupts is typical.
            // For now, just ensure ppu_cycles_this_frame doesn't accumulate if we're not ticking PPU.
            // If an interrupt occurs, cpu.step() will handle un-halting.
            // We still need to check if window is closed or ESC is pressed in STOP mode for GUI.
            if let Some(w) = window.as_ref() {
                 if !w.is_open() || w.is_key_down(Key::Escape) {
                    running = false;
                }
            }
        }

        // Check for exit conditions and handle input if window exists
        // This block handles input regardless of STOP mode, allowing Joypad to wake the CPU.
        if let Some(w) = window.as_mut() { // GUI mode exit conditions and input handling
            if !w.is_open() || w.is_key_down(Key::Escape) { // This check is duplicated if !is_halted && !in_stop_mode
                running = false;                           // but harmless. Can be refactored if needed.
            }

            // Handle Joypad Input
            let mut bus_mut = bus.borrow_mut();
            let key_mappings = [
                (Key::Right, JoypadButton::Right),
                (Key::Left, JoypadButton::Left),
                (Key::Up, JoypadButton::Up),
                (Key::Down, JoypadButton::Down),
                (Key::Z, JoypadButton::A), // 'Z' for A
                (Key::X, JoypadButton::B), // 'X' for B
                (Key::Enter, JoypadButton::Start),
                (Key::RightShift, JoypadButton::Select),
                // (Key::LeftShift, JoypadButton::Select), // Alternative for Select
            ];

            for (key, button) in key_mappings.iter() {
                let pressed = w.is_key_down(*key);
                if bus_mut.joypad.button_event(*button, pressed) {
                    bus_mut.request_interrupt(InterruptType::Joypad);
                }
            }
            // Also handle LeftShift for select if desired, ensuring it doesn't double-trigger if both are pressed
            // For simplicity, this example uses only RightShift. If LeftShift is also needed,
            // ensure only one event is processed for "Select" if both shifts are pressed.
            // One way:
            let left_shift_pressed = w.is_key_down(Key::LeftShift);
            if bus_mut.joypad.button_event(JoypadButton::Select, w.is_key_down(Key::RightShift) || left_shift_pressed) {
                 bus_mut.request_interrupt(InterruptType::Joypad);
            }


        } else if is_headless { // Headless mode specific checks (no input handling from minifb)
            // (Headless specific checks like MAX_STEPS_HEADLESS or halt_duration_seconds are handled later)
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
                // Check for halt_cycles_count condition
                if running { // Only check if not already stopped by time limit or max steps
                    if let Some(limit) = halt_cycles_count {
                        if emulation_steps >= limit {
                            println!("Headless mode: Cycle limit of {} reached.", limit);
                            running = false;
                        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_halt_cycles_valid() {
        let args = vec![
            "program_name".to_string(),
            "--headless".to_string(),
            "--halt-cycles".to_string(),
            "100".to_string(),
        ];
        let (_, _, halt_cycles, _, _, _) = parse_args(&args);
        assert_eq!(halt_cycles, Some(100));
    }

    #[test]
    fn test_parse_args_halt_cycles_invalid() {
        let args = vec![
            "program_name".to_string(),
            "--headless".to_string(),
            "--halt-cycles".to_string(),
            "abc".to_string(),
        ];
        // Assuming error printed and value remains None (or default)
        let (_, _, halt_cycles, _, _, _) = parse_args(&args);
        assert_eq!(halt_cycles, None);
    }

    #[test]
    fn test_parse_args_halt_cycles_missing_value() {
        let args = vec![
            "program_name".to_string(),
            "--headless".to_string(),
            "--halt-cycles".to_string(),
        ];
        let (_, _, halt_cycles, _, _, _) = parse_args(&args);
        assert_eq!(halt_cycles, None);
    }

    #[test]
    fn test_parse_args_headless_default_halt_time() {
        let args = vec!["program_name".to_string(), "--headless".to_string()];
        let (_, halt_time, _, _, _, _) = parse_args(&args);
        assert_eq!(halt_time, Some(30));
    }

    #[test]
    fn test_parse_args_headless_custom_halt_time() {
        let args = vec![
            "program_name".to_string(),
            "--headless".to_string(),
            "--halt-time".to_string(),
            "10".to_string(),
        ];
        let (_, halt_time, _, _, _, _) = parse_args(&args);
        assert_eq!(halt_time, Some(10));
    }

    #[test]
    fn test_parse_args_headless_halt_time_zero() {
        let args = vec![
            "program_name".to_string(),
            "--headless".to_string(),
            "--halt-time".to_string(),
            "0".to_string(),
        ];
        let (_, halt_time, _, _, _, _) = parse_args(&args);
        assert_eq!(halt_time, Some(0));
    }

    #[test]
    fn test_parse_args_no_args() {
        let args = vec!["program_name".to_string()];
        let (is_headless, halt_time, halt_cycles, rom_path, _, _) = parse_args(&args);
        assert_eq!(is_headless, false);
        assert_eq!(halt_time, None);
        assert_eq!(halt_cycles, None);
        assert_eq!(rom_path, "roms/cpu_instrs.gb".to_string());
    }

    #[test]
    fn test_parse_args_rom_path_only() {
        let args = vec!["program_name".to_string(), "my_rom.gb".to_string()];
        let (_, _, _, rom_path, rom_explicitly_set, _) = parse_args(&args);
        assert_eq!(rom_path, "my_rom.gb".to_string());
        assert_eq!(rom_explicitly_set, true);
    }
}
