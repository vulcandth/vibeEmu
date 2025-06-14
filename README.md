# vibeEmu

vibeEmu is a Game Boy and Game Boy Color emulator written in Rust. The current implementation contains the basic building blocks such as the CPU core and a simplified memory system. It is meant as a learning project and starting point for a more complete emulator.

## Building

Ensure you have a recent Rust toolchain installed. To build the project run:

```bash
cargo build
```

The frontend uses the `minifb` crate for window creation. On Linux you may need
X11 development packages installed (e.g. `libx11-dev`).

## Running

The emulator expects the path to a ROM file. The command below will start the emulator in CGB mode by default:

```bash
cargo run -- path/to/rom.gb
```

Pass `--dmg` to force DMG mode or `--serial` to run in serial test mode.

Test ROMs used for development are located in the `roms/` directory.

## Testing

Unit tests can be executed with:

```bash
cargo test
```

## Project roadmap

A detailed checklist of planned tasks can be found in `TODO.md`.
