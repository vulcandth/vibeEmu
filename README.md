# vibeEmu
![GitHub License](https://img.shields.io/github/license/vulcandth/vibeEmu)
![Codecov](https://img.shields.io/codecov/c/github/vulcandth/vibeEmu?label=core%20coverage)
![Crates.io Version](https://img.shields.io/crates/v/vibe-emu-core?label=vibe-emu-core)
![docs.rs](https://img.shields.io/docsrs/vibe-emu-core)

<img src="gfx/vibeEmu_512px.png" alt="vibeEmu Logo" width="250" />

> **AI is a tool, not a crutch.**  
>
> <!-- about-page-excerpt:start -->
> *vibeEmu* is a personal project to see how far I could get making an emulator
> using **vibe coding** — describing what you want in natural language and
> letting an asynchronous AI agent do most of the heavy lifting. I chose to do
> this out of fun, to get some exposure to Rust, and to better understand what
> AI is good and bad at, as well as how to interact with it effectively. I also
> wanted an emulator of my own that I could use for other projects.
> <!-- about-page-excerpt:end -->
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

vibeEmu is a Game Boy and Game Boy Color emulator written in Rust. It pairs a
platform-agnostic emulation core with a desktop frontend built on
`egui`/`eframe`. The desktop app is focused on playing games, while the
`vibe-emu-core` crate can also be reused as a library in other projects. The
repository is organized as a Cargo workspace with multiple crates:

- `vibe-emu-core` contains the platform-agnostic emulation library.
- `vibe-emu-ui` provides the desktop frontend built on the core crate.
- `vibe-emu-mobile` provides Mobile Adapter GB integration (libmobile wrapper).
- `vibe-emu-mobile-sys` builds/links libmobile and exposes minimal FFI.

## Desktop UI features

- Load a ROM from the command line or from the app menu.
- Configurable keyboard controls with optional gamepad input.
- Screenshot capture and display filter settings.
- Selectable serial peripherals, including link cable support and Mobile
  Adapter GB.
- A VRAM viewer for inspecting tiles, maps, sprites, and palettes.

## Building

Ensure you have a recent Rust toolchain installed, then build the entire workspace:

```bash
cargo build
```

For better performance when playing games, use a release build:

```bash
cargo build --release
```

### Runtime requirements

- **Windows** and **macOS**: no extra runtime setup beyond the platform system
  libraries used by the app.
- **Linux**: the desktop UI depends on the windowing, audio, and GTK/GLib
  libraries provided by your distribution.

### Build requirements

- **All platforms**: a recent stable Rust toolchain.
- **Windows**: Visual Studio Build Tools or Visual Studio with C++ support.
- **Linux**: a C toolchain plus the GTK3/GLib, X11/Wayland, ALSA, and
  `pkg-config` development packages listed in [BUILD.md](BUILD.md).
- **macOS**: Xcode Command Line Tools.

For detailed platform-specific instructions, troubleshooting, and build configuration options, see [BUILD.md](BUILD.md).

### Mobile Adapter GB support (bundled by default)

The desktop UI builds with **Mobile Adapter GB** support enabled by default
using the vendored `crates/vibe-emu-mobile/vendor/libmobile-0.2.2` sources.
This requires a working C toolchain on your platform.

**License Notice**: The bundled `libmobile` library is licensed under the
**GNU Lesser General Public License (LGPL) v3**. See
`crates/vibe-emu-mobile/vendor/libmobile-0.2.2/COPYING.LESSER` for the full
license text.

1. **Use a pre-installed library**: If you already have `libmobile` installed,
   build with the `mobile-system` feature instead of the bundled one. If the
   library is not in the default linker search path, set
   `LIBMOBILE_LIB_DIR=/path/to/libdir`.
   ```bash
   cargo build -p vibe-emu-ui --no-default-features --features mobile-system
   ```

2. **Replace the vendored copy**: Replace
   `crates/vibe-emu-mobile/vendor/libmobile-0.2.2/` with your modified source,
   or point `LIBMOBILE_SRC_DIR` at an alternate libmobile checkout:
   ```bash
   export LIBMOBILE_SRC_DIR=/path/to/your/libmobile
   cargo build -p vibe-emu-ui
   ```

If you want to build the UI without Mobile Adapter GB support entirely:

```bash
cargo build -p vibe-emu-ui --no-default-features
```

## Running

Run the desktop UI with the default debug profile:

```bash
cargo run
```

For better performance when playing games:

```bash
cargo run --release
```

You can optionally pass a ROM path after `--` to open it immediately:

```bash
cargo run -- path/to/rom.gb
```

If no ROM path is supplied, the UI starts paused and you can load one from the
menu.

Common arguments:

- `--dmg`: force DMG mode.
- `--dmg-neutral`: use neutral DMG palette settings.
- `--cgb`: force CGB mode.
- `--bootrom <path>`: load a boot ROM file.
- `--headless`: run without a window or audio output.
- `--frames <n>`: in headless mode, stop after `n` frames.
- `--seconds <n>`: in headless mode, stop after about `n` seconds.
- `--cycles <n>`: in headless mode, stop after `n` CPU cycles.
- `--mobile`: start with Mobile Adapter GB selected.
- `--mobile-config <path>`: load a Mobile Adapter configuration file.
- `--mobile-device <blue|yellow|green|red>`: choose the Mobile Adapter model.
- `--mobile-unmetered`: mark the Mobile Adapter connection as unmetered.
- `--mobile-dns1 <addr>` / `--mobile-dns2 <addr>`: override Mobile Adapter DNS
  servers.
- `--mobile-relay <addr>`: set the Mobile Adapter relay address.
- `--mobile-p2p-port <port>`: set the Mobile Adapter peer-to-peer port.
- `--mobile-diag`: enable Mobile Adapter diagnostic logging.
- `--keybinds <path>`: use a custom keybind configuration file.
- `--log-level <off|error|warn|info|debug|trace>`: override the default log
  verbosity.

Run `cargo run -- --help` for the full command-line reference.

## Logging

Debug builds default to `info` logging. Release builds default to `off`.
Override the default with `--log-level`:

```bash
cargo run -p vibe-emu-ui --release -- --log-level info path/to/rom.gb
```

`RUST_LOG` can still be used for per-module filtering. For example:

```bash
RUST_LOG=vibe_emu_ui=debug,vibe_emu_core=trace cargo run -p vibe-emu-ui -- --log-level trace path/to/rom.gb
```

Some trace streams are additionally gated by environment variables such as
`VIBEEMU_TRACE_OAMBUG` and `VIBEEMU_TRACE_LCDC`.

## Controls

The default keyboard controls are:

- **Arrow Keys**: D-pad
- **A**: A button
- **S**: B button
- **Tab**: Select
- **Enter**: Start
- **Space**: Hold to fast-forward
- **F12**: Capture screenshot
- **P**: Pause/unpause emulation
- **Escape**: Quit the emulator

Use the **top menu bar** to load ROMs, change settings, capture screenshots, or
open the VRAM Viewer and serial peripheral settings. Screenshot hotkeys are configurable in
**Options → Settings... → Keybinds**. Captures are saved to a `screenshots/`
folder next to the loaded ROM. Display filtering is configurable in
**Options → Settings... → Emulation**, including separate horizontal/vertical
sampling and optional scanline/LCD grid effects.

## Testing

Unit tests for the emulation core can be executed with:

```bash
cargo test -p vibe-emu-core
```

If you are iterating on the frontend, run its tests with:

```bash
cargo test -p vibe-emu-ui
```
