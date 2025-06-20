# vibeEmu

vibeEmu is a Game Boy and Game Boy Color emulator written in Rust. The current implementation contains the basic building blocks such as the CPU core and a simplified memory system. It is meant as a learning project and starting point for a more complete emulator.

## Building

Ensure you have a recent Rust toolchain installed. To build the project run:

```bash
cargo build
```

The frontend uses `winit` and `wgpu` for windowing and GPU rendering. On Linux
you will need a working X11 or Wayland environment. Audio output relies on
`cpal`, which requires ALSA headers. Install `libasound2-dev` if you build on
Linux.

## Running

The emulator expects the path to a ROM file. The command below will start the emulator in CGB mode by default:

```bash
cargo run -- path/to/rom.gb
```

Pass `--dmg` to force DMG mode, `--cgb` to force CGB mode, or `--serial` to run in serial test mode. Add `--headless` to run without a window or audio output. When headless you can control execution with:

* `--frames <n>` – run for the given number of frames.
* `--seconds <n>` – run for about `<n>` seconds.
* `--cycles <n>` – stop after `<n>` CPU cycles.

If no limit is specified the emulator runs until interrupted.

Test ROMs used for development are located in the `roms/` directory.

## Controls

The default keyboard mapping is:

- **Arrow Keys**: D-pad
- **S**: A button
- **A**: B button
- **Shift**: Select
- **Enter**: Start

## Testing

Unit tests can be executed with:

```bash
cargo test
```

## Project roadmap

A detailed checklist of planned tasks can be found in `TODO.md`.
