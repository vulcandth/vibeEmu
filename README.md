# vibeEmu

<img src="gfx/vibeEmu_512px.png" alt="vibeEmu Logo" width="250" />

> **AI is a tool, not a crutch.**  
>
> *vibeEmu* is a personal project to see how far I could get making an emulator
> using **vibe coding** — describing what you want in natural language and
> letting an asynchronous AI agent do most of the heavy lifting. I chose to do
> this out of fun, to get some exposure to Rust, and to better understand what
> AI is good and bad at, as well as how to interact with it effectively. I also
> wanted an emulator of my own that I could use for other projects.
>
> This project is **not** a statement! I am not endorsing the use of AI, and I
> understand many people have differing opinions about it. vibeEmu passes many
> test ROMs, but I will never claim that it is truly "more accurate" or that it
> contains as much love as the emulators created by the individuals who poured
> their hearts into their work, such as the developers of SameBoy, mGBA, BGB,
> and many many more. AI agents have a tendency to brute-force their way into
> passing tests, whereas the aforementioned emulators are written with deeper
> understanding of the underlying hardware thanks to the effort and expertise
> from their developers.
>
> Although I initially limited myself to vibe coding only, I am less strict about 
> limiting myself to it now. I encourage others to open PRs and contribute if 
> they'd like. You do **_not_** have to vibe code to contribute! You may use 
> vibeEmu to play; I just hope you understand the project for what it is, just 
> something fun I did with my spare time.
>
> One final note: the AI did derive some bits of code from other emulators, and
> I have attributed where I felt it was necessary. If you notice any code that
> appears to be derived from elsewhere, please kindly bring it to my attention,
> and I will absolutely provide proper attribution without hesitation.
>
> — Vulcandth

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

1. **Use a pre-installed library**: If you have `libmobile` already compiled and installed on your system (e.g., via a package manager or custom build), you can link against it instead of the bundled version. The library must be findable via standard system library paths or by setting the `LIBMOBILE_LIB_DIR` environment variable to point to the directory containing the compiled library file (e.g., `libmobile.so` on Linux, `mobile.lib` on Windows).
   ```bash
   cargo build -p vibe-emu-ui --no-default-features --features mobile-system
   ```

2. **Replace the vendored copy**: You may replace the contents of `vendor/libmobile-0.2.2/` with your modified or updated version and rebuild. The build system will automatically compile and link your replacement.
   
   Alternatively, to keep multiple versions side-by-side:
   - Rename `vendor/libmobile-0.2.2/` to something else (e.g., `vendor/libmobile-0.2.2-original/`)
   - Place your modified version in `vendor/libmobile-0.2.2/`
   - Or place it in `vendor/libmobile/` and set the `LIBMOBILE_SRC_DIR` environment variable:
     ```bash
     export LIBMOBILE_SRC_DIR=/path/to/your/libmobile
     cargo build -p vibe-emu-ui
     ```

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

## Logging

In `--release` builds, vibeEmu defaults to **no console log output**. To enable
logging, pass `--log-level`:

```bash
cargo run -p vibe-emu-ui --release -- --log-level info path/to/rom.gb
```

Supported levels:

- `off`: no logs (default in release builds)
- `error`: fatal errors only
- `warn`: non-fatal problems (e.g. audio disabled)
- `info`: high-level lifecycle (ROM load, reset, DMG/CGB mode)
- `debug`: emulator/UI diagnostics (serial dumps, CPU state snapshots)
- `trace`: very verbose tracing (PPU/APU traces, DMA, LCDC and OAM bug tracing)

Log targets used by the codebase:

- `vibe_emu_ui::serial`: formatted serial output
- `vibe_emu_ui::cpu`: periodic CPU state snapshots
- `vibe_emu_core::cartridge`: ROM/load/save/RTC messages
- `vibe_emu_core::ppu`, `vibe_emu_core::apu`: subsystem traces
- `vibe_emu_core::dma`, `vibe_emu_core::lcdc`, `vibe_emu_core::oambug`: deep
   timing/diagnostic traces

Advanced filtering is still available via `RUST_LOG` (env_logger syntax). For
example:

```bash
RUST_LOG=vibe_emu_ui=debug,vibe_emu_core=trace cargo run -p vibe-emu-ui -- --log-level trace path/to/rom.gb
```

Some traces are additionally gated by environment variables (for example
`VIBEEMU_TRACE_OAMBUG` and `VIBEEMU_TRACE_LCDC`). These enable generating the
trace events, but you still need `--log-level trace` (or an equivalent
`RUST_LOG` filter) to actually see them.

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
- **P**: Pause/unpause emulation
- **Escape**: Quit the emulator

Use the **top menu bar** to load ROMs, change settings, or open debugging tools.

## Testing

Unit tests for the emulation core can be executed with:

```bash
cargo test -p vibe-emu-core
```

If you are iterating on the frontend, run its tests with:

```bash
cargo test -p vibe-emu-ui
```
