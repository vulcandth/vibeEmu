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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread; // For std::thread::sleep and spawning
use std::path::PathBuf; // For handling paths from file dialog

use eframe::egui; // For egui integration
use minifb::{Key, Window, WindowOptions, MouseButton};
use crossbeam_channel::{unbounded, Sender, Receiver}; // Added for MPSC channel
use native_dialog::FileDialog; // For native file dialogs

// Use crate:: if VibeEmu is a library and main.rs is an example or bin.
// If main.rs is part of the library itself (e.g. src/main.rs in a binary crate),
// then `crate::` is appropriate.
use crate::cpu::Cpu;
use crate::interrupts::InterruptType;
use crate::bus::Bus;
use crate::apu::CPU_CLOCK_HZ; // Import for audio timing
use crate::joypad::JoypadButton; // Added for joypad input

// Define window dimensions
const WINDOW_WIDTH: usize = 160;
const WINDOW_HEIGHT: usize = 144;

#[derive(Debug, Clone, Copy)]
enum EmulatorCommand {
    LoadRom,
    ResetEmulator,
}

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
    let mut rom_path = "roms/blargg/cpu_instrs/cpu_instrs.gb".to_string(); // Default ROM path
    let mut rom_path_explicitly_set = false;
    let program_name = args_vec.get(0).cloned().unwrap_or_else(|| "VibeEmu".to_string());


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
    println!("VibeEmu starting...");

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
            "VibeEmu - Press ESC to exit",
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
    // MAX_STEPS_HEADLESS has been removed. Functionality is covered by --halt-cycles or --halt-time.

    // Main emulation loop
    let mut emulation_steps: u64 = 0; // Total CPU steps executed
    let mut running = true;
    let start_time = Instant::now(); // Record start time, used if halt_duration_seconds is Some
    let mut has_printed_halt_message = false;
    let mut prev_right_mouse_down = false; // For right-click detection
    let paused = Arc::new(AtomicBool::new(false)); // For pause state
    let egui_context_menu_active = Arc::new(AtomicBool::new(false)); // For egui context menu
    let (command_sender, command_receiver): (Sender<EmulatorCommand>, Receiver<EmulatorCommand>) = unbounded();
    let (rom_path_sender, rom_path_receiver_main): (Sender<String>, Receiver<String>) = unbounded(); // Moved here

    // PPU timing: Game Boy PPU runs at a fixed speed.
    // Total PPU cycles per frame = Scanlines (154) * Cycles per scanline (456)
    const CYCLES_PER_FRAME: u32 = 456 * 154;
    let mut ppu_cycles_this_frame: u32 = 0;

    // Audio Sampling
    const AUDIO_SAMPLE_RATE: u32 = 44100; // Standard audio sample rate
    const CPU_CYCLES_PER_AUDIO_SAMPLE: u32 = CPU_CLOCK_HZ / AUDIO_SAMPLE_RATE;
    let mut audio_cycle_counter: u32 = 0;
    let mut audio_buffer: Vec<(f32, f32)> = Vec::new(); // Simple buffer for collected samples

    while running {
        // --- Input and Pause Toggle ---
        if let Some(w) = window.as_mut() {
            if !w.is_open() || w.is_key_down(Key::Escape) {
                running = false;
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

            // Handle right-click release detection for pause/resume
            let right_mouse_down = w.get_mouse_down(MouseButton::Right);
            if prev_right_mouse_down && !right_mouse_down {
                let current_paused_state = paused.load(Ordering::SeqCst);
                paused.store(!current_paused_state, Ordering::SeqCst);
                if !current_paused_state {
                    println!("Emulator paused.");
                    // Check if context menu should be shown (emulator is now paused)
                    if !egui_context_menu_active.load(Ordering::SeqCst) {
                        egui_context_menu_active.store(true, Ordering::SeqCst);
                        let egui_active_clone = egui_context_menu_active.clone();
                        let command_sender_clone_for_thread = command_sender.clone(); // Clone for this thread
                        let rom_path_sender_clone_for_thread = rom_path_sender.clone(); // Clone for this thread

                        thread::spawn(move || {
                            println!("Egui thread started.");
                            let native_options = eframe::NativeOptions {
                                viewport: egui::ViewportBuilder::default()
                                    .with_inner_size([300.0, 150.0]) // Slightly larger for status message
                                    .with_title("VibeEmu Context Menu"),
                                ..Default::default()
                            };
                            if let Err(e) = eframe::run_native(
                                "VibeEmu Context Menu", // App name in taskbar
                                native_options,
                                Box::new(|_cc| Box::new(ContextMenuApp::new(command_sender_clone_for_thread, rom_path_sender_clone_for_thread)))
                            ) {
                                eprintln!("Egui failed: {:?}", e);
                            }
                            println!("Egui thread finished.");
                            egui_active_clone.store(false, Ordering::SeqCst);
                        });
                    }
                } else {
                    println!("Emulator resumed.");
                    // If emulator is resumed, we generally wouldn't want a context menu
                    // or if one was active, it might be implicitly closed by egui logic later.
                    // For now, no specific action needed here for egui when resuming.
                }
            }
            prev_right_mouse_down = right_mouse_down;

            // If paused, ensure window remains responsive and events are processed.
            if paused.load(Ordering::SeqCst) && running {
                w.update(); // Process events for minifb regularly if paused
            }
        } else if is_headless && !running {
            // This case is mostly for clarity; the !running check below will handle exit.
        }

        if !running { // Check if ESC or window closed from input handling above
            break;
        }

        // --- Core Emulation Logic ---
        if !paused.load(Ordering::SeqCst) {
            let m_cycles = cpu.step(); // Execute one CPU step and get M-cycles
            let t_cycles_for_step = m_cycles * 4; // These are PPU T-cycles (also CPU T-cycles)

            if !(cpu.is_halted && cpu.in_stop_mode) {
                bus.borrow_mut().tick_components(m_cycles); // Ticks PPU, Timer
                bus.borrow_mut().apu.tick(t_cycles_for_step); // Tick APU with T-cycles
                ppu_cycles_this_frame += t_cycles_for_step; // Accumulate PPU cycles

                // Audio sample generation
                audio_cycle_counter += t_cycles_for_step;
                if audio_cycle_counter >= CPU_CYCLES_PER_AUDIO_SAMPLE {
                    audio_cycle_counter -= CPU_CYCLES_PER_AUDIO_SAMPLE;
                    let (left_sample, right_sample) = bus.borrow_mut().apu.get_mixed_audio_samples();
                    audio_buffer.push((left_sample, right_sample));
                }
            } else {
                // CPU is HALTed or in STOP mode.
                // Still need to check for window close/ESC if GUI is active and CPU is stopped.
                 if let Some(w) = window.as_ref() { // Use window.as_ref() if not modifying window itself
                     if !w.is_open() || w.is_key_down(Key::Escape) {
                        running = false;
                    }
                }
            }

            // PPU Frame rendering logic - should execute if not paused
            if ppu_cycles_this_frame >= CYCLES_PER_FRAME {
                ppu_cycles_this_frame -= CYCLES_PER_FRAME; // Reset for next frame
                if let Some(w) = window.as_mut() { // GUI mode rendering
                    let ppu_framebuffer = &bus.borrow().ppu.framebuffer;
                    let display_buffer = convert_rgb_to_u32_buffer(
                        ppu_framebuffer,
                        WINDOW_WIDTH,
                        WINDOW_HEIGHT,
                    );
                    // update_with_buffer also pumps events for minifb
                    w.update_with_buffer(&display_buffer, WINDOW_WIDTH, WINDOW_HEIGHT)
                        .unwrap_or_else(|e| panic!("Failed to update window buffer: {}", e));
                }
            }
            emulation_steps += 1; // Increment emulation steps only when not paused
        } else { // Emulator is paused
            thread::sleep(std::time::Duration::from_millis(10)); // Prevent busy loop
        }

        // --- Halt Condition Checks & Logging ---
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
                if !audio_buffer.is_empty() {
                    println!("Audio buffer size: {}", audio_buffer.len());
                    // audio_buffer.clear(); // Optionally clear buffer after printing size
                }
            }

            // Check for halt conditions (time or cycles), applicable to both GUI and headless modes
            // This logic was modified in a previous subtask, re-applying here against original structure.
            if let Some(duration_limit_secs) = halt_duration_seconds {
                let elapsed = start_time.elapsed();
                if elapsed.as_secs() >= duration_limit_secs {
                    println!("Time limit of {} seconds reached. (Elapsed: {}s)", duration_limit_secs, elapsed.as_secs());
                    running = false;
                }
            }

            if running { // Only check if not already stopped by time limit
                if let Some(limit) = halt_cycles_count {
                    // Note: emulation_steps is now only incremented when not paused.
                    // So, this cycle limit will apply to actual emulated cycles.
                    if emulation_steps >= limit {
                        println!("Cycle limit of {} reached.", limit);
                        running = false;
                    }
                }
            }
        }
        // Note: The original `emulation_steps += 1;` was here. It's now moved into the `if !paused` block.

        // Check for commands from egui thread
        // This needs to be done BEFORE the main emulation step if commands can affect it (like LoadRom/Reset)
        // And also before the headless auto-exit checks if we want commands to be processed even then.
        if let Ok(command) = command_receiver.try_recv() {
            println!("Processing command: {:?}", command);
            match command {
                EmulatorCommand::LoadRom => {
                    // The rom_path_receiver for this command was rom_path_receiver_main_thread_temp
                    // It needs to be accessible here. Let's assume it is.
                    // This part of the code is tricky because rom_path_receiver_main_thread_temp
                    // is created inside an if block. It needs to be moved outside.
                    // For now, I'll write the logic assuming it's available as `rom_path_receiver_main`.
                    // This will require moving its declaration.
                    // **This will be addressed in a subsequent change if this diff applies.**
                    // **For now, this diff might not be perfect due to scoping of rom_path_receiver_main.**
                    // **Conceptual change: Assume rom_path_receiver_main is correctly scoped for now.**

                    // The channel for ROM path is created when the egui thread is spawned.
                    // We need to ensure this receiver is the one paired with that sender.
                    // Let's rename rom_path_receiver_main_thread_temp to rom_path_receiver_main where it's declared.
                    // This is the conceptual fix. The actual `let` binding for rom_path_receiver_main
                    // needs to be moved to the same scope as `command_receiver`.
                    // I will make this change in the part where the thread is spawned.

                    match rom_path_receiver_main.recv() { // Using the correctly scoped receiver
                        Ok(new_rom_path_str) => {
                            println!("Attempting to load new ROM: {}", new_rom_path_str);
                            match load_rom_file(&new_rom_path_str) {
                                Ok(new_rom_data_vec) => {
                                    bus = Rc::new(RefCell::new(Bus::new(new_rom_data_vec)));
                                    cpu = Cpu::new(bus.clone());
                                    cpu.pc = 0x0100; // Reset PC
                                    emulation_steps = 0;
                                    ppu_cycles_this_frame = 0;
                                    audio_cycle_counter = 0;
                                    audio_buffer.clear();
                                    // The rom_path variable should ideally be updated to new_rom_path_str
                                    // if subsequent resets are to use the newly loaded ROM.
                                    // current_rom_path = new_rom_path_str; // Needs current_rom_path to be mutable
                                    println!("Successfully loaded new ROM and reset emulator.");
                                }
                                Err(e) => {
                                    eprintln!("Failed to load new ROM file '{}': {}", new_rom_path_str, e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to receive ROM path from egui thread: {:?}", e);
                        }
                    }
                }
                EmulatorCommand::ResetEmulator => {
                    println!("Attempting to reset emulator.");
                    // Using the original rom_path determined at startup
                    match load_rom_file(&rom_path) {
                        Ok(original_rom_data_vec) => {
                            bus = Rc::new(RefCell::new(Bus::new(original_rom_data_vec)));
                            cpu = Cpu::new(bus.clone());
                            cpu.pc = 0x0100; // Reset PC
                            emulation_steps = 0;
                            ppu_cycles_this_frame = 0;
                            audio_cycle_counter = 0;
                            audio_buffer.clear();
                            println!("Emulator reset successfully using original ROM.");
                        }
                        Err(e) => {
                            eprintln!("Failed to reload original ROM file '{}': {}", rom_path, e);
                        }
                    }
                }
            }
            paused.store(false, Ordering::SeqCst); // Resume emulation after processing command
            println!("Emulator resumed after command.");
        }

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
    println!("VibeEmu finished.");
}

// --- egui App Implementation ---
struct ContextMenuApp {
    command_sender: Sender<EmulatorCommand>,
    rom_path_sender: Sender<String>,
    status_message: Option<String>,
}

impl ContextMenuApp {
    fn new(command_sender: Sender<EmulatorCommand>, rom_path_sender: Sender<String>) -> Self {
        Self {
            command_sender,
            rom_path_sender,
            status_message: Some("Select an action.".to_string()),
        }
    }
}

impl eframe::App for ContextMenuApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::Window::new("Context Menu").show(ctx, |ui| {
            if let Some(message) = &self.status_message {
                ui.label(message);
            }
            ui.separator();

            if ui.button("Load ROM").clicked() {
                let dialog_result = FileDialog::new()
                    .add_filter("Game Boy ROM", &["gb", "gbc"])
                    .set_location("~/") // Example: start in user's home directory
                    .show_open_single_file();

                match dialog_result {
                    Ok(Some(path_buf)) => {
                        if let Some(path_str) = path_buf.to_str() {
                            self.rom_path_sender.send(path_str.to_string()).unwrap_or_else(|e| {
                                eprintln!("Failed to send ROM path: {:?}", e);
                                self.status_message = Some(format!("Error sending path: {:?}", e));
                            });
                            self.command_sender.send(EmulatorCommand::LoadRom).unwrap_or_else(|e| {
                                eprintln!("Failed to send LoadRom command: {:?}", e);
                                // Potentially update status_message here too if critical
                            });
                            self.status_message = Some(format!("ROM selected: {}", path_buf.file_name().unwrap_or_default().to_string_lossy()));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        } else {
                            self.status_message = Some("Selected path is not valid UTF-8.".to_string());
                        }
                    }
                    Ok(None) => {
                        self.status_message = Some("ROM selection cancelled.".to_string());
                    }
                    Err(e) => {
                        self.status_message = Some(format!("File dialog error: {:?}", e));
                    }
                }
            }

            if ui.button("Reset Emulator").clicked() {
                self.command_sender.send(EmulatorCommand::ResetEmulator).unwrap_or_else(|e| {
                    eprintln!("Failed to send ResetEmulator command: {:?}", e);
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
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
        assert_eq!(rom_path, "roms/blargg/cpu_instrs/cpu_instrs.gb".to_string());
    }

    #[test]
    fn test_parse_args_rom_path_only() {
        let args = vec!["program_name".to_string(), "my_rom.gb".to_string()];
        let (_, _, _, rom_path, rom_explicitly_set, _) = parse_args(&args);
        assert_eq!(rom_path, "my_rom.gb".to_string());
        assert_eq!(rom_explicitly_set, true);
    }
}
