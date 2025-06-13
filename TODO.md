# Rust Game Boy Color Emulator – Design Guide & Checklist

## Overview and Goals

This design guide outlines a **Game Boy / Game Boy Color emulator** written in Rust, focusing on **cycle accuracy**, multi-model support (DMG, CGB, and other hardware revisions), and cross-platform compatibility. The architecture draws inspiration from the SameBoy emulator’s highly accurate design[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,downsampled%20from%202MHz%2C%20and%20accurate), prioritizing correctness and timing fidelity. Key goals include:

- **Support for DMG & CGB models** (original Game Boy and Game Boy Color), with provisions for other variants (e.g. Game Boy Pocket, Super Game Boy)[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,compatibility%20with%20other%20emulators). The emulator will detect the game’s hardware mode and emulate model-specific behavior (e.g. DMG mode vs. CGB enhancements).

- **Cycle-accurate emulation** of the CPU, PPU, APU, and timers, ensuring timing edge cases and quirky hardware behavior are faithfully reproduced (as evidenced by SameBoy’s ability to pass tough test ROMs[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,the%20Demotronic%20trick%2C%20Prehistorik%20Man)). Accuracy is prioritized over speed, but overall performance will be tuned to achieve real-time gameplay.

- **Cross-platform GUI**: The emulator core will be portable, with frontends for Windows and Linux (and potential WebAssembly support). Video output will be displayed in a window with proper scaling, and input from keyboard/gamepad will be supported. The PPU rendering is decoupled from OS-specific drawing to ease future web integration.

- **Audio output with `cpal`**: Sound emulation (APU) will produce audio fed to the host system via the `cpal` crate, which provides cross-platform audio I/O.

- **Extensibility for debugging and linking**: While initial focus is on core emulation, the design will accommodate **debugging tools** (instruction stepper, memory/VRAM viewer, etc.) and plan for **link cable emulation** in the future. (Save states are explicitly **out of scope** to simplify design.)

- **No save states**: The emulator will support battery-backed save RAM for games (to save in-game progress), but will not implement emulator save states, focusing instead on run-time accuracy and stability.

With these goals in mind, we present the architecture, module breakdown, implementation plan, and checklists to build the emulator.

## Architecture Overview

The emulator is structured into a **core library** (emulation engine) and a **platform-specific frontend**. This separation ensures that the core logic can run independently of any OS or UI, which is important for adding platforms (like a web frontend) later. The overall architecture and module communication are summarized below:

- **Emulation Core**: Contains the simulation of all Game Boy hardware components:
  
  - **CPU**: The LR35902 8-bit processor (similar to Z80/8080) executes instructions, manages registers/flags, and orchestrates device I/O. It runs in step with a master clock.
  
  - **PPU (Pixel Processing Unit)**: Simulates the Game Boy’s graphics unit which generates 160×144 pixel frames. It operates asynchronously relative to the CPU but synchronized to the same clock, reading from video memory and issuing interrupts (VBlank, STAT) to the CPU.
  
  - **APU (Audio Processing Unit)**: Simulates the 4-channel sound hardware (2 square wave channels, 1 waveform channel, 1 noise channel). It produces audio samples corresponding to the Game Boy’s audio frames, which will be sent to the audio output.
  
  - **MMU (Memory Management Unit)**: Central bus/router that handles reads/writes to memory addresses. It maps the CPU’s 16-bit address space to RAM, VRAM, I/O registers, cartridge ROM/RAM, etc., and directs memory accesses to the correct module. This is the glue that connects CPU with PPU, APU, cartridge, and other hardware.
  
  - **Cartridge**: Emulates the game cartridge, including the ROM data and Memory Bank Controller (MBC) if present. It handles bank switching, external RAM, and contains logic for special hardware (e.g. Real Time Clock in MBC3, rumble in MBC5, etc.). The MMU will call into the cartridge module for any ROM/RAM accesses in the cartridge address range.
  
  - **Timer Unit**: Simulates the divider (`DIV`) and timer (`TIMA/TMA/TAC`) registers. It is ticked by the CPU clock and can raise timer interrupts at the correct cycles. It handles edge cases like the timer increment timing and DIV reset behaviors.
  
  - **Interrupt Controller**: Manages interrupt requests from various sources (VBlank, LCD STAT, Timer, Serial, Joypad). In practice, this is just a couple of memory-mapped registers (`IF` and `IE`), but the emulator will model the logic of setting flags and the CPU checking them at instruction boundaries (or mid-instruction for certain scenarios).
  
  - **Input (Joypad)**: Reads host input (keyboard or gamepad) and updates the joypad register (`P1`) accordingly. The joypad state is polled by games via memory reads and can trigger an interrupt on state change.
  
  - **Serial/Link Port**: Provides serial data transfer emulation. Initially, this can be stubbed or looped-back (for example, some test ROMs send output through the serial port which we can print to console). The design will later allow connecting two instances or a network socket for link cable emulation (e.g. trading Pokémon), as SameBoy already supports local link and IR emulation[sameboy.github.io](https://sameboy.github.io/features/#:~:text=%2A%20Three%20audio%20high,Game%20Boy%20Printer%20emulation3).

- **Frontend (Platform Layer)**: The frontend handles everything related to the outside world:
  
  - **Video Output**: A window or canvas to display frames. The PPU will produce a frame buffer (pixel array) for each video frame; the frontend takes this pixel data and blits it to the screen using a graphics library. By keeping PPU logic separate from rendering, we could swap the backend (e.g. use a WebCanvas for web, or an SDL/OpenGL window for desktop) without changing PPU code.
  
  - **Audio Output**: Uses `cpal` to send audio samples to the system’s sound device. The emulator’s APU generates raw audio sample buffers (at a suitable sample rate, e.g. 44.1 kHz or 48 kHz). The frontend’s audio callback will pull data from the APU or a shared audio buffer and output sound. (SameBoy, for instance, generates high-quality 96 kHz audio[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,in%20the%20CGB%2FAGB%20boot%20ROM), so we aim for high fidelity audio output.)
  
  - **Input Handling**: Captures keyboard or controller events from the OS (e.g. using the windowing library or SDL) and updates the emulator’s joypad state. The input is typically read each frame or polling cycle and written into the `P1` register bits.
  
  - **User Interface & Debug**: On desktop, this might include menus, an on-screen LED indicating status, or eventually UI panels for debugging (like tile viewers, registers, etc.). On the web, it could be web page elements. Initially, the UI is minimal – just the game screen and maybe an FPS display – but the design allows extension.

**Module Communication:** All core components communicate primarily via the **MMU/bus and shared clock**:

- The **CPU** drives the clock: it executes instructions and advances a cycle counter. After each instruction (or each machine cycle), it will inform other components to tick forward. For example, if an instruction took 4 cycles, the CPU might call `ppu.step(4)` and `timer.step(4)` so that the PPU and timers advance by 4 cycles worth of work.

- The **PPU** monitors the cycle count to know when to switch LCD modes (OAM scan, drawing, HBlank, VBlank) and when to draw pixels. It writes to a frame buffer in memory (e.g. an array of 160×144 color values) as it renders each scanline. It also uses the MMU to read tile data and OAM (sprite attributes) during rendering. When a frame is complete (end of VBlank), it can notify the frontend to display the new frame.

- The **APU** similarly steps forward with CPU cycles, updating its channel generators (square waves, noise LFSR, etc.) and producing audio samples. Because the APU’s internal clock dividers operate at 2 MHz on the DMG[sameboy.github.io](https://sameboy.github.io/features/#:~:text=blargg%E2%80%99s%20test%20ROMs%20%2A%20Sample,the%20Demotronic%20trick%2C%20Prehistorik%20Man) (half the CPU clock), we need to accumulate audio samples at the correct rate. The APU might accumulate samples in a ring buffer. The audio output thread will consume those samples via `cpal`.

- **Interrupts**: When the PPU hits VBlank or certain scanline conditions, it sets a flag in `IF`. Likewise, timer overflow sets `IF`. The CPU, at instruction boundaries, will check `IF & IE` and if the interrupt master enable (IME) is true, it will service the highest priority interrupt. The design might model this by checking interrupts after each instruction execution (and also handling the edge case of the HALT state, which has special behavior if an interrupt triggers while IME=0).

- **Memory access**: The CPU reads/writes via the MMU. The MMU will delegate the request to the correct module:
  
  - Cartridge: for ROM and external RAM addresses (e.g. 0000-7FFF, A000-BFFF).
  
  - WRAM/HRAM: for work RAM and high RAM addresses (C000-DFFF, E000-FDFF, FF80-FFFE).
  
  - PPU: for VRAM and OAM addresses (8000-9FFF, FE00-FE9F), and LCD registers (FF40-FF4B).
  
  - APU: for sound registers (FF10-FF3F).
  
  - Timer/Interrupt: for timer registers (FF04-FF07) and IF/IE (FF0F, FFFF).
  
  - Joypad: for 0xFF00, reading input state.
  
  - Serial: for 0xFF01/FF02, reading/writing link data.
  
  - Unmapped areas (like unused I/O or the echo RAM at E000-EFFF duplication) will be handled appropriately (often by mirroring or ignoring writes).

- **Decoupling PPU Rendering**: The PPU module will ideally not call any OS drawing routines directly; instead, it writes to a pixel buffer. Once a frame is ready, the core can pass this buffer to the frontend (or the frontend can read it via an API). This means the core doesn’t depend on a library like SDL or OpenGL – making it feasible to compile the core to WebAssembly or use it in a different environment. For example, SameBoy’s core is platform-independent and used in both its Cocoa (macOS) frontend and SDL frontend[sameboy.github.io](https://sameboy.github.io/features/#:~:text=Core%20Emulation%20Features).

In summary, the architecture ensures a clear separation between the **emulation core modules** (which handle Game Boy hardware logic) and the **system interface** (window, audio, input). Communication is orchestrated mainly by the CPU’s execution loop and memory bus interactions, with carefully timed updates to achieve cycle accuracy.

## Recommended Libraries and Tools

To implement this emulator in Rust with high performance and cross-platform support, we recommend the following crates and libraries:

- **CPU & Core Performance**: The core emulation logic can be written in pure Rust (no external crate needed for CPU). However, for performance and ergonomics:
  
  - Use `bitflags` crate to manage CPU flag register bits (ZF, NF, HF, CF) efficiently.
  
  - Use `enum_primitive` or `num_enum` for mapping opcodes to handler functions, if using a jump table for the CPU decoder.
  
  - Optimize memory access by using Rust slices or arrays for RAM, and perhaps the `bytemuck` crate for safe casting between byte arrays and structured data if needed.
  
  - Consider enabling `#![inline(always)]` on small opcode functions or using macros for the CPU opcodes to reduce overhead.
  
  - Profile with tools like `cargo profiler` or `perf` to find hotspots. The Game Boy CPU runs ~4.19 MHz (DMG) or ~8.38 MHz (CGB double speed[copetti.org](https://www.copetti.org/writings/consoles/game-boy/#:~:text=cutting,8.38%20MHz)), so the emulator must execute millions of instructions per second. Well-written Rust can handle this, as demonstrated by other emulators (e.g. MooneyeGB achieving 2000-4000% speed on an i7 in release mode[github.com](https://github.com/Gekkio/mooneye-gb#:~:text=Performance)).

- **Graphics (Window & Rendering)**: For cross-platform window creation and 2D graphics, two primary approaches are recommended:
  
  1. **SDL2 crate** (`rust-sdl2`): SDL2 provides a straightforward way to create a window, handle input events, and draw graphics. It’s battle-tested and works on Windows/Linux. We could use SDL2 to create a texture and update it each frame with the 160×144 pixel buffer (scaling it up for display). SDL2 also has audio, but we prefer `cpal` for audio in this design. (SameBoy uses an SDL frontend on non-macOS platforms[sameboy.github.io](https://sameboy.github.io/features/#:~:text=Core%20Emulation%20Features)).
  
  2. **Winit + Pixels**: The `winit` crate can create a window and handle events in a cross-platform way, and the `pixels` crate can render a pixel buffer to that window using GPU acceleration (via `wgpu`). This combination is pure Rust and supports WebAssembly (via WebGL) with minimal changes. Using `pixels` allows efficient scaling and filtering (nearest-neighbor for crisp pixels or optional smoothing). This approach cleanly separates the emulator frame buffer (just a `[u32; 160*144]` array of RGBA pixels, for example) from the rendering backend.
  
  3. **Minifb**: As an alternative, `minifb` is a lightweight framebuffer library that opens a window and blits a pixel buffer. It’s simple to use and has few dependencies. Projects like "Gameboy" by Rustfinity use minifb for easy framebuffer management[rustfinity.com](https://www.rustfinity.com/open-source/gameboy#:~:text=Gameboy%20relies%20on%20several%20Rust,libraries%20with%20native%20dependencies). Minifb doesn’t support Web, but it’s a good choice for quick prototyping on desktop.

  *Recommended:* In the short term, use `minifb` or `pixels` for simplicity. For long-term and web support, **Winit + Pixels** is ideal, as it aligns with our decoupled PPU design and can run on both desktop and WebAssembly.

- **Audio**: Use the **`cpal`** crate for cross-platform audio output[rustfinity.com](https://www.rustfinity.com/open-source/gameboy#:~:text=Gameboy%20relies%20on%20several%20Rust,libraries%20with%20native%20dependencies). `cpal` allows you to open an audio stream and either provide a callback to supply audio samples in real-time or manually write to an output buffer. We will configure `cpal` for stereo audio at a standard sample rate (e.g. 44,100 Hz). The Game Boy has 4 channels mixed into 2 output channels (left/right) with 4-bit DACs. We’ll generate audio frames in the APU and buffer them.
  
  - Consider using a lock-free ring buffer (e.g. from the `ringbuf` crate or a simple `VecDeque` with a mutex) for audio samples. The APU can push samples as they are generated, and the `cpal` audio callback pops them. This decouples the emulator timing from the exact audio callback timing.
  
  - Alternatively, use `cpal`’s callback to drive audio: each time the callback fires (requesting N frames), step the emulator APU forward enough cycles to produce N frames of audio. This is more complex to coordinate but ensures audio timing stays locked with emulation.
  
  - Ensure to handle underflow/overflow: if the emulator is running slower than real-time, the audio buffer might underflow (causing crackling); if running too fast, buffer might overflow (increasing latency). A simple solution is to drop samples or duplicate last sample if mismatched, but our goal is to run in real-time, so a well-timed main loop is crucial.

- **Threading & Concurrency**: The emulator can be largely single-threaded (the CPU, PPU, APU, etc. must be tightly coordinated). However, we may use threads for:
  
  - **Audio**: `cpal` often uses a separate thread for audio callbacks. Our code inside that callback should be minimal (e.g. just pulling from a buffer prepared by the emulation thread).
  
  - **Frontend vs Core**: We could run the emulation core in one thread and the UI (window event loop) in another to avoid stalling input/events when emulation is busy. If using a typical game loop on the main thread, this may not be necessary as long as we poll events regularly.
  
  - If we do use multi-threading, the **`crossbeam`** crate is useful for its channels (for sending events like “new frame ready” or “user pressed Reset”) and for its thread spawning utilities. Rust’s standard `mpsc` channel or an `Arc<Mutex<T>>` can also be used to share state between threads (e.g. the pixel buffer can be an `Arc<Mutex<Vec<u32>>>` if the render thread needs to read it, though double buffering might be better).
  
  - The **`parking_lot`** crate offers more efficient synchronization primitives (Mutex, Condvar) than the standard library and can be considered if lock contention becomes an issue.
  
  - **Rayon** is not particularly needed here since most of the work is sequential. Parallelism might be applied to rendering (e.g. drawing scanlines in parallel), but given the small resolution of the Game Boy, that’s overkill. The audio mixing of 4 channels could be done in parallel but again is trivial in cost. So, we primarily use threads for separating concerns (UI vs emulation vs audio).

- **Utilities and Testing**:
  
  - **Logging**: Use the `log` crate with a backend like `env_logger` for debug output. This is helpful for development (e.g. logging unimplemented opcodes or important events).
  
  - **Unit Testing**: Use Rust’s built-in test framework for unit tests of components (e.g. a test for the ADD instruction behavior). No external crate needed.
  
  - **Integration Testing**: To automate running ROM test suites, we can use the `assert_cmd` crate or simply build a test runner that loads ROMs and checks expected output. (More details in the Testing Strategy section below.)
  
  - **File I/O**: Use `std::fs` to load ROM binaries and to save cartridge RAM to disk. No special crate needed, though something like `dirs` crate can help find the user’s save directory (for storing battery saves).
  
  - **Configuration**: If we have config files (key bindings, etc.), `toml` or `serde` could be used to parse them. Initially, hardcoding defaults or command-line flags (using `clap` crate) is sufficient. For example, RBoy uses command-line options for things like forcing DMG mode or enabling serial log[github.com](https://github.com/mvdnes/rboy#:~:text=Options%3A%20,Print%20help).

**Summary of Crates:**

| Concern            | Crate(s) & Tools                                                                                                                                               | Notes                                           |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| **Audio**          | `cpal`[rustfinity.com](https://www.rustfinity.com/open-source/gameboy#:~:text=Gameboy%20relies%20on%20several%20Rust,libraries%20with%20native%20dependencies) | Low-level cross-platform audio output.          |
| **Graphics**       | `winit` + `pixels` or `minifb`                                                                                                                                 | Cross-platform window & pixel rendering.        |
| **Input & Events** | `winit` or `SDL2` (built-in)                                                                                                                                   | Provided by windowing library (keyboard, etc).  |
| **Threading**      | std threads + `crossbeam` (channels)                                                                                                                           | For concurrency; `parking_lot` for locks.       |
| **Performance**    | Profile (e.g. `cargo flamegraph`)                                                                                                                              | Use efficient Rust, bitflags, avoid allocations |
| **Testing**        | (built-in) + `serial_test` (opt)                                                                                                                               | Serial test outputs; compare expected strings.  |
| **Debug UI**       | `egui` or dev-only SDL window                                                                                                                                  | (Future) For building debugger UI overlays.     |

Using these libraries, we ensure the emulator runs smoothly on Windows and Linux (and likely macOS, though not explicitly required), and lays the groundwork for a WebAssembly build in the future (where we’d swap out the audio backend to use `cpal`’s WASM support[reddit.com](https://www.reddit.com/r/rust/comments/12qj2ty/ironboy_high_accuracy_gameboy_emulator_written_in/#:~:text=Reddit%20www,youtube) and use the same core logic with a web canvas).

## Module Responsibilities & Interactions

This section details each major module in the emulator core, its role, and how it interacts with other components. Understanding these responsibilities will clarify the implementation tasks and their dependencies.

### CPU (LR35902 core)

**Responsibilities:** Emulate the Game Boy’s 8-bit CPU, including all opcodes, registers, and instruction timings. The CPU fetches instructions from memory (via MMU), executes them (updating registers and memory), and manages the program counter, stack pointer, and flags. It also controls enabling/disabling interrupts (IME flag) and entering low-power states (HALT, STOP).

**Accuracy considerations:** All 256 primary opcodes and 256 extended (0xCB-prefixed) opcodes must be implemented with correct behavior and cycle timings[github.com](https://github.com/mvdnes/rboy#:~:text=,Timer). The CPU runs at ~4.19 MHz (4194304 Hz) in normal mode; on CGB it can double to ~8.38 MHz if the game enables double speed mode[copetti.org](https://www.copetti.org/writings/consoles/game-boy/#:~:text=cutting,8.38%20MHz). The design should include the CGB speed switch (via the KEY1 register), meaning the CPU module might have a mode flag and adjust timings accordingly[github.com](https://github.com/mvdnes/rboy#:~:text=,Timer).

**Interactions:**

- **Memory**: The CPU uses the MMU to read instructions and data. For each memory access (opcode fetch, operand fetch, data load/store), it calls the MMU. The MMU, in turn, delegates to the correct component (RAM, VRAM, I/O, etc.). Some instructions like `LD (HL),A` will ultimately affect PPU memory if HL points to VRAM or OAM, so MMU must enforce any access restrictions (e.g. VRAM not accessible during mode 3 rendering).

- **Clock & Sync**: The CPU is the master driver of cycles. After executing an instruction (or even each machine cycle if doing cycle-by-cycle), it increments a global cycle counter. The CPU then informs other units to advance. For example, if an instruction took 12 cycles, the CPU might call `ppu.step(12)` and `timer.step(12)` and so on. Alternatively, the CPU could run in a loop where each iteration is one clock tick: update CPU internals partially, update PPU one tick, etc. Both approaches achieve the same end; a common compromise is stepping in 4-cycle increments (the smallest unit of work on the Game Boy bus) for simplicity while maintaining accuracy.

- **Interrupt Handling:** The CPU checks the interrupt flags. If an interrupt is enabled and requested, and IME=1, it will perform an interrupt sequence (push PC to stack, jump to the interrupt vector, etc.). If IME=0 and a HALT was executed, the CPU must handle the HALT bug scenario[gbdev.io](https://gbdev.io/pandocs/halt.html#:~:text=When%20a%20,fails%20to%20be%20normally%20incremented). The CPU must implement the RETI instruction to flip IME back on, handle DI/EI with their delayed-IME behavior (EI enables interrupts after the *next* instruction). All these details ensure correct interrupt timing.

- **Special States:**
  
  - *HALT:* As mentioned, if HALT is executed with interrupts disabled and an interrupt pending, the CPU doesn’t halt but also doesn’t advance the PC (the HALT bug)[gbdev.io](https://gbdev.io/pandocs/halt.html#:~:text=When%20a%20,fails%20to%20be%20normally%20incremented). We will implement this quirk because some games (and test ROMs) rely on it.
  
  - *STOP:* The STOP instruction (0x10) is used primarily on CGB to switch double speed mode. When STOP is executed and a speed-switch is requested via the KEY1 register, the CPU will toggle speed mode. We must simulate this by updating our cycle timing multiplier and clearing the request bit.
  
  - *Undocumented behavior:* The Game Boy CPU doesn’t have unofficial opcodes like the Z80 does (most undefined opcodes on LR35902 just perform different 8-bit loads). But we should implement the documented behavior for those (many are just aliases of others). Emulating any quirks like incorrect flag results on certain operations (if any discovered) should be noted via research (Pan Docs and test ROMs cover these).

**Data structures:** Likely a `Cpu` struct holds registers (A, B, C, D, E, H, L, F (flags), SP, PC) and some state flags (IME, halted, etc.). Execution can be a match on opcode to a function or a computed jump table for speed. The opcode implementation can be organized by grouping similar ops (e.g. arithmetic, loads, bit ops) or generated via macros. Each opcode handler returns the number of cycles taken (so the CPU knows how far to advance other components).

### PPU (Picture Processing Unit)

**Responsibilities:** Emulate the Game Boy’s LCD controller which produces video output. This includes:

- Maintaining video memory (VRAM) which holds tile data and tile maps. On CGB, VRAM is 16 KB with two banks; on DMG, 8 KB single bank.

- Maintaining OAM (Object Attribute Memory) for sprites (40 entries). The PPU reads OAM to draw sprites.

- Scanning through modes 0-3 for each scanline: Mode 2 (OAM scan), Mode 3 (drawing pixels), Mode 0 (HBlank), and Mode 1 (VBlank). The timing of these modes is critical for things like the STAT interrupt and certain LCD trick effects.

- Constructing each scanline’s pixel data by reading background tile maps, window tile maps, tile patterns, and sprite data, obeying priorities and attribute flags (flips, palette, behind-background flags, etc.).

- Managing palettes: On DMG, there are 4 shades of gray and two palette registers (BG and OBJ palettes) that assign those shades to color indices. On CGB, there are 8 palettes for background and 8 for sprites, each with 4 colors (from a 15-bit RGB palette), and these are stored in separate palette RAM. The PPU must apply the correct palette to each pixel.

- Triggering **LCD STAT interrupts** for various conditions: LY==LYC compare, mode transition interrupts (OAM (mode2) interrupt, VBlank (mode1) interrupt, HBlank (mode0) if enabled).

- Triggering **VBlank interrupt** at the end of the frame (LY=144).

- Handling **OAM DMA** transfers: When the CPU writes to DMA register (0xFF46), the PPU/CPU perform a DMA from CPU RAM to OAM. During this 160-byte transfer, the CPU is paused for about 160 * 4 cycles (plus setup)[gbdev.io](https://gbdev.io/pandocs/OAM_Corruption_Bug.html#:~:text=,or%20write%20is%20not%20asserted). We must simulate this pause (perhaps by “stealing” cycles from CPU or by a DMA handler that runs in parallel but prevents CPU memory access to 0xFE00-0xFE9F).

- Handling **CGB DMA (HDMA)**: On Game Boy Color, there is a HDMA mechanism to transfer data from ROM to VRAM either in one go (General DMA) or during HBlank intervals. This is more advanced: if a game uses HDMA, the PPU and CPU will be synchronizing on HBlank. Our emulator should plan to implement this for compatibility with CGB-exclusive games. This might involve a HDMA controller that triggers on each HBlank, halting the CPU for a few cycles to transfer 0x10 or 0x11 bytes, and repeating until done (or doing all at once in General DMA mode, which halts CPU fully during the transfer).

- Emulating **LCD connectivity**: On real hardware, if the LCD is off (bit 7 of LCDC register = 0), the PPU stops operation and the screen is blank. When turned on, it takes some time (several frames) to warm up. For simplicity, we can instantly turn it on/off, but we must reset PPU state properly on LCD off. Also, when off, VRAM and OAM are accessible with no restrictions (games toggle LCD off to manipulate VRAM quickly).

**Interactions:**

- **Memory**: The PPU’s VRAM and OAM are part of the memory map. The CPU can write to VRAM and OAM through the MMU except when the PPU is in modes that block access (for example, VRAM access is typically not allowed during mode 3, OAM not allowed during mode 2/3). The emulator should enforce these: e.g., if the CPU attempts to read VRAM mid-frame where it shouldn’t, one approach is to return 0xFF or the last value (some emulators do this to emulate the glitchy reads).

- **CPU Sync**: The PPU module will have a function like `step(cycles)` that advances the internal PPU state by a given number of clock cycles. The PPU will accumulate cycles and, once enough cycles pass to move to a new mode or scanline, it updates the LY register and checks for interrupts. For example, one scanline (modes 2->3->0) on DMG lasts 456 cycles. After 144 scanlines, it goes into VBlank for 10 lines (LY=144 to 153). The cycle counting must be exact. A **T-cycle accurate design** would advance one clock at a time and update PPU state machine accordingly[sameboy.github.io](https://sameboy.github.io/features/#:~:text=%2A%20Sample,optional%20frame%20blending%20modes%2017). A simpler, yet still accurate, design is to simulate in chunks (e.g. 4-cycle increments).

- **Interrupts**: The PPU raises two kinds of interrupts: VBlank (when LY=144) and LCD STAT (which can be from OAM, HBlank, VBlank, or LY==LYC match depending on bits in STAT register). Our PPU should set the IF flags at the correct cycle. For instance, the LY==LYC interrupt should trigger at the *transition* to the matching line, and only if enabled. If the game toggles LYC or changes STAT interrupt enables mid-frame, our emulation should handle that (likely by checking after each mode or line).

- **Frame Buffer**: The PPU writes pixel values to a frame buffer (an off-screen buffer). For DMG, these can be 0-3 values (shade indices) which we later map to actual grayscale colors. For CGB, the PPU will produce 15-bit RGB values (0-31 per channel) which we then convert to 24-bit color for display. We can maintain the frame buffer as an array of e.g. `u32` RGBA values for easy blitting. The PPU sets a flag or calls a callback when a frame is complete (end of VBlank), so the frontend knows to draw the new frame.

- **Timing quirks**: There are known edge cases like the “mid-scanline palette swap” trick and others where games turn off the LCD or modify registers at specific times to produce effects. A cycle-accurate PPU is needed to emulate those. SameBoy, for instance, achieves pixel-perfect timing allowing even obscure demos to run correctly[sameboy.github.io](https://sameboy.github.io/features/#:~:text=%2A%20Sample,optional%20frame%20blending%20modes%2017). We should document which effects might not work if we simplify timing, and aim to refine toward cycle accuracy.

**Data structures:** A `Ppu` struct will hold:

- VRAM (8KB or 16KB array, possibly with bank index for CGB).

- OAM (160 bytes array for sprite attributes).

- LCDC, STAT, SCY, SCX, LY, LYC, WY, WX registers, etc.

- Palette data (for CGB: arrays of 8 palettes × 4 colors for BG and OBJ).

- A cycle counter and mode state (current mode and how many cycles into it).

- Perhaps a line buffer for current scanline being rendered, before copying to frame buffer.

- The output frame buffer (160×144 pixels).

The PPU will implement methods to reset (clear memory on boot), tick cycles, and a method to get a reference to the frame buffer for rendering.

### APU (Audio Processing Unit)

**Responsibilities:** Emulate the Game Boy’s sound hardware, which consists of 4 channels:

1. **Channel 1:** Square wave with frequency sweep (tone + sweep).

2. **Channel 2:** Square wave (tone).

3. **Channel 3:** Wave channel (programmable 32-byte waveform playback).

4. **Channel 4:** Noise channel (LFSR pseudo-random noise).

Each channel has its own registers for frequency, volume envelope, length counter (to auto-stop sound after a duration), etc. The APU mixes these channels into left/right output with volume control (VIN and output terminal enables). We must emulate:

- Volume envelopes (volume auto-increase or decrease every N frames).

- Frequency sweep on Channel 1 (adjust frequency at given intervals).

- Length counters on all channels (tick at 256 Hz to turn off channels when their length expires, if enabled).

- The wave channel’s playback of the waveform RAM (NR3x registers and the 32-byte Wave RAM at 0xFF30-0xFF3F).

- Noise channel’s LFSR behavior (7-bit vs 15-bit modes depending on a control bit).

- The frame sequencer: Game Boy’s APU runs a sequencer at 512 Hz that steps through 8 subframes, which trigger volume envelope updates, length counter check, and sweep updates at specific intervals (e.g. sweep at 128 Hz, envelopes at 64 Hz, etc.). We should implement this frame sequencer to correctly time these events.

- Mixing and output: Each channel can be enabled on the left and/or right output (controlled by NR51 register). The master volume (NR50) controls the volume to each side. The output of each channel is 4-bit (0-15) which we combine and output as audio samples. Typically, emulators output at a higher sample rate by interpolating or stepping multiple times per sample. For simplicity, we can run the APU at the CPU clock granularity (or a subdivision thereof) to produce high-resolution samples, then downsample.

- **CPAL integration**: The APU should produce a buffer of audio samples (e.g. 16-bit PCM values). Ideally, one sample per ~? CPU cycles. The DMG’s native sample rate is around 1048576 Hz / 32 ~ 32768 Hz when deriving from frame sequencer? However, many emulators output at fixed rates like 44.1 kHz. We can oversample and then average or just tick the APU to generate audio at the needed rate.

- Optionally emulate DAC behavior like sound popping when enabling/disabling channels, and the effect of the wave channel’s DAC power (NR30 bit 7). For high accuracy, the startup state of sound registers and the exact behavior of disabling sound channels (e.g. wave channel DAC off mutes channel immediately) should be replicated. SameBoy’s APU is **sample-accurate down to 2 MHz sampling**[sameboy.github.io](https://sameboy.github.io/features/#:~:text=blargg%E2%80%99s%20test%20ROMs%20%2A%20Sample,the%20Demotronic%20trick%2C%20Prehistorik%20Man), which is very high quality. We might target something a bit less extreme initially but keep design open for improvement.

**Interactions:**

- **Memory**: Sound registers (NR10-NR14 for Ch1, NR21-24 Ch2, NR30-34 Ch3, NR41-44 Ch4, NR50-52 master) reside at FF10-FF26. The APU module will expose these registers via the MMU. When the CPU writes to them, we update the corresponding channel parameters (with some writes also resetting things, e.g. writing to NR14 (channel 1 frequency high) with the high bit set triggers that channel’s sound restart).

- **Interrupt**: The APU raises the **sound interrupt** (also known as frame sequencer interrupt) at a rate of 256 Hz if enabled (this is the so-called NR52 bit 7?). Actually, Game Boy has a “sound length expired” or “frame sequencer” interrupt in some older documentation, but in reality the Game Boy only has 5 interrupt sources (VBlank, LCD, Timer, Serial, Joypad). There is no dedicated sound interrupt that the CPU uses; sound events are polled by the game if needed. So we do not have a separate interrupt line for APU, aside from the possibility of using the unused bit 3 of IF (which is often labeled “Serial”).

- **CPU Sync**: Like the PPU, the APU needs to advance in step with cycles. We will implement `apu.step(cycles)` which accumulates cycles and updates channel state. The frame sequencer ticks every 8192 CPU cycles (512 Hz), so we can count cycles and when that threshold passes, update envelopes, sweeps, lengths accordingly. Also, more frequently, we need to generate audio samples. A common technique is to accumulate a fractional phase for each channel at the target sample rate. For example, if CPU runs at 4,194,304 Hz and audio output is 44,100 Hz, that’s ~95 CPU cycles per audio sample. We can accumulate error and generate one sample when that threshold passes.

- **Output Buffer**: The APU could directly call an audio callback or push to a ring buffer. Perhaps simplest: maintain an internal ring buffer of i16 stereo samples. Each APU step, check if enough cycles have elapsed to generate the next audio sample (using a ratio as described). If yes, mix the channels and produce a sample pair, push to buffer. The audio thread will consume from this. We need to ensure thread-safety (maybe wrap buffer in a mutex or use a lock-free structure).

- **APU Enable/Disable**: Writing 0 to the sound on/off register (NR52, bit 7) stops all sound (and clears registers). We should implement this (it’s rarely used by games except maybe to reduce power).

**Data structures:** An `Apu` struct containing sub-structs for each channel (with their state: frequency timer, envelope timer, current volume, etc.). Also store the frame sequencer step and cycle count. Keep a small buffer for audio samples and possibly some synchronization primitive for the audio thread to signal if buffer is empty/filled. The APU also has a global enable and flags for each channel on/off (NR52 bits). A possible approach is to update channel outputs every CPU tick but that’s heavy; instead we might compute at smaller time slices.

### MMU (Memory Management Unit) / Bus

**Responsibilities:** Implements the 16-bit address bus for the Game Boy, mapping addresses to the correct memory or I/O. It centralizes all reads and writes so we can enforce restrictions and trigger side effects. Major regions to handle:

- **0000-3FFF:** Cartridge ROM bank 0 (fixed bank).

- **4000-7FFF:** Cartridge ROM switchable bank.

- **8000-9FFF:** VRAM (switchable bank on CGB).

- **A000-BFFF:** Cartridge external RAM (or other cartridge-specific hardware like RTC).

- **C000-CFFF:** Work RAM bank 0.

- **D000-DFFF:** Work RAM bank 1 (CGB has up to bank 7 switchable via SVBK).

- **E000-FDFF:** *Echo RAM* (mirror of C000-DDFF). We can implement this mirror or simply map these addresses to the same memory as C000-DDFF.

- **FE00-FE9F:** OAM (sprite attribute memory).

- **FEA0-FEFF:** Unusable area (returns 0xFF if read, no effect on write). Real hardware basically cannot use these addresses – on DMG, certain OAM bug behaviors occur if accessed at specific times[gbdev.io](https://gbdev.io/pandocs/OAM_Corruption_Bug.html#:~:text=There%20is%20a%20flaw%20in,PPU%20is%20in%20mode%202), but generally it’s off-limits.

- **FF00-FF7F:** I/O registers. This includes:
  
  - FF00: Joypad.
  
  - FF01/FF02: Serial data and control.
  
  - FF04-FF07: Timer/DIV registers.
  
  - FF0F: Interrupt Flag.
  
  - FF10-FF3F: Sound registers (APU).
  
  - FF40-FF4B: LCD/PPU registers (LCDC, STAT, SCY, etc.).
  
  - FF4F: VRAM bank select (CGB only).
  
  - FF50: Boot ROM disable register (more on this below).
  
  - FF51-FF55: CGB DMA registers (HDMA1-5).
  
  - FF68-FF6B: CGB palette registers.
  
  - FF70: Work RAM bank (SVBK on CGB).
  
  - Others (e.g. FF6C, FF72-FF77 are not used or CGB internal).

- **FF80-FFFE:** High RAM (fast zero-page RAM).

- **FFFF:** Interrupt Enable register.

The MMU will have to implement reads and writes to all these areas. In many cases, it just indexes into a RAM array (WRAM, HRAM, VRAM, etc.). But for I/O registers, writing and reading often has side effects or restricted behavior:

- Writing to **DIV (FF04)** resets the divider to 0[gbdev.io](https://gbdev.io/pandocs/Timer_Obscure_Behaviour.html#:~:text=Notice%20how%20the%20bits%20themselves,causes%20a%20few%20odd%20behaviors).

- Reading from **DIV** gives the high bits of the internal counter – it changes continuously, and on CGB double speed it increments twice as fast.

- Writing to **TIMA** or **TMA** or **TAC** immediately affects the timer (and there are obscure behaviors when writing to these at certain cycles around an overflow; these are detailed in research[gbdev.io](https://gbdev.io/pandocs/Timer_Obscure_Behaviour.html#:~:text=Notice%20how%20the%20bits%20themselves,causes%20a%20few%20odd%20behaviors) but can be added later).

- Writing to **DMA (FF46)** should initiate OAM DMA. We simulate by scheduling 160 byte copies from the specified source address (source * 0x100) to OAM. During this copy, for 640 cycles, the CPU can only access HRAM (FF80-FFFE); if it accesses other areas, on real hardware it will read from open bus. We can simplify by halting CPU execution for 160×4 cycles when DMA is triggered, or emulate cycle-by-cycle blocking.

- **VRAM and OAM blocking**: When PPU is in modes that restrict access, the MMU can either return 0xFF on read or ignore writes (some emulators do that), or better, we prevent the CPU from accessing those at all by timing (i.e. the CPU will not get to execute those cycles because PPU is mid-operation). A simpler method: allow access but if a test ROM checks for proper behavior, it might fail. For accuracy, implement proper blocking or effects (the Pan Docs describe what values are seen if you read VRAM at bad times).

- **Boot ROM**: At power-up, addresses 0x0000-0x00FF (for DMG) or 0x0000-0x0100 (for CGB) are initially mapped to the boot ROM (Nintendo logo splash, etc.). When the boot ROM finishes, it writes to register FF50 to disable itself, after which the cartridge ROM is visible at 0x0000. We should support a boot ROM for authenticity and to pass certain tests (the boot ROM initializes certain registers and verifies the Nintendo logo). We can include open-source boot ROMs (SameBoy provides these[sameboy.github.io](https://sameboy.github.io/features/#:~:text=other%20emulators%20,A%20%2B%20B%20%2B%20direction)) or require the user to supply the official ones. Alternatively, for simplicity, we could skip the boot ROM and instead load the cartridge directly, while initializing the registers to known post-boot values (Pan Docs provides these initial values). However, skipping the boot may cause some games to misbehave (and fails tests that specifically check boot behavior). Design-wise, including a small boot ROM step is ideal. We map a BootROM component in the MMU that responds at 0x0000 until FF50 is written.

**Interactions:**

- The MMU is accessed by **CPU** constantly for instruction fetch and data. It will call into **Cartridge** for ROM/RAM, **PPU** for VRAM/OAM and LCD registers, **APU** for sound registers, **Timer** for timer registers, etc. This means MMU either holds references to these modules or provides callback hooks. E.g., MMU might have `ppu: &mut Ppu` and so on to delegate. Alternatively, the MMU could be a trait that each module implements parts of (less common).

- Many modules need to access memory as well:
  
  - PPU reading from VRAM (the PPU could access VRAM directly without going through MMU since it “owns” VRAM, but if we keep MMU as the single source of truth, PPU can call MMU too – however, to avoid double indirection, PPU might just index into a VRAM array it owns).
  
  - APU might need to read Wave channel RAM or check NRX values – but those it can have direct access to its regs.
  
  - The **Cartridge** needs to know when the CPU writes to certain addresses (like banking registers, or the RTC latch register on MBC3, etc.). The MMU will intercept writes in 0x0000-0x7FFF and 0xA000-BFFF and pass them to a cartridge handler (instead of writing to any RAM).

- The MMU will also be involved in **saving game data**: when the emulation is stopped, we should take the cartridge’s RAM (if any) and write it to a .sav file. Likewise, at load time, we initialize cartridge RAM from disk. We can integrate that by giving Cartridge module file I/O capability or handling it at a higher level (frontend calls a save function).

**Data structures:**

- A `Mmu` struct could contain arrays for internal RAM (8KB WRAM, 8KB VRAM etc.) or references to modules that manage their memory. Alternatively, we might not have a distinct `Mmu` struct and instead have a series of `match` or if-else in the CPU’s memory access functions to route addresses. However, a dedicated MMU struct is cleaner.

- The Cartridge interface: perhaps a trait `CartridgeMBC` implemented by specific MBC types (NoMBC, MBC1, MBC3, etc.), with methods like `read(addr) -> u8`, `write(addr, value)`. The MMU will call `cart.read(addr)` for ROM/RAM ranges. This makes it easier to support different cartridge types.

- Flags for currently selected banks (VRAM bank, WRAM bank) as set by CGB registers.

### Cartridge & MBC

**Responsibilities:** Emulate the behavior of the game cartridge:

- Provide read access to ROM banks (possibly multiple megabytes of ROM, though we can simply memory-map it in an array and use bank offset math to return the correct byte).

- Handle writes to MBC control registers (which are usually in the 0x0000-0x7FFF range). For example:
  
  - **MBC1:** Writes in 0x6000-0x7FFF select RAM banking mode vs ROM banking mode; writes in 0x4000-0x5FFF select high bit of ROM bank or RAM bank (depending on mode); writes in 0x2000-0x3FFF select lower 5 bits of ROM bank; writes in 0x0000-0x1FFF enable/disable RAM.
  
  - **MBC2:** Similar but with internal 512x4-bit RAM and only 4 ROM bank bits.
  
  - **MBC3:** Adds RTC support. 0x0000-0x1FFF RAM enable, 0x2000-3FFF ROM bank, 0x4000-5FFF selects RAM bank or RTC register, 0x6000-7FFF latches RTC. We need to emulate an RTC clock that runs in real time (or at least increments when the emulator is running). We can use system time for RTC or advance it with cycles (since typically it’s 1 Hz increments).
  
  - **MBC5:** Supports up to 8MB ROM, 128KB RAM, uses 9-bit ROM bank number (split across two writes), etc., and potentially rumble (some MBC5 carts have a rumble motor triggered by writing to a certain bit).
  
  - Other MBCs (MBC6, MBC7 with accelerometer, HuC3, etc.) are rare and can be omitted initially or stubbed.

- [x] Manage external RAM: allocate a vector for it if present (size determined by cartridge header). Ensure that when RAM is “disabled” via MBC, reads return 0xFF or open-bus (and writes don’t stick).

- [x] Provide a mechanism to **save** the RAM to disk (battery-backed RAM). This could be an API call from the frontend to dump the RAM to a file when the emulator exits, and load from file when starting. It’s easiest to do this in the cartridge module as it knows the RAM size and content. (SameBoy supports battery save for game progress[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,on%20a%20Game%20Boy%20Color), and we will too, but **not** emulator state save).

- [x] Identify cartridge type: On loading a ROM, read the header at 0x0147 to determine the MBC type and instantiate the appropriate handler[gbdev.io](https://gbdev.io/pandocs/#:~:text=32,46)[gbdev.io](https://gbdev.io/pandocs/#:~:text=36,M161). For instance, 0x01 means MBC1, 0x13 means MBC3+RAM+Battery, etc. Also check 0x014C (CGB flag) to know if game supports CGB mode (0x80 or 0xC0 means CGB capable) so we start emulator in CGB or DMG mode accordingly.

- [x] Map the **boot ROM**: Not exactly cartridge, but we should consider how to handle the boot ROM mapping. Possibly the MMU can have a flag that first 0x100 bytes come from an internal boot ROM until disabled.

**Interactions:**

- **MMU**: As described, the MMU will call cartridge’s read/write for addresses in cartridge space. The cartridge code computes which bank and offset to use. E.g., a read at 0x4000 with current bank 3 means return `rom[bank3_offset + (addr - 0x4000)]`.

- **No direct interrupts**: Cartridges don’t issue interrupts except indirectly (RTC might trigger an alarm in HuC3, but that’s extremely niche). The RTC is polled by the game via registers.

- **Timing**: Normally, ROM/RAM accesses take the same time as any memory (1 M-cycle). We might not need to simulate wait states for different cartridges (Game Boy did have some differences, but generally it’s consistent except CGB has double speed mode affecting relative timing). We can ignore any minor timing differences of MBC (most are transparent).

- If we emulate **link cable in future**: Some cartridges (like Pokémon) use the link port, but that’s via the Serial I/O, not the cartridge, so no effect here.

**Data structures:**

- Perhaps an enum `CartridgeKind` { NoMBC, MBC1(Mbc1State), MBC2(Mbc2State), MBC3(Mbc3State), MBC5(Mbc5State), Unknown }.

- Each state includes current bank numbers, RAM on/off flag, RTC registers (for MBC3), etc.

- The raw ROM data (Vec<u8>) and RAM (Vec<u8>) stored in the main cartridge struct.

- Implement methods: `read_rom(addr)`, `read_ram(addr)`, `write_rom(addr, val)`, `write_ram(addr, val)` depending on address to handle bank registers vs actual data.

- The MMU could use these via `cart.read(addr)` which internally decides ROM vs RAM region.

### Timer & Divider

**Responsibilities:** Emulate the system timer:

- The **DIV** register at FF04 is an 8-bit register that increments at 16384 Hz (on DMG). It is essentially the upper 8 bits of a 16-bit counter that runs at 256 KHz (or something similar – in fact, DIV increments every 256 cycles). Writing to DIV resets that whole counter to 0[gbdev.io](https://gbdev.io/pandocs/Timer_Obscure_Behaviour.html#:~:text=DIV%20is%20just%20the%20visible,part%20of%20the%20system%20counter).

- The **TIMA** (FF05) is the timer counter that the game can use, which increments at a frequency determined by **TAC** (FF07) bits. TAC selects one of 4 clock speeds for TIMA: 4096 Hz, 262144 Hz, 65536 Hz, 16384 Hz (these are derived from the main clock by dividing by 1024, 16, 64, 256 respectively). Only when the timer is enabled (TAC bit 2), TIMA will increment at that rate (which is a sub-multiple of DIV’s rate).

- When TIMA overflows (goes from 0xFF to 0x00), it resets to the value in **TMA** (FF06) and triggers a Timer Interrupt (setting IF bit 2).

- There are some tricky behaviors:
  
  - If TIMA is written at the exact cycle it was about to overflow, the behavior can be glitched (either the value takes effect or the overflow happens – known as the “TIMA reload bug” in older documentation).
  
  - Writing to DIV can cause an edge case: if you reset DIV at the moment a timer increment was scheduled, you might skip or double increment TIMA[gbdev.io](https://gbdev.io/pandocs/Timer_Obscure_Behaviour.html#:~:text=Notice%20how%20the%20bits%20themselves,causes%20a%20few%20odd%20behaviors).
  
  - The Timer behaves slightly differently on CGB vs DMG in terms of how the divider is connected[gbdev.io](https://gbdev.io/pandocs/Timer_Obscure_Behaviour.html#:~:text=On%20DMG%3A), though functionally games usually don’t notice aside from certain homebrew tests.

- The **Timer module** will essentially simulate the 16-bit divider counter and the TIMA counter. It will need to be ticked every CPU cycle.

**Interactions:**

- **CPU**: Will call `timer.step(cycles)` each loop. The timer adds cycles to an internal counter, updating DIV every 256 cycles (since DIV is 1/256 of that counter). When enough cycles accumulate that meet the TAC-selected threshold, TIMA increments.

- **Interrupt**: If TIMA overflows, the Timer module will request an interrupt (set IF bit 2). The CPU will service it when appropriate.

- **MMU**: The timer registers are memory-mapped, so reads/writes go through MMU to the Timer. Writing to DIV triggers an immediate action (reset counters)[gbdev.io](https://gbdev.io/pandocs/Timer_Obscure_Behaviour.html#:~:text=Notice%20how%20the%20bits%20themselves,causes%20a%20few%20odd%20behaviors). Writing to TAC (control) can change the frequency divider immediately (Pan Docs describes behavior if switching rates at certain times). Writing to TIMA or TMA just updates those values (with TIMA write also potentially affecting an ongoing overflow event depending on timing).

- The Timer doesn’t directly affect other modules, but indirectly it’s part of game logic (many games use it for periodic events, and it must be accurate or games might run too fast/slow or lock up if they rely on exact overflow timing).

**Data structure:** A `Timer` struct with fields:

- `div_counter: u16` for the internal counter (or just use one u16 and derive DIV from its high byte).

- `tima: u8`, `tma: u8`, `tac: u8`.

- Perhaps a record of the last cycle count at which we updated, to handle partial steps (or accumulate cycles in a fractional way).

The `step(cycles)` method will loop or accumulate until it accounts for all cycles:

- Increment `div_counter` by `cycles`. Then for each cycle, or in chunk, determine if `div_counter` overflowed past 0xFF00 (which would increment DIV register visible part).

- Actually easier: since DIV is upper 8 bits of a 16-bit counter, just do: previous_div = div_counter >> 8; div_counter += cycles; new_div = div_counter >> 8; if new_div != previous_div, DIV changed (though we can simply compute DIV when needed).

- For TIMA: We check the timer’s input clock. A common approach is to know how many CPU cycles per TIMA increment based on TAC:
  
  - If TAC enabled and freq=00: increment TIMA every 1024 cycles (4096 Hz).
  
  - freq=01: every 16 cycles (262144 Hz).
  
  - freq=10: every 64 cycles (65536 Hz).
  
  - freq=11: every 256 cycles (16384 Hz).  
    We can keep a separate counter for TIMA, or derive from div_counter bits (since DIV’s bits 0-1-2 correspond to some of those rates).  
    In fact, Pan Docs shows that the timer uses a particular bit of the divider as a clock, which is why writing DIV can cause a tick if it resets that bit[gbdev.io](https://gbdev.io/pandocs/Timer_Obscure_Behaviour.html#:~:text=On%20DMG%3A).

- When TIMA overflows, set a flag to request interrupt and reset TIMA to TMA.

We will implement the simplest correct logic and refine later for edge cases.

### Interrupt Controller

**Responsibilities:** Manage the interrupt enable (IE at FFFF) and interrupt request/flags (IF at FF0F). This is not a large module, but conceptually:

- The **IF** register’s bits are set by various hardware events (VBlank, LCD STAT, Timer, Serial, Joypad). They are cleared when the CPU acknowledges the interrupt (when jumping to the handler, the CPU writes 0 to the corresponding bit of IF).

- The **IE** register is set by the program to enable specific interrupts.

- The CPU’s internal IME (interrupt master enable) flag determines if interrupts are allowed.

- The interactions are largely handled in CPU (checking and dispatching), but we might have the Interrupt Controller module to encapsulate setting/clearing bits.

**Interactions:**

- **PPU**: calls a function to set IF bit 0 (VBlank) or bit 1 (LCD STAT) when needed.

- **Timer**: sets IF bit 2 when TIMA overflows.

- **Serial**: sets IF bit 3 when a serial transfer completes (if we emulate that).

- **Joypad**: sets IF bit 4 when a falling edge on a button input is detected (the Joypad register has an interrupt on change).

- **CPU**: reads IF & IE to decide what to service. Also CPU will write to IF (e.g., writing 0 to clear bits, which is done after servicing or by some games manually).

- This module (or just part of MMU) will handle read/write of IF and IE addresses.

In implementation, we might not have a separate struct for this – IF and IE can be stored in the CPU or MMU. But logically, it’s a distinct piece of hardware.

### Input (Joypad)

**Responsibilities:** Emulate the joypad input registers. The game reads from memory address 0xFF00 to get button states. This register is a bit unusual: it has two modes selected by two high bits (bit 5 and 4: Select Action buttons vs Select Directional buttons). When a mode bit is 0, that group of 4 inputs is enabled for reading (the others appear as 1s). The lower 4 bits are the actual inputs (0 = pressed). So, software writes to 0xFF00 to select which group, then reads it to get the state of those 4 buttons.

We need to map keyboard (or gamepad) inputs to these 8 Game Boy inputs (Up, Down, Left, Right, A, B, Start, Select). For example, arrow keys or WASD for directions, Z for A, X for B, Enter for Start, Backspace for Select – similar to RBoy’s defaults[github.com](https://github.com/mvdnes/rboy#:~:text=Gameplay%20Keybindings). We can allow reconfiguration later.

**Interactions:**

- The frontend will capture key presses/releases. We maintain a bitfield of the 8 buttons. When a button transitions from ‘up’ to ‘down’, we should set the Joypad Interrupt (IF bit 4) if that interrupt is enabled (IE bit 4) – this mimics the hardware behavior of generating an interrupt on input. (On real hardware, the interrupt is on a falling edge of any key that is enabled by the select bits).

- The MMU’s read of 0xFF00 will combine the currently selected high bits (which group) and the current button states. Typically:
  
  - If the game writes 0x10 to select directions, then reading 0xFF00 should give 0x10 ORed with bits for each direction (bits 0-3) where 0 means pressed. Non-selected group (buttons) returns 1s in those bits.
  
  - We can compute the return as: lower_nibble = ~current_state of selected group; then OR with 0xF0 for the bits that are 1 (depending on selection bits).

- If no buttons are pressed, the bits read as 1 (since pulled-up). If a button is pressed and its group is selected, that bit reads as 0.

- We will also ensure that at startup or reset, no buttons pressed = all 1s in FF00 lower bits.

We might implement Joypad handling in the MMU or as a small struct that holds the state and has a method `read_joypad(select_bits) -> u8`.

### Serial (Link Cable)

**Responsibilities:** Emulate the serial port (Link Cable) registers: SB (0xFF01) and SC (0xFF02). Games use these to send/receive data to another Game Boy. Bit 7 of SC when set initiates a transfer, and an interrupt (IF bit 3) is raised when the transfer completes. Typically, the two Game Boys shift 8 bits to each other over 8K cycles (8192 cycles for a full byte at 1KB/s on DMG).

For our emulator, initially, we can shortcut this:

- If running a test ROM that outputs to serial (like Blargg’s tests), they often write a byte to SB and then write 0x81 to SC (start transfer with internal clock). On real hardware, if no partner, nothing comes in, and after 8K cycles, the byte in SB is what was sent by itself or 0xFF if nothing? Actually, if using internal clock mode and no external device, I think it will just transfer 0xFF as the incoming bits. Blargg’s tests use this mechanism to send characters out (they expect the other side to be some device reading them).

- We can intercept this by, for example, every time a byte is “sent” (SC written with start), we could immediately set SB to 0xFF (as received byte) and flag the transfer complete, raising IF bit 3. Additionally, if a special mode is enabled (like a CLI flag `--serial`), we could take the sent byte and print it to stdout (to see test output)[github.com](https://github.com/mvdnes/rboy#:~:text=Options%3A%20,Print%20help).

- For actual link cable emulation later: we would need to have two instances of the emulator send bytes to each other. This could be via a socket or pipe, or if in the same process, an internal link cable connector struct that transfers SB to the other’s SB. The design should keep this in mind by maybe abstracting the link cable I/O through a trait or interface, so later we can plug in a network or local two-player connection.

**Interactions:**

- **CPU**: writes to SB/SC to initiate transfers, reads SB to get received data.

- **Interrupt**: when transfer completes, IF bit 3 (Serial) is set if enabled.

- **Timer**: Actually, the serial can optionally be tied to the Timer’s clock if using external clock mode (SC bit 0 = 0 for external clock, meaning the other device pulses clock). Usually games use internal clock (bit 0 = 1), meaning the sender provides timing.

- In the emulator, we can treat it as immediate or with a slight delay. Since link cable timing is not usually critical in games (except for the communication protocol), we could simulate the 8192 cycle delay by not instantly setting the interrupt until that many cycles pass. But an easier route is to approximate it or ignore timing for now (just instantly transfer), which should be fine for tests and most use.

**Data structures:**

- A `Serial` struct with perhaps a buffer or just storing SB and SC values. When SC write indicates start, if we have a link partner, forward the byte. If not, store the byte for printing (if in test mode).

- A flag or mode to indicate “headless serial print” vs “connected link”.

- For link cable future: maybe keep a reference to another Serial struct (for the other Game Boy) or a channel to send bytes over network.

**Boot ROM**  
Though not a module per se, worth noting:

- We will have a small 256-byte (DMG) and 2048-byte (CGB) boot ROM. We can include an open-source version[sameboy.github.io](https://sameboy.github.io/features/#:~:text=other%20emulators%20,A%20%2B%20B%20%2B%20direction). The MMU at reset will map this at 0x0000. It will remain mapped until a write to FF50 occurs (which the boot ROM itself does at end). After that, MMU will map the cartridge ROM at 0x0000 and ignore any further reads of boot ROM.

- We should allow an option to skip boot (set FF50=1 immediately and initialize registers manually) for faster dev iteration or if the user doesn’t have a boot ROM.

Now that each module is described, we can see interdependencies: **CPU** depends on MMU (for memory), which ties into **PPU**, **APU**, **Timer**, **Cartridge**, etc. **PPU/APU** depend on being ticked by CPU timing. **Cartridge** and **Timer** need to raise interrupts via **Interrupt/CPU**. **Input/Serial** connect via **MMU** and **Interrupts** too.

The design ensures each module has a single clear role, but they interlock through the MMU and the central timing loop.

## Implementation Roadmap & Checklist

Implementing a full emulator is complex – breaking it into manageable pieces with a logical sequence is crucial. The following checklist outlines the stages of development, the dependencies of each stage, and the tasks to complete. Each step should be tested (as described in the Testing section) before moving to the next.

- **Project Setup & Skeleton** – *Dependencies: None.*
  
  - [x] Initialize a Rust project (binary crate for now). Add dependencies: `cpal`, `minifb`/`winit+pixels`, etc.

  - [x] Define module structure (Rust `mod` files) for core components: `cpu.rs`, `mmu.rs`, `ppu.rs`, `apu.rs`, `cartridge.rs`, `timer.rs`, etc. Define basic structs (fields only) for each, allowing cross-references via passing mutable references or storing within a `GameBoy` struct.

  - [x] Plan a `GameBoy` or `Emulator` struct that contains all modules or acts as the top-level, orchestrating the main loop.

  - [x] Set up logging (optional) and a basic CLI argument parsing (e.g. accept a ROM file path, and flags like `--serial` for test mode, `--dmg` to force DMG mode, etc.).

- **Cartridge Loading** – *Dep: Project setup.*
  
    - [x] Write a function to load a ROM file into memory (Vec<u8>).
  
    - [x] Parse the cartridge header to determine MBC type, RAM size, and whether the game supports CGB mode (if CGB and the user’s emulator is in “CGB capable” mode, we’ll emulate in color mode, otherwise DMG mode).
  
    - [x] Instantiate the appropriate MBC state (for now, implement **No MBC** (ROM only), **MBC1**, **MBC3**, **MBC5** as these cover most games[github.com](https://github.com/mvdnes/rboy#:~:text=%2A%20MMU%20%2A%20MBC,Printing). Others can be stubbed or added later).
  
    - [x] Initialize cartridge RAM (Vec<u8> of correct size, filled with 0xFF or 0x00). If a battery save file exists for this ROM (determine filename by ROM name), load it into RAM.
  
    - [x] Implement basic `read(addr)` and `write(addr, val)` for ROM/RAM:  
    
    * [x] For No MBC: reading 0x0000-0x7FFF just returns from ROM (no banking), writing 0xA000-BFFF writes to RAM if present.
    * [x] For MBC1: support bank switching (5-bit ROM bank number, 2-bit RAM bank or upper ROM bits, mode select), and RAM enable.
    * [ ] For MBC3: support 7-bit ROM bank, 4 8KB RAM banks, RTC registers mapping (just stub the RTC read/write logic initially), RAM enable.
    * [x] For MBC5: support 9-bit ROM bank, up to 16 RAM banks, RAM enable, possibly rumble enable bit (can ignore actual rumble feedback for now).
  
    - [x] Test: print out some cartridge info (title, MBC type) to ensure parsing is correct.

- **CPU Core (basic)** – *Dep: Cartridge (for fetching opcodes from ROM).*
  
  - [x] Implement CPU registers and the main fetch-decode-execute loop. Start with a limited set of opcodes to get something running (e.g. NOP, JP, basic loads).

  - [x] Set up reset/initial state: If using boot ROM, load it at 0x0000 and set PC=0x0000. If skipping boot, initialize registers to known defaults (AF=$01B0 on DMG, etc., as per Pan Docs).

  - [x] For now, ignore interrupts and most peripheral effects; focus on the CPU able to read opcodes and advance PC correctly.

  - [x] Include a cycle counter in CPU and increment by each instruction’s cycles.

  - [x] MMU integration: The CPU’s fetch and memory access should call MMU. Implement a provisional MMU that can read from cartridge and an array for RAM. (PPU, APU, etc., can be stubbed: e.g., reads from 0xFF00-FF7F return 0xFF, writes do nothing for now).

  - [x] Implement enough opcodes to run a simple test. For example, write a test in Rust with a small bytecode that loads values, adds, jumps, etc., and verify CPU produces expected register values. *(Unit test)*.
  
  - [x] Gradually implement all opcodes. Use a reference (like Pan Docs or Game Boy manual) to ensure each sets flags correctly. For CB-prefixed opcodes (bit operations, shifts, etc.), implement those too.
  
  - [x] Validate instruction timing: maintain a table of cycles per opcode and ensure CPU adds the correct amount. Mark opcodes that have conditional cycle lengths (e.g. JR taken vs not taken).

  - [x] Implement interrupts in CPU: Check IF & IE after each instruction if IME is enabled. If an interrupt is pending and IME=1, service it (push PC, jump to vector, reset IME). If IME=0 and HALT was executed, handle the halt bug conditions[gbdev.io](https://gbdev.io/pandocs/halt.html#:~:text=When%20a%20,fails%20to%20be%20normally%20incremented).
  
  - Test: Use known CPU test ROMs (blargg’s **cpu_instrs.gb**). This will require more of the system to be in place (at least a rudimentary PPU or serial output capture for results). Alternatively, implement a partial instruction logger and compare with expected sequence. Blargg’s test can output “Passed” via serial which we can check once serial is in place. Keep this test in mind and revisit after implementing Timer/Serial.

- **MMU & Memory Map** – *Dep: CPU (so we can test memory ops).*
  
  - Flesh out the MMU to cover all regions:  
    
    * [x] Map 0x0000-0x7FFF and 0xA000-BFFF through Cartridge module.
    * [x] Allocate 8KB for WRAM, and if CGB mode, 8 banks (bank 1-7) of an additional 4KB each (total 32KB, though one bank is fixed at 0).
    * [x] Map WRAM bank 0 at C000-CFFF, switchable WRAM bank at D000-DFFF (default to bank 1). Implement writes to FF70 (SVBK) to change the bank (on DMG, ignore it).
    * [x] VRAM: allocate 8KB (DMG) or 2×8KB (CGB). Map bank 0 (or selected bank) at 8000-9FFF. Implement FF4F register to switch VRAM bank on CGB.
    * [x] HRAM: 127 bytes at FF80-FFFE.
    * [x] OAM: 160 bytes at FE00-FE9F.
    * [x] Echo RAM: E000-EFFF mirror of C000-CFFF, F000-FDFF mirror of D000-DDFF. We can handle this by masking address (addr & 0x1FFF) and using the same WRAM array. Writes to echo should write to main RAM. Reads likewise.
    * [x] Unusable memory FEA0-FEFF: if read, return 0xFF; if written, ignore.
  
  - [x] Implement read/write functions in MMU that do an address range check and dispatch appropriately. This will be a large match or if-else chain.
  
  - Integrate PPU, APU, Timer, Input, Serial into MMU: at this stage, those modules might be partially implemented or at least have their data arrays.  
    
    * [x] For PPU registers (FF40-FF4B, etc.), have MMU call `ppu.write_reg(addr, val)` and `ppu.read_reg(addr)` which we will implement soon.
    * [x] Same for APU (NR10-NR52): call `apu.write_reg(addr,val)`. Initially, we can stub APU responses (e.g. return 0 for reads or specific default values, since APU registers often have unused bits returning 1).
    * [x] Timer registers FF04-FF07: MMU can directly update Timer module (e.g. writing FF04 calls `timer.reset_div()`, writing FF05 sets TIMA, etc.). Or the Timer module can expose read/write.
    * [x] IF (FF0F) and IE (FFFF): These can be stored in an `interrupt` module or simply in MMU. We can store `mmu.if` and `mmu.ie`. Writes to them update the value, reads return current.
    * [x] Joypad (FF00): MMU reads from Input module (combining state with select bits), writes might set the select bits (in practice, games write to FF00 to select mode – we track those bits).
    * [x] Serial (FF01, FF02): Writes to FF01 store the byte (SB). Writes to FF02 with bit7=1 trigger transfer (so call serial module to handle it). Reads return SB or status.
  
  - Boot ROM handling:

    * [x] If using a boot ROM, load it into an array. MMU at 0x0000-0x00FF should return boot ROM bytes until a flag `boot_completed` is set (when FF50 is written).
    * [x] Implement write to FF50: set `boot_completed=true`, after which reads from 0x0000 go to cartridge ROM. Ensure FF50 write only works once (actual hardware cannot re-enable boot ROM without reset).
  
  - [x] Test: Write unit tests for MMU addressing (e.g., write to an address and read back to confirm it goes to correct memory). Also, test bank switching logic: e.g., set VRAM bank via FF4F and verify reads go to different memory locations.

- **PPU Implementation (Rendering)** – *Dep: CPU (for timing), MMU (for memory interface).*
  
  - Implement PPU registers and internal data:  
    
    * LCDC (FF40) bits: control enabling BG, sprites, sprite size, BG tilemap base, tile data base, window enable, window tilemap base, LCD on/off.  
    * STAT (FF41) bits: the mode bits (0-1), coincidence flag (bit2), and interrupt enable flags for LY=LYC, OAM, VBlank, HBlank.  
    * SCY, SCX (scroll), LY, LYC, WY, WX, BGP, OBP0, OBP1 (DMG palettes).  
    * If CGB mode: BG palette index/register (FF68/69) and OBJ palette index/register (FF6A/6B), which allow access to CGB palette tables.  
    * OAM memory for 40 sprites (each 4 bytes: y, x, tile, flags).  
    * VRAM memory for tiles and maps (with 2 banks in CGB).
  
  - PPU step function:  
    
    * Aim for a line-based timing at first: e.g., accumulate cycles until 456, then that constitutes one scanline. Within that, you can subdivide: OAM scan maybe 80 cycles, drawing 172 cycles, HBlank 204 cycles (these add to 456). But perhaps implement a simpler model:  
    - Keep a counter, add cycles. Determine current mode based on counter range.  
    - When counter < 80 (mode 2), when it hits 80 switch to mode 3; when it hits 80+172=252 switch to mode 0; when it hits 456, reset counter to 0, increment LY, potentially change mode to 2 of next line or to 1 if LY==144.  
    - When LY == 144, enter VBlank (mode 1) and stay in mode 1 for 456*10 lines (10 lines of VBlank). During VBlank, the PPU can just increment LY every 456 cycles without rendering.  
    - When LY goes from 153 back to 0, that’s end of VBlank, new frame.  
    * Implement mode transitions and set STAT mode bits accordingly.  
    * Trigger STAT interrupt if enabled when entering OAM (mode2), entering VBlank (mode1) or entering HBlank (mode0). Also check LY==LYC: if LY equals LYC at the *compare time*, set STAT coincidence flag and trigger interrupt if enabled[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,downsampled%20from%202MHz%2C%20and%20accurate).  
    * Trigger VBlank interrupt when entering mode1 at line 144.  
    * Rendering: For now, implement a straightforward pixel generation during mode 3:  
    - If LCDC says BG enable or the game is in DMG mode (BG cannot be turned off on DMG, it’s always enabled at least as a white background), draw background: For each x of the line, determine the tile from the background tile map (using SCX, SCY plus the current LY), fetch tile data from pattern table (taking into account LCDC tile data area and signed tile indices if using 8800-97FF area), get the correct tile line (using SCY+LY mod 8), and palette color.  
    - If window is enabled (LCDC bit and if current line >= WY and WX in range), at WX position, switch to drawing window tiles instead of background.  
    - Draw sprites: After drawing BG/window for the line, iterate sprites in OAM (or better, pre-collected during mode 2 OAM scan which we can simulate by collecting the first up to 10 sprites on this line, since only 10 sprites can be drawn per line). Sprites have priority: lowest OAM index has highest priority if overlapping (typical for sprites). If sprite x is in range for this pixel and its priority allows (OBJ-to-BG priority respect), draw sprite pixel (using its tile data, which might come from second tile bank on CGB if specified, and using OBP0/OBP1 or CGB palette).  
    - Mark sprite pixel as drawn to enforce the 10 sprites/line limit.  
    - Write the final pixel color into the frame buffer.  
    - This can be simplified initially (e.g. ignore priority and just draw sprites after background, or implement without window). But to pass many games’ visuals, eventually implement fully.  
    * Support both 8x8 and 8x16 sprite sizes (LCDC bit 2).  
    * If CGB mode, use tile VRAM bank as specified by tile attributes (BG tile can come from bank 0/1, sprite tiles similarly).  
    * If performance becomes an issue for rendering, consider optimizing using lookup tables (for tile decoding) or caching tile graphics, but correctness first.
  
  - As each line is rendered, the PPU could optionally output it immediately (some emulators do scanline rendering). But easier is to render to frame buffer and after LY=143, when going into VBlank, we know the frame is done and we can copy or present the frame.
  
  - Handle LCD on/off: If LCD is off (LCDC bit7 = 0), PPU should not draw anything and LY will stay at 0 (or goes immediately to 0?). Actually, turning off LCD mid-frame resets LY to 0 within 1-2 lines and PPU stays in VBlank state effectively. We might handle this by if LCD off, we don’t do normal mode stepping, maybe just reset LY. This is an edge case; games rarely turn it off except to load tiles quickly.
  
  - Test: To validate PPU, one approach: run known good ROMs like the Nintendo logo (boot ROM) or a simple homebrew that draws something. Alternatively, use test ROMs from the `mooneye-gb` suite (there are many PPU tests). You can also write a simple program to draw a pattern in tile data and see if the frame buffer matches expected values. If using boot ROM, when boot ROM finishes, it displays the Nintendo logo – check that it appears correctly (requires the front end to actually show the frame).

- **Basic Frontend (Window + Input)** – *Dep: PPU (to get frame), maybe APU (for audio toggle).*
  
  - Create a window using chosen library (e.g. using `minifb`: create Window, or using `winit`: create event loop and window).
  
  - For `minifb`: set up a 160x144 framebuffer (or scaled to 2x, etc.). For `pixels`: set up a surface for 160x144.
  
  - Each iteration of the emulation loop, once a frame is ready (e.g. after PPU runs LY 0-143 and hits VBlank end), update the texture/pixel buffer and blit to window.
  
  - Manage timing: Ideally, throttle to ~59.7 FPS (GB frame rate). We can use vsync via the window (if available) or manually sleep the thread to control speed. In early development, running unthrottled is fine (especially if CPU is not heavy yet, it will run too fast – but that’s where a frame limiter is needed for actual play).
  
  - Input handling: For `minifb`, use `get_keys_pressed()` etc., for `winit`, handle `WindowEvent::KeyboardInput`. Map keys to Game Boy buttons (e.g., Up/Down/Left/Right arrows, Z = A, X = B, Enter = Start, Backspace = Select as defaults[github.com](https://github.com/mvdnes/rboy#:~:text=Gameplay%20Keybindings)).
  
  - Update the Input module’s state accordingly (set bits for pressed buttons). If a button transitions from not pressed to pressed, set the joypad interrupt flag (IF bit 4) in the Interrupt controller.
  
  - Provide a way to close the emulator (window close event breaks loop).
  
  - Test: Run a simple ROM (like Tetris or Dr. Mario, which don’t require MBC) to see if you get graphics and can control. Many things might still be missing (timing, etc.), but this will shake out integration issues.

- **Timers & DIV** – *Dep: CPU (timing), MMU.*
  
  - Implement the Timer module fully as described:  
    
    * Keep track of `div_counter` (16-bit). Perhaps every CPU cycle call `timer.tick()`.  
    * When `div_counter` overflows 0xFFFF -> 0x0000, it automatically wraps (DIV register goes through 00-FF).  
    * Use TAC to determine when to increment TIMA. Simplest: whenever the specific bit of `div_counter` goes from 1 to 0 (falling edge) as per TAC selection, increment TIMA (if timer enabled). Use Pan Docs schematic[gbdev.io](https://gbdev.io/pandocs/Timer_Obscure_Behaviour.html#:~:text=On%20DMG%3A) or simpler approach: maintain a separate counter for TIMA that counts down cycles until next increment.  
    * If TIMA overflows, set TIMA = TMA, request interrupt.  
    * Respond correctly to writes:  
    - Write to DIV: set div_counter = 0 (and thus DIV=0). Also, in doing so, handle the edge-case: if the timer was about to tick at that moment, do we skip it or not? (This is a subtle detail; tests exist for it. We can refine later.)  
    - Write to TAC: note if enabling/disabling timer might immediately cause a tick depending on counter state.  
    - Write to TIMA: if an overflow was in progress (there’s a known glitch where writing TIMA in the short window after overflow and before it reloads from TMA cancels the interrupt). We can simplify initial implementation by ignoring that glitch, then later add if needed for passing test ROMs.  
    * Write to TMA: simply sets the modulo for future overflow reload.
  
  - Integrate with MMU reads/writes (some done in MMU step above).
  
  - Test: Use known timer test ROMs from mooneye or blargg (e.g., “timer/div behavior” tests). Also, verify in a game that uses timer (some games use timer interrupt for periodic tasks – if those tasks happen or not can be noticed).
  
  - Once timer is working, the **Speed of games** will depend on it for certain things (like music tempo if driven by timer interrupt). We also should ensure our CPU loop is generating the correct number of cycles per frame (~70224 cycles per frame for 59.7 Hz) to keep real-time.

- **Interrupt & HALT behavior** – *Dep: CPU, Timer, PPU integration.*
  
  - By now, CPU should be checking for interrupts each iteration. But ensure the following:  
    
    * After an EI instruction, interrupts are not recognized on the immediately following instruction (IME gets set after one instruction delay).  
    * HALT: implement the two cases as per Pan Docs[gbdev.io](https://gbdev.io/pandocs/halt.html#:~:text=If%20,instruction%20is%20first%20executed). If IF & IE == 0 when HALT executed (no pending enabled interrupts), CPU enters low-power HALT (we can simulate by simply not executing any instruction until an interrupt occurs, i.e., break out of the normal loop until IF & IE != 0). If an interrupt is pending but IME=0, then HALT will immediately exit but the next opcode is not skipped (PC doesn’t advance)[gbdev.io](https://gbdev.io/pandocs/halt.html#:~:text=When%20a%20,fails%20to%20be%20normally%20incremented). We can handle this by a flag in CPU like `halt_bug` that indicates to not increment PC on next instruction fetch.  
    * STOP: If STOP is executed and KEY1’s speed switch bit is set (CGB double speed toggle), then switch speed and clear that bit. If STOP is executed with no speed switch, we can simply treat it like a very short HALT (or just continue; on DMG it’s basically a no-op for most emulators, unless we want to simulate power saving which we don’t).
  
  - Ensure IF and IE are respected properly: e.g., if multiple interrupts pending, service in priority order (VBlank highest, Joypad lowest).
  
  - Test: some of blargg’s instr_timing tests and interrupt tests from mooneye will validate this. Also, input interrupt can be tested by seeing if a game responds to button presses without actively polling (some might rely on interrupt, though most poll).

- **Audio (APU) Implementation** – *Dep: CPU (timing), Timer (frame sequencer sync).*
  
  - Implement APU channel classes:  
    
    * Square Channel (1 & 2): fields for frequency timer, duty cycle (from NR11/NR21), envelope (NR12/NR22), sweep (for Ch1 NR10), length counter.  
    * Wave Channel (3): waveform RAM (already in MMU at FF30-FF3F), fields for frequency, length, volume code (NR32), position in waveform, enabled flag.  
    * Noise Channel (4): LFSR state, divisor, clock shift, length, envelope.
  
  - Frame Sequencer: use an internal counter that runs at 512Hz. It ticks 8 steps (0 to 7) in a cycle:  
    
    * Steps 0,2,4,6: clock length counters (if enabled).  
    * Step 2,6: clock sweep (Ch1).  
    * Steps 7: clock volume envelopes.  
    * (This is per Game Boy APU specification.)
  
  - Each APU `step(cycles)`:  
    
    * Accumulate cycles, and also accumulate a fraction towards the next 512 Hz step. When 8192 cycles have accumulated, that’s one frame sequencer tick.  
    * Also, for each channel, decrement their frequency timers by `cycles`. If a channel’s timer <= 0, it means the waveform output should tick: for square wave, flip the waveform output according to duty; for wave channel, advance to next sample; for noise, shift the LFSR. Then reload the frequency timer (which is (2048 - frequency) * 4 for squares, or appropriate formula for others). For noise, the timer period is 2^(shift+1)* (divisor?).  
    * Mix samples: We might generate audio at the same time as emulation cycles, but that’s a lot of samples (4 million per second!). Instead, decide on an output sample rate. A straightforward approach: every X CPU cycles, output one sample. For example, if we choose 44,100 Hz, that’s ~95 cycles per sample (at DMG rate). So we can accumulate a `sample_timer += cycles`, and while `sample_timer >= cycles_per_sample`, do:  
    - Compute the output of each channel at this moment (each channel either outputs a 4-bit value or is silent if disabled).  
    - Mix according to NR50/NR51: each channel to left/right.  
    - Scale to an 8 or 16-bit PCM range.  
    - Store the sample in the ring buffer.  
    - `sample_timer -= cycles_per_sample`.  
    * Handle turning channels on/off: If a channel’s length expires or we write to their disable bits, mark channel as off (no output).  
    * Handle writes to sound registers:  
    - NRX1 (length): writing high bits can set length counter.  
    - NRX2 (envelope): update envelope parameters, maybe even restart envelope.  
    - NRX3/4 (freq low/high): writing high (with trigger bit) should initialize channel: set length if not already running, reset frequency timer, reset envelope to initial volume, etc., and set channel enabled.  
    - NR52 (sound on/off): if bit7 goes 0, turn off all sound (clear channels, etc.).  
    * Many of these behaviors can be guided by Pan Docs and other resources. We can implement a simplified APU that produces recognizable sound, then refine.
  
  - Integrate with `cpal`:  
    
    * Initialize `cpal` audio output stream in stereo. Use a callback that reads from the APU’s sample buffer and fills the output.  
    * If buffer underflows (no sample ready), decide to output silence or block. Ideally, our emulator loop keeps it filled. If we run emulator in real-time, we might run one frame (~16.7ms) of emulation then wait to sync with real time, which naturally feeds audio correctly.  
    * Alternatively, drive emulator by audio callback: not recommended for frame-based systems, easier to produce audio as part of main loop.
  
  - Volume levels: Game Boy audio is not loud; we can multiply the mixed 4-bit sums by a factor to get a nice range. Possibly allow a volume slider in the future.
  
  - Test: Sound is hard to test without listening. Use a known ROM with distinct audio (like a test tone generator or a music ROM) to subjectively verify. Or use blargg’s **dmg_sound** test ROMs for objective pass (they write “Passed” if sound is correct to a certain tolerance).
  
  - Note: Achieving **exact** APU behavior (like channel initial sweep glitch, exact LFSR taps, etc.) is advanced. Aim for getting most games’ audio sounding correct; refine with test ROMs for any errors.

- **Full DMG/CGB Support & Refinements** – *Dep: All components implemented.*
  
  - Ensure CGB mode differences are handled:  
    
    * Double speed mode: When enabled, the CPU will execute instructions effectively twice as fast relative to PPU and APU. Implementation: we can simply halve the number of cycles the PPU/APU think have passed relative to CPU instructions, or conceptually double the CPU clock and adjust timer, etc. Alternatively, treat our normal cycle as 2 T-cycles in double speed. In practice, you might introduce a `cpu_speed` factor of 1 or 2. When 2, the PPU and Timer should get half the ticks per CPU instruction. Another way: if double speed, PPU, timer, etc. run on every *other* CPU cycle. We need to be careful to emulate how DIV behaves (it ticks at same real-time freq, meaning in double speed it increments every 2x cycles).  
    * We already handled VRAM banking, WRAM banking, and extra palettes.  
    * OAM bug is *not present* on CGB[gbdev.io](https://gbdev.io/pandocs/OAM_Corruption_Bug.html#:~:text=Objects%200%20and%201%20,not%20affected%20by%20this%20bug), but present on DMG/Pocket. If we choose to emulate OAM bug, only apply if model is DMG/MGB. This bug can likely be deferred or made optional because few games rely on it (mostly homebrew tests).  
    * Infrared: CGB has an IR port (register RP at FF56). We can ignore or stub (always not connected).  
    * Prepare for other model specifics: (If implementing SGB later, that would involve high-level emulation, not needed now.)
  
  - **Link Cable planning**:  
    
    * Design the interface such that Serial module could either connect to a partner or loopback. Possibly define a trait `LinkPort` with `send_byte(byte)` and `on_receive(callback)`.  
    * For now, implement a stub `NullLinkPort` that when send is called, immediately sets received byte to 0xFF (or if loopback mode, echo back the byte).  
    * Confirm that two instances could be connected by assigning each other’s LinkPort (for future).  
    * Not implementing the actual link functionality now, just ensuring the design can accommodate.
  
  - **Real-time Clock (RTC)** for MBC3:  
    
    * If a ROM uses MBC3 with RTC, we should have at least a basic RTC implementation so games like Pokémon Gold/Silver keep time.  
    * Emulate registers: seconds, minutes, hours, day low/high, and latch mechanism.  
    * Approach: Use system time to seed RTC, and increment an internal counter for time while running. The latch register (0x6000 write 0x00 then 0x01) should copy the current time into the RTC registers so the game can read a stable snapshot.  
    * Save the RTC state (current time or offset) in the save file too, so it persists between runs.  
    * This can be added after core functionality, but listing here for completeness.
  
  - **Performance check**: Run a demanding game or test (like a Pokemon battle with many sprites, or a game with lots of sound) and see if the emulator can maintain full speed. Use the host timers to ensure ~60 FPS. If not, consider optimizations:  
    
    * Use release mode and LTO in Cargo for final build.  
    * If PPU rendering is slow, consider caching tile data or optimizing pixel loops (e.g., unrolling, using SIMD for 8 pixels at once).  
    * If CPU is slow, ensure the opcode match or jump table is not too branchy; possibly use a direct indexed table of function pointers for opcodes.  
    * If audio causes stutter, increase buffer sizes.  
    * Profile to find bottleneck.
  
  - **Accuracy check**: Run the comprehensive test ROM suites and note any failures:  
    
    * **Blargg’s tests**: cpu_instrs, instr_timing, mem_timing, oam_bug, etc.[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,the%20Demotronic%20trick%2C%20Prehistorik%20Man).  
    * **Mooneye-gb tests** (for various obscure behaviors).  
    * Wilbert Pol’s APU tests[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,the%20Demotronic%20trick%2C%20Prehistorik%20Man).  
    * If any fail, address the specific edge case (some known ones we cited: HALT bug, DIV timing, etc.).  
    * Our goal is to pass as many as possible to ensure high accuracy. SameBoy reaches 99.9% compatibility[sameboy.github.io](https://sameboy.github.io/features/#:~:text=emulation%20of%20the%20Demotronic%20trick%2C,optional%20frame%20blending%20modes%2017); we aim to approach that with iterative improvements.

- **Debugging Features** – *Dep: Core features working (for meaningful data).*
  
  - **Logging and Tracing**: Implement an optional CPU instruction trace. E.g., a debug mode where every executed instruction (with PC, mnemonic, register state, cycles) is printed to a log or saved to a file. This helps when comparing against known good emulator traces for debugging.
  
  - **Debugger Breakpoints**: Allow some interface (maybe a console command or a special key) to pause execution. Internally, you can have a global flag to break out of the main loop. Once paused, allow stepping one instruction at a time or dumping state. This could be via a simple REPL on stdin or a TCP socket or integration with `gdbstub` crate to allow GDB connections.
  
  - **Memory Inspection**: Provide a function to dump a region of memory or registers for debugging. Could integrate with the above REPL or simply use logs.
  
  - **VRAM Viewer**: To emulate BGB’s VRAM viewer, one approach:  
    
    * When paused, take the contents of VRAM and interpret them to generate an image of the tile set or background map. For instance, you can render the 256 tiles of the tile data as a 128x128 image. Or render the current BG map (32x32 tiles) using the tiles and palette.  
    * If using a GUI library (like egui or a separate window), you could display these. Alternatively, dump them to a PNG file for external viewing.  
    * At design time, plan the PPU to expose methods to get tile pixel data easily (since PPU already decodes tiles to draw the screen, we can reuse that logic to draw all tiles).
  
  - **Register Viewer**: In debug mode, overlay text of current register values, current instruction, etc., on the screen (if using a library that can draw text, or simply print to console every frame which is spammy).
  
  - Many of these features can be added gradually. The key is that our architecture doesn’t hinder them:  
    
    * We have a global `Emulator` state accessible so any dev tool can read memory, VRAM, etc.  
    * We ensure the emulator can pause (which means breaking out of the tight loop that runs frames).  
    * It might be useful to run the emulation loop step-by-step rather than frame-by-frame when debugging.
  
  - Plan a **CLI flag** like `--debug` to enable a simple interactive debugger. Or even integrate with an existing debugger UI (some emulators allow connecting a remote GDB to the emulated CPU).
  
  - Document how to use these debugging features for developers.

- **Final Polish** – *Dep: All features implemented.*
  
  - Remove or conditionalize any development logs to not flood release version.
  
  - [x] Ensure battery save files are written on exit (and perhaps periodically to avoid loss on crash).
  
  - Graceful shutdown when window closed or user interrupts.
  
  - Packaging: Make sure the emulator runs on Windows (maybe requiring bundling the SDL2 DLL if we used SDL2, or no extra steps if using winit/pixels).
  
  - Update README (if this were a real project) with usage instructions.

The above steps can be grouped into **milestones** such as: *CPU + minimal MMU*, *Basic PPU output*, *Interrupts/Timers*, *Complete PPU*, *Audio*, *Accuracy fixes*, *Debug features*. Tackle them in order, as later ones depend on earlier components working. This checklist ensures that by the end, we have a fully functioning, accurate, and extensible Game Boy Color emulator.

## High-Accuracy Emulation Details

Achieving high accuracy means handling the myriad edge cases and hardware quirks that the Game Boy platform presents. Below we highlight important details to consider (many of which are tested by community test ROMs):

- **Timing Units & Synchronization:** The Game Boy’s master clock is often described in terms of machine cycles (M-cycles) and shorter T-cycles. 1 M-cycle = 4 T-cycles on DMG. Most CPU instruction timings are given in M-cycles. For cycle accuracy, it’s best to simulate at the granularity of 1 T-cycle (the smallest unit) for critical components[sameboy.github.io](https://sameboy.github.io/features/#:~:text=%2A%20Sample,optional%20frame%20blending%20modes%2017). However, this is computationally expensive. A good compromise is to update in 4-cycle increments (one M-cycle), since many hardware events align to those. Our design uses a cycle counter where 1 unit = 1 T-cycle for simplicity, but we might step 4 at a time. For *absolute* accuracy (passing tests for things like the “Demotronic” stat interference trick[sameboy.github.io](https://sameboy.github.io/features/#:~:text=%2A%20Sample,optional%20frame%20blending%20modes%2017) or mid-scanline timing), one might need to go to T-cycle level. The architecture should allow reducing step size if needed (e.g. change PPU to process 1 cycle at a time).

- **Interrupt Timing:** An interrupt, when triggered, does not fire immediately if the CPU is in the middle of an instruction. It will only be serviced at the next instruction boundary (except for STOP/HALT behavior). The exception is that EI enables interrupts after the *following* instruction, and DI disables immediately. Also, the order of checking multiple interrupts, and the fact that the act of servicing (jumping to vector) automatically disables IME until re-enabled. Our CPU loop must handle this carefully: typically, after each instruction execution, if IME=1 and IF & IE != 0, determine the highest priority set bit and service that. Also, the delay of one instruction after EI can be handled by an `ime_delay` flag.

- **HALT bug:** Already described, but to reiterate: if interrupts are disabled (IME=0) and an interrupt is **pending** (IF & IE != 0) when HALT executes, the CPU will not halt; it will continue as if a NOP, *but* the next instruction’s fetch is skipped (PC doesn’t advance, effectively executing the same byte again)[gbdev.io](https://gbdev.io/pandocs/halt.html#:~:text=When%20a%20,fails%20to%20be%20normally%20incremented). We must implement this or some games (and tests) break.

- **OAM Corruption Bug:** Present on DMG/GBP but fixed on CGB[gbdev.io](https://gbdev.io/pandocs/OAM_Corruption_Bug.html#:~:text=Objects%200%20and%201%20,not%20affected%20by%20this%20bug). This obscure bug involves certain instructions (INC/DEC of 16-bit regs, or LD (HL+/-)) executed at a precise time during PPU mode 2 causing corrupt data to be written to OAM[gbdev.io](https://gbdev.io/pandocs/OAM_Corruption_Bug.html#:~:text=There%20is%20a%20flaw%20in,PPU%20is%20in%20mode%202)[gbdev.io](https://gbdev.io/pandocs/OAM_Corruption_Bug.html#:~:text=,or%20write%20is%20not%20asserted). Very few games rely on this (if any), but test ROMs do. In our emulator, we could simulate it by checking if an OAM memory access occurs at mode2 and if the instruction is one of those, then corrupt OAM (details of corruption pattern are complex). Given its rarity, this can be a low-priority fix, toggled only in DMG mode. SameBoy does emulate it since it’s extremely accuracy-focused.

- **Timing of LCD Events:** To be pixel-perfect, the PPU must change modes and trigger interrupts at the exact cycle. For example, LY=0 and mode2 at cycle 0, switch to mode3 at cycle ~80, mode0 at cycle ~252, etc., and LY=144 (VBlank) at the right time. If these are off, certain LCD trickery (scanline effects, etc.) won’t work. The STAT interrupt for LY==LYC should fire at the *compare moment* (which occurs during the first T-cycle of line when LY increments to LYC, or immediately when LYC is written matching current LY if that happens). Also, if LYC == 144 and LY hits 144 at VBlank, both the STAT (LYC) and VBlank interrupts fire. Order might matter if one clears a condition. Our PPU implementation should be tested against known tricky cases (there are test ROMs like “stat_irq” in mooneye).

- **Memory Access Conflicts:** The Game Boy has scenarios where certain memory is inaccessible or behaves oddly:
  
  - VRAM cannot be accessed by CPU during mode 3 (PPU rendering). If CPU tries, on real hardware it typically returns the last fetched byte on the data bus (or locks out). We might simplify by preventing CPU access or returning 0xFF for reads. Some test ROMs check this by writing to VRAM mid-frame and seeing if it takes effect (it shouldn’t until VBlank).
  
  - OAM cannot be accessed during mode 2 or 3 by CPU. Writes are ignored or delayed and reads return 0xFF (except maybe OAM bug cases). We should simulate that in MMU: if in mode2/3, and CPU does OAM read/write, just return 0xFF or ignore.
  
  - During OAM DMA, for 160 cycles, CPU can only access HRAM (FF80-FFFE). If it tries other areas, it gets locked out. We can implement this by halting CPU for 160 cycles (easy method), or by setting a flag “oam_dma_active” and in MMU making any non-HRAM access a NOP during those cycles.

- **DMA and HDMA on CGB:**
  
  - Normal DMA (0xFF46) copies from (source * 0x100) to OAM. On DMG, it blocks CPU. On CGB, it also blocks CPU but IIRC CGB can do double speed, but effectively still 160 microseconds.
  
  - HDMA (CGB): If game initiates an HDMA (FF55 write), it can choose mode: General DMA (do all at once, locking CPU for ~$10 * bytes cycles) or HBlank DMA (transfer 16 bytes each HBlank, not locking CPU except for a couple cycles each transfer). HBlank DMA is tricky: we need PPU to coordinate starting a small DMA at each HBlank until done. If the game writes to FF55 to start it, we should implement a mechanism in PPU’s HBlank handler to do these copies and decrement the counter (and clear the HDMA active flag when done).
  
  - If we skip implementing HDMA initially, CGB games that use it (some later games) will have graphic issues. So perhaps implement the basic HBlank DMA after core PPU is working.

- **Undocumented Instructions & Behaviors:** Game Boy has a couple of “undocumented” opcodes (0xD3, 0xDB etc.) which are essentially no-ops on real hardware or behave like other instructions (some sources say they read/write from wrong ports). Most games don’t use them, but a few homebrews or test ROMs might. We can implement them as NOPs or as the documented unofficial behavior.

- **Initial Register Values:** After boot ROM, certain registers have specific values (e.g., DMG: A=0x01, F=0xB0, B=0x00, C=0x13, D=0x00, E=0xD8, H=0x01, L=0x4D, SP=0xFFFE) – if we skip boot, we must set these to mimic boot ROM【63†22. Power-Up Sequence】. Also, IE = 0x00, IF = 0xE1 (on DMG). On CGB, registers differ slightly (A=0x11, etc.). Using the boot ROM will naturally set these, but if skipping, do it manually to avoid misbehavior (some games assume those values).

- **Serial Transfer Timing:** If implementing link, the exact timing of bits matters if two GBs run not in lockstep. Could be left for future; for now, just ensure after initiating transfer, we eventually flag it complete. Serial may also require proper handling of fast CGB double-speed mode (CGB can double serial clock if bit1 of SC).

- **Peripheral Quirks:** Unused bits in I/O registers often read back as 1. For example, only some bits of SC are used, the others return 1. We should ensure MMU returns the correct masked values. Pan Docs usually states which bits are unreadable or always 1.

- **Game Boy Color specific**:
  
  - Some games running on GBC use the extra VRAM bank and object palettes, etc., which we handle. Also, GBC double-speed mode affects the divider (DIV runs twice as fast in terms of incrementing? Actually, in double speed, the clock is doubled, so to maintain same real time, DIV’s increment rate doubles too – meaning if we base everything on CPU cycles, it’s consistent, but from a “time” perspective, things happen faster. We want the game logic to go faster only if the game intentionally switches speed).
  
  - GBC has an undocumented quirk that the boot ROM enables the IR port or something that some games check to detect if they’re on GBC vs GBA. GBA in GBC mode has slightly different behavior (the boot ROM for GBA is different). SameBoy includes an AGB mode to emulate GBA differences[forums.libretro.com](https://forums.libretro.com/t/regarding-the-sameboy-core/12817/38#:~:text=The%20GBA%27s%20CGB%20boot%20ROM,CGB%20boot%20ROM)[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,SGB2). We likely won’t emulate GBA’s differences (like the slightly different palette initial values or timing).
  
  - Work RAM (WRAM) on CGB: when switching banks via SVBK, reading from the banked area while switching could produce some weird behavior (not likely, but ensure bank switching is instantaneous in our emu).

- **Testing against known edge cases:** For example:
  
  - The “DI + HALT” bug: if a game does DI followed by HALT (with no EI in between), on real hardware it halts forever because interrupts are disabled and none can wake it. But on CGB (due to a hardware quirk) it actually doesn’t halt at all. There’s a small difference between DMG and CGB here. Most emulators mimic DMG behavior for both unless specifically emulating CGB precisely. It’s an esoteric scenario, but documented. Not high priority unless a test demands it.
  
  - **Demotronic (Tech demo)**: uses LCD trick that requires exact timing of mode and interrupt handling. If we aim to support that, our PPU and CPU sync must be nearly perfect. Possibly a stretch goal.

By addressing these details, the emulator’s accuracy will greatly increase. Many of these are confirmed by the fact SameBoy passes all known test suites[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,the%20Demotronic%20trick%2C%20Prehistorik%20Man). We strive for the same level. Each quirk can be added one by one, using test ROM feedback to guide us.

## Testing Strategy

Testing is essential throughout development to ensure each component works correctly and that changes don’t introduce regressions. We will employ several testing methodologies:

### Unit Tests (Module-Level Tests)

Use Rust’s unit test framework within each module to test the smallest pieces of functionality in isolation:

- **CPU instruction tests:** We can write tests for individual opcodes. For example, set up a `Cpu` with known register values and a tiny memory containing the opcode and operands, execute one step, and check that registers, flags, and PC are as expected. This can catch mistakes in opcode implementations early (e.g., flags on ADC, behavior of corner cases like 0xFF + 0x01 causing half-carry).

- **Timer tests:** Simulate specific scenarios: e.g., write TAC to select a freq, then call `timer.step` appropriate cycles and see that TIMA increments at the right times, overflows correctly, etc. Also test DIV reset by writing to DIV and see that TIMA behaves according to known obscure case (if we implement it).

- **MMU tests:** As mentioned, verify address mapping. For example, write to an address in each memory region and ensure the data lands in the correct backing store. Test bank switching: set up an MBC with multiple banks, switch and test that reading the same address yields data from different banks.

- **APU channel tests (if possible):** This is harder without hearing, but we can test some logic like volume envelope calculations or that the frequency timer reload values are correct. Alternatively, test writing to registers produces expected internal state (like writing to NRX4 with trigger sets channel enabled).

- **PPU tests:** The PPU is complex to unit test because it involves sequence over time. But we can test smaller functions like “fetch tile index from BG map given X,Y scroll” or “decode a tile line to pixels given tile data and palette”. We might provide known tile data and ensure the bitplane decoding works.

- **Utility tests:** e.g., test the bitflags for interrupts or joypad.

The idea is to catch as many errors as possible in isolation, which makes debugging easier than only running full games.

### Integration Tests (Full-System Automated Tests)

These tests run the emulator with real ROMs or test ROMs in a controlled way:

- **Blargg’s Test ROMs:** These are a collection of `.gb` files that test specific things:
  
  - cpu_instrs.gb – CPU instructions correctness.
  
  - instr_timing.gb – instruction timing.
  
  - mem_timing.gb – memory access timing.
  
  - oam_bug.gb – tests OAM bug behavior.
  
  - dmg_sound.gb – tests APU (for DMG).
  
  - cgb_sound.gb – tests APU differences on CGB (like higher-quality wave channel, etc.).
  
  - etc.

  Each of Blargg’s tests, when run, will output either “Failed [code]” or “Passed” via the serial port or by writing to VRAM (blargg’s usually uses serial). We should capture this:

- Implement in the Serial module: if a special flag is set (like `--serial` CLI), then every byte sent to serial (SC write 0x81) is collected into a buffer or printed as ASCII. Blargg’s tests print logs and final result as text[github.com](https://github.com/mvdnes/rboy#:~:text=Options%3A%20,Print%20help).

- We can automate: run each test ROM for a certain number of emulated frames (or until we see “Passed” or “Failed” in the serial output or a certain memory location).

- We might create a cargo integration test that loads each ROM, steps the emulator until completion condition, and asserts that the output contains “Passed”. This can be time-consuming to automate fully but is extremely useful for non-regression.

  For example, the Rustfinity Gameboy project includes an example to run blargg’s tests and prints results[rustfinity.com](https://www.rustfinity.com/open-source/gameboy#:~:text=Tests).

- **Mooneye-gb Test Suite:** This suite has many small ROMs targeting specific hardware behaviors (like “halt_ime0” for halt bug, “div_write” for DIV timing, “dma_basic”, “hdma_edge_cases”, etc.). We can run these similarly. Some of them communicate results by writing specific values to memory or register (not always via serial). The Mooneye tests often indicate success by what value they put in AF registers or so when the ROM ends. We’d need to read documentation on each test or simply check known expected behavior (some output to serial as well in a simple protocol).

- **Graphics Tests / Visual Verification:** There are manual test ROMs that display certain patterns on screen if things are correct. For instance, there is the “lcd_timing” test that shows a grid if PPU timing is right. For these, an automated test could compare the emulator’s frame buffer to an expected image. That’s advanced and brittle if not pixel-perfect. Alternatively, run them and manually verify or take screenshots.

- **Game-Specific Tests:** Short of playing a whole game, we can identify moments that stress certain features:
  
  - Does the opening of “Pokemon Yellow” play the Pikachu cry correctly (tests sound channel frequency sweep)?
  
  - Does “Link’s Awakening” have correct music tempo (timer + audio)?
  
  - Do colors in a GBC game (say Zelda Oracle of Ages) appear correctly (testing palette implementation)?
  
  - These are subjective tests, but useful once we reach a certain point.

- **Performance Tests:** Not an official suite, but measure the emulator speed (e.g., how many frames per second in unlimited mode vs real-time). Ensure that with release build, we can sustain full speed on target machines. If not, go back to optimize.

### Regression Testing

As new features are added or edge cases fixed, maintain a set of quick tests that can be run to ensure previously working functionality hasn’t broken:

- Re-run Blargg’s CPU tests whenever CPU code is touched.

- Re-run sound tests if APU is modified.

- Use continuous integration to run these automatically if possible (would require headless mode or at least serial outputs).

### User Testing (Playtesting)

Finally, playing a few games entirely (or significant parts) is the best real-world test:

- Choose a mix of games: original DMG games and GBC games. For DMG, e.g., Super Mario Land, Tetris, Pokémon Red. For GBC, Mario Bros Deluxe, Wario Land 3, Pokémon Crystal.

- While playing, use debug tools to monitor if any unusual logs pop up (e.g., unimplemented features).

- Watch for graphics glitches (indicating PPU issues) or crashes (possibly bad opcode handling or interrupt mismanagement).

- Listen for audio issues (pops, incorrect pitches).

- If link cable were implemented in the future, test two-player games or trading (beyond initial scope).

### Testing Save & Persistence

- Verify that saving in a game (like saving in Pokémon) writes to cartridge RAM and that on closing and reopening the emulator, the save persists (i.e., our .sav file logic works).

- Make sure saving doesn’t occur too frequently (to avoid disk wear) but also not lost if crash (maybe save on game exit or periodically).

By using a combination of these strategies, we ensure confidence in each part of the emulator. Notably, the community test ROMs (blargg’s, Mooneye, etc.) are critical; passing them is a strong indicator of accuracy[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,the%20Demotronic%20trick%2C%20Prehistorik%20Man). Our implementation plan integrates testing at every milestone.

## Debugging Features Design

While the first goal is to get the emulator working, adding debugging tools will greatly enhance development and also make the emulator useful for developers (much like BGB and SameBoy include powerful debuggers[sameboy.github.io](https://sameboy.github.io/features/#:~:text=Debugging)). We outline a design for debugging features:

- **Instruction Logger:** As mentioned, implement a logger that can record each executed instruction with register states. This could be controlled by a debug flag or even toggled at runtime (e.g., press a key to start/stop logging). Logs can be written to a file or stdout. Be careful to not significantly slow down emulation when not in use (so guard the logging with `if debug_logging { … }`). The format could mimic BGB or SameBoy’s debugger one-line disassembly format, e.g., `00:8000 NOP A=01 F=B0 B=00 C=13 D=00 E=D8 HL=014D SP=FFFE` etc.

- **Breakpoint Support:** Allow setting breakpoints at certain addresses or when a certain condition is met. This could be done via:
  
  - A simple array or `HashSet<u16>` of breakpoint addresses; each instruction fetch, check if PC is in that set, if so, pause.
  
  - Conditional breakpoints (based on register or memory values) are more complex; might skip for initial implementation.
  
  - UI: since we don’t have a full GUI debugger yet, breakpoints could be configured via a config file or hardcoded for testing. Eventually, an interactive console or GUI could manage them.

- **Step Execution:** When paused (breakpoint hit or user pause), implement a “step” function that executes one instruction, updates state, then pauses again. Bind this to a key or debugger command.

- **Live Memory Inspector:** Provide a way to dump memory content. In a simple form, on pause, print 16 bytes around a certain address in hex + ASCII. Or allow the user to query an address. If integrating with a UI, one could have a scrollable hexdump view.

- **Register Modification:** In a debugger, allow changing register values or memory to experiment. This is advanced and would require a command interface.

- **VRAM/Tiles Viewer:** This can be either an in-emulator GUI overlay or an external dump:
  
  - The emulator can generate image files of VRAM content. For example, create a 256x256 PNG of all tiles in VRAM (16 tiles across, 24 down for 384 tiles in CGB mode). Or generate two images for tile data from each bank. Also, generate an image of the BG map (with tiles drawn using current tileset and palette).
  
  - These could be written to disk when triggered (press a debug key to dump VRAM).
  
  - A more interactive approach: if using egui (a Rust immediate mode GUI that can integrate with winit), one could create a debug window that shows the tile atlas and updates it each frame in pause mode.
  
  - SameBoy’s debugger includes a VRAM viewer in its Cocoa frontend[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,Video%20RAM%20viewer%208) – for us a simple approach is fine initially (dump to file).

- **Audio Debugging:** Perhaps allow dumping the audio buffer to a WAV file for analysis. Or at least printing out some register state of APU (like current frequency, volume levels).

- **State Consistency Checks:** In debug mode, run sanity checks – e.g., verify that the CPU cycle count is synchronized with PPU LY and timer, or that no unknown opcode was encountered. If something goes off rails, log an error or trap.

- **Link Cable Debugging:** If we implement link, debug feature could simulate a second player input easily or provide logs of link data sent/received.

**Integrating Debug Mode:**

- We can have a global flag or compile-time feature for debugging. Compile-time (with `#[cfg(debug_assertions)]`) can include extra checks or logs. But runtime toggle is more flexible (so user can switch on/off).

- Possibly have a separate main function for debugging that runs the emulator with a CLI debugger (text prompts).

- Or integrate with an existing debugger protocol. The `gdbstub` crate can allow the emulator to interface with GDB, meaning you could use GDB or an IDE to put breakpoints in the emulated code (by treating the Game Boy CPU like a remote target). That is advanced but known to be implemented in some emulators (for NES, etc.).

**User Interface for Debugging:**

- In the absence of a full GUI, a simple approach:
  
  - Pressing, say, F1 could pause/resume.
  
  - F10 could step one instruction.
  
  - F2 could dump registers to console.
  
  - F3 could dump a segment of memory.
  
  - These can be handled in the window event loop (if key pressed and debug mode active, perform action instead of sending to game).
  
  - On console (stdout), the output of debug actions can appear.

If we want to build a more user-friendly UI:

- Could embed an egui panel alongside the game screen showing registers and allowing step, etc. Egui can draw on top of pixels/wgpu context, making a mini UI. However, that adds complexity and is optional.

**Documentation & Usage:**

- We should document how to use the debugger in our README or help output. e.g., “Press F1 to pause, F2 to step, etc., when run with --debug mode.”

- Possibly require `--debug` flag to enable these key bindings, otherwise those keys might conflict with game controls.

With these debugging features, developers using our emulator can inspect and trace program execution, which is invaluable for development of homebrew or understanding ROM behavior. It also helps us diagnose issues in the emulator itself by seeing exactly where a test fails.

Finally, because we decided not to implement save states (to keep things simpler), our debugging cannot rely on quickly saving/restoring state. This means stepping through code from power-on each time which can be slow. In future, we could implement an unofficial state save just for debugging (like snapshots) but that remains out-of-scope per requirements. Instead, we rely on fast forwarding (we could implement a toggle to un-throttle emulation to speed through frames when debugging, then slow down near the area of interest).

---

By following this comprehensive design and checklist, we will create a robust, accurate Game Boy Color emulator in Rust. The modular structure (inspired by SameBoy’s architecture) will ease maintenance and future enhancements. We emphasized **cycle accuracy and correctness** (passing strict test suites[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,the%20Demotronic%20trick%2C%20Prehistorik%20Man)), while also considering performance optimizations to ensure the emulator is **playable in real time**. Additionally, we laid groundwork for extended features like a debugger and link cable support, which can be incrementally added. With careful implementation and thorough testing, the emulator will handle classics and edge cases alike, providing both a gaming experience and a development tool for the Game Boy platform.

Sources:

- SameBoy Emulator Features and Accuracy[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,downsampled%20from%202MHz%2C%20and%20accurate)[sameboy.github.io](https://sameboy.github.io/features/#:~:text=%2A%20Sample,optional%20frame%20blending%20modes%2017)[sameboy.github.io](https://sameboy.github.io/features/#:~:text=,Video%20RAM%20viewer%208)

- RBoy Rust Game Boy Color Emulator (features implemented)[github.com](https://github.com/mvdnes/rboy#:~:text=,Timer)[github.com](https://github.com/mvdnes/rboy#:~:text=%2A%20MMU%20%2A%20MBC,Printing)

- Pan Docs (GB technical reference) for hardware behaviors[gbdev.io](https://gbdev.io/pandocs/OAM_Corruption_Bug.html#:~:text=There%20is%20a%20flaw%20in,PPU%20is%20in%20mode%202)[gbdev.io](https://gbdev.io/pandocs/halt.html#:~:text=When%20a%20,fails%20to%20be%20normally%20incremented)[gbdev.io](https://gbdev.io/pandocs/Timer_Obscure_Behaviour.html#:~:text=Notice%20how%20the%20bits%20themselves,causes%20a%20few%20odd%20behaviors)

- Rustfinity GameBoy project (use of cpal and minifb, testing with Blargg ROMs)[rustfinity.com](https://www.rustfinity.com/open-source/gameboy#:~:text=Gameboy%20relies%20on%20several%20Rust,libraries%20with%20native%20dependencies)[rustfinity.com](https://www.rustfinity.com/open-source/gameboy#:~:text=Tests)

- Copetti’s analysis (Game Boy Color architecture and timing)[copetti.org](https://www.copetti.org/writings/consoles/game-boy/#:~:text=cutting,8.38%20MHz)
