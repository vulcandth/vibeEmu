# vibeEmu

<img src="gfx/vibeEmu_512px.png" alt="vibeEmu Logo" width="250" />

> **AI is a tool, not a crutch.**  
> *vibeEmu* is a personal research experiment in **vibe coding** – describing what you want in natural language and letting an asynchronous AI agent do most of the heavy lifting. The goal is to measure how far large‑language‑model assistance can take a programmer when building a cycle‑accurate Game Boy ⁄ Game Boy Color emulator in Rust, and where human expertise is still essential.

This repository intentionally exposes both successes *and* failures so others can judge the approach. It is **not** production‑ready software and **not** an endorsement of replacing human engineers with AI.

vibeEmu is a Game Boy and Game Boy Color emulator written in Rust.  It aims to
feature a cycle‑accurate CPU, MMU, PPU and APU along with a `winit` + `pixels`
frontend.  An ImGui powered debug UI will expose a register viewer and a VRAM
viewer, making the emulator useful both for playing games and for studying how
the hardware works. The repository is organised as a Cargo workspace with two
crates:

- `vibe-emu-core` contains the platform-agnostic emulation library.
- `vibe-emu-ui` provides the desktop frontend built on the core crate.

## Building

Ensure you have a recent Rust toolchain installed. You can compile the entire
workspace from the repository root:

```bash
cargo build
```

Each crate can also be built individually. The UI depends on the core library,
so building the frontend will build the core automatically:

```bash
# Core library only
cargo build -p vibe-emu-core

# UI frontend (pulls in the core crate as a dependency)
cargo build -p vibe-emu-ui
```

The frontend uses `winit` with the `pixels` crate for window creation and
rendering via `wgpu`. On Linux you may need X11 development packages installed
(e.g. `libx11-dev`) and GTK development headers (`libgtk-3-dev`). Audio output
relies on `cpal`, which requires ALSA headers. Install `libasound2-dev` as well
if you build on Linux.

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

Test ROMs used for development are located in the `roms/` directory.

## Debugging UI

Right‑click the main window to pause emulation and open a context menu.  From
here you can load another ROM, reset the Game Boy or open the **Debugger** and
**VRAM Viewer** windows.  The debugger shows CPU registers while the VRAM viewer
lets you inspect background maps, tiles, OAM and palettes.  Hold **Space** to
fast‑forward (4× speed) and press **Escape** to quit.

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

### Manual volume control

The Game Boy's "zombie" mode allows adjusting the envelope while a channel is
playing. Different models behave inconsistently, but using increase mode with a
period of zero works reliably. Write `$V8` to `NRx2` to set the initial volume
before triggering the channel, then repeatedly write `$08` to increment the
volume by one. Performing this fifteen times effectively decreases the volume by
one.

## Testing

Unit tests for the emulation core can be executed with:

```bash
cargo test -p vibe-emu-core
```

If you are iterating on the frontend, run its tests with:

```bash
cargo test -p vibe-emu-ui
```

### Game Boy Compatibility Tests

The gambatte (Game Boy compatibility suite) tests are **not** included in the default `cargo test` runs to keep feedback cycles fast. To run the full compatibility suite explicitly:

```bash
cargo gambatte_test
```

This will run the comprehensive gambatte test suite which validates emulation accuracy against reference implementations.

### Audio Snapshots

Use the wave-channel helper to capture deterministic audio for regression checks:

```bash
cargo run -p vibe-emu-core --example dump_wav -- <rom> <out.wav> --seconds=3 --cgb
```
