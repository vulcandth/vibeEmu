# vibeEmu

<img src="gfx/vibeEmu_512px.png" alt="vibeEmu Logo" width="250" />

> **AI is a tool, not a crutch.**  
> *vibeEmu* is a personal research experiment in **vibe coding** – describing what you want in natural language and letting an asynchronous AI agent do most of the heavy lifting. The goal is to measure how far large‑language‑model assistance can take a programmer when building a cycle‑accurate Game Boy ⁄ Game Boy Color emulator in Rust, and where human expertise is still essential.

This repository intentionally exposes both successes *and* failures so others can judge the approach. It is **not** production‑ready software and **not** an endorsement of replacing human engineers with AI.

vibeEmu is a Game Boy and Game Boy Color emulator written in Rust. It aims to
feature a cycle‑accurate CPU, MMU, PPU and APU along with a `winit` + `pixels`
frontend. An ImGui powered debug UI exposes a register viewer and a VRAM viewer,
making the emulator useful both for playing games and for studying how the
hardware works. The repository is organised as a Cargo workspace with multiple
crates:

- `vibe-emu-core` contains the platform-agnostic emulation library.
- `vibe-emu-ui` provides the desktop frontend built on the core crate.
- `vibe-emu-mobile` provides Mobile Adapter GB integration (libmobile wrapper).
- `vibe-emu-mobile-sys` builds/links libmobile and exposes minimal FFI.

## Building

Ensure you have a recent Rust toolchain installed, then build the entire workspace:

```bash
cargo build
```

For better performance when playing games, use a release build:

```bash
cargo build --release
```

### Platform Requirements

- **Windows**: Visual Studio Build Tools with C++ support
- **Linux**: X11, GTK3, and ALSA development packages
- **macOS**: Xcode Command Line Tools

For detailed platform-specific instructions, troubleshooting, and build configuration options, see [BUILD.md](BUILD.md).

### Mobile Adapter GB support (bundled by default)

The UI builds with **Mobile Adapter GB** support enabled by default using the
vendored `vendor/libmobile-0.2.2` sources. This requires a working C toolchain
on your platform (e.g. MSVC Build Tools on Windows, or clang/gcc on Linux/macOS).

**License Notice**: The bundled `libmobile` library is licensed under the **GNU Lesser General Public License (LGPL) v3**. See `vendor/libmobile-0.2.2/COPYING.LESSER` for the full license text. The LGPL permits linking this library into your application. If you wish to use a modified or updated version of `libmobile`, you have two options:

1. **Use the system-provided library**: Build vibeEmu with the `mobile-system` feature to link against your own `libmobile` installation instead of the bundled version:
   ```bash
   cargo build -p vibe-emu-ui --no-default-features --features mobile-system
   ```

2. **Replace the vendored copy**: You may replace the contents of `vendor/libmobile-0.2.2/` with your modified or updated version and rebuild. The build system will automatically compile and link your replacement.

If you want to build the UI without Mobile Adapter GB support entirely:

```bash
cargo build -p vibe-emu-ui --no-default-features
```

## Running

The emulator expects the path to a ROM file. The command below will start the emulator in CGB mode by default:

```bash
cargo run -p vibe-emu-ui -- path/to/rom.gb
```

Pass `--dmg` to force DMG mode, `--cgb` to force CGB mode, or `--serial` to run in serial test mode. Add `--headless` to run without a window or audio output. When headless you can control execution with:

* `--frames <n>` – run for the given number of frames.
* `--seconds <n>` – run for about `<n>` seconds.
* `--cycles <n>` – stop after `<n>` CPU cycles.

If no limit is specified the emulator runs until interrupted.

### Mobile Adapter GB

The desktop UI includes Mobile Adapter GB support (libmobile). You can select
the active serial peripheral at runtime via the UI (see **Debugging UI**).

To start the emulator with the Mobile Adapter selected immediately:

```bash
cargo run -p vibe-emu-ui -- --mobile path/to/rom.gbc
```

To log libmobile debug messages and socket activity:

```bash
cargo run -p vibe-emu-ui -- --mobile --mobile-diag path/to/rom.gbc
```

Test ROMs used for development are located in the `roms/` directory.

## Debugging UI

Right‑click the main window to pause emulation and open a context menu.  From
here you can load another ROM, reset the Game Boy, choose the active **Serial
Peripheral**, or open the **Debugger** and **VRAM Viewer** windows. The debugger
shows CPU registers while the VRAM viewer lets you inspect background maps,
tiles, OAM and palettes. Hold **Space** to fast‑forward (4× speed) and press
**Escape** to quit.

## Controls

The default controls are:

- **Arrow Keys**: D-pad
- **S**: A button
- **A**: B button
- **Shift**: Select
- **Enter**: Start
- **Space**: Hold to fast-forward
- **Escape**: Quit the emulator

Use the **right mouse button** to pause/resume and bring up the context menu.

## Testing

Unit tests for the emulation core can be executed with:

```bash
cargo test -p vibe-emu-core
```

If you are iterating on the frontend, run its tests with:

```bash
cargo test -p vibe-emu-ui
```
