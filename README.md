# VibeEmu

This project is a Game Boy Color emulator named VibeEmu written in Rust.

## Current State

The project is in the early stages of development, with a focus on implementing the core components of the Game Boy Color such as the CPU, PPU, memory management, and other peripherals. It is an ongoing effort to bring the classic handheld gaming experience to modern platforms.

This project is meant for fun where Jules.google.com is writing the majority of the code 'vibe coding' as a fun experiment on asynchronous AI coding agents.

## Building, Running, and Testing

This project uses Cargo, the Rust build system and package manager, for managing the build, execution, and testing processes. Ensure you have Rust and Cargo installed. You can find installation instructions on the [official Rust website](https://www.rust-lang.org/tools/install).

**Linux Users Note**: For audio functionality, you will likely need to install the `libasound2-dev` package (or a similar package like `alsa-lib-devel` for Fedora-based distributions). This is because the `cpal` crate, used for audio output, often relies on ALSA on Linux systems. You can typically install it using your distribution's package manager, for example:
```bash
sudo apt-get install libasound2-dev
```

Once Rust and Cargo are set up (and any system dependencies like `libasound2-dev` are installed for Linux users), navigate to the project directory in your terminal and use the following commands:

### Build the Emulator
```bash
cargo build
```
This command compiles the emulator source code. You typically run this after fetching new changes or modifying the code.

### Run the Emulator
```bash
cargo run
```
This command builds (if necessary) and then runs the emulator.

To run the emulator with a specific ROM file, you pass the path to the ROM as an argument after `--`. For example:
```bash
cargo run -- path/to/your/rom.gb
```
Refer to the "Command-Line Arguments" section for other arguments you can use with `cargo run`.

### Run Tests
```bash
cargo test
```
This command executes the automated tests for the project to ensure components are working as expected.

## Command-Line Arguments

The emulator accepts the following command-line arguments:

-   `<path_to_rom>`: Specifies the path to the Game Boy Color ROM file you want to load.
    -   **Default**: If not provided, the emulator will attempt to load a default ROM, if configured, or may not start. (Note: The exact default behavior for a missing ROM path might need to be defined or clarified based on the emulator's current implementation).
-   `--headless`: Runs the emulator without a graphical interface. This is useful for testing, running automated tasks, or when a visual output is not needed.
-   `--halt-time <seconds>`: When running in `--headless` mode, this argument specifies the number of emulated seconds to run before the emulator automatically halts.
    -   **Default**: If not specified in headless mode, the emulator might run indefinitely or have a predefined default halt time. (Note: The exact default behavior should be clarified based on implementation).
-   `--halt-cycles <cycles>`: Specifies the number of CPU cycles to emulate before the emulator automatically halts. This is useful for debugging and testing specific points in a ROM's execution.

## Keyboard Controls

The emulator uses the following keyboard mappings for Game Boy Color controls and other actions:

-   **D-Pad**: Arrow Keys (Up, Down, Left, Right)
-   **A Button**: 'Z'
-   **B Button**: 'X'
-   **Start Button**: 'Enter'
-   **Select Button**: 'RightShift' or 'LeftShift'
-   **Exit Emulator**: 'ESC'
-   **Pause/Resume & Context Menu**: 'Right Mouse Click Release' (Releasing the right mouse button toggles pause/resume and opens a context menu if available)

## Key Dependencies

VibeEmu leverages several external Rust crates to provide its functionality. Here's an overview of the main dependencies:

-   **`minifb`**: Used for creating a simple window and rendering the emulator's display (pixel buffer).
-   **`cpal`**: Handles cross-platform audio output, allowing the emulator to play Game Boy Color sound.
-   **`egui` and `eframe`**: These crates are part of the `egui` immediate mode GUI library. `eframe` is used as the application framework, and `egui` is used to build the debug interface, context menus, and other UI elements.
-   **`image`**: Utilized for image decoding and manipulation, which can be helpful for tasks like loading ROM icons or handling screenshot functionality.
-   **`crossbeam-channel`**: Provides efficient channel-based communication, likely used for message passing between different parts of the emulator (e.g., between the UI thread and the emulation thread).
-   **`native-dialog`**: Enables the use of native system dialogs, such as file pickers for loading ROMs.

Understanding these dependencies can provide insight into the emulator's architecture and how different aspects like graphics, audio, and UI are handled.

## Future Development

Future tasks will involve implementing the CPU, PPU, memory management, and other components of the Game Boy Color.
