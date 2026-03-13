![GitHub License](https://img.shields.io/github/license/vulcandth/vibeEmu)

# vibe-emu-core

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

`vibe-emu-core` is the platform-agnostic Game Boy and Game Boy Color emulation
library at the heart of [vibeEmu](https://github.com/vulcandth/vibeEmu). It
aims for cycle-accurate emulation of the CPU, MMU, PPU, and APU.

## Features

- Cycle-accurate LR35902 CPU emulation
- Memory Management Unit (MMU) with cartridge support for ROM-only, MBC1,
  MBC2, MBC3 (with RTC), MBC30, MBC5, and TPP1
- Pixel Processing Unit (PPU) — DMG and CGB rendering modes
- Audio Processing Unit (APU) with all four channels
- Game Boy Color (CGB) support including double-speed mode and CGB palettes
- Serial link port emulation
- Timer and interrupt handling
- Optional tracing features behind feature flags (`ppu-trace`, `apu-trace`,
  `cpu-trace`)

## Usage

Add `vibe-emu-core` to your `Cargo.toml`:

```toml
[dependencies]
vibe-emu-core = "0.0.1"
```

The main entry point is `GameBoy`, which holds the `cpu` and `mmu` fields.
Step the CPU to advance emulation and read the framebuffer from `mmu.ppu`:

```rust
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

// Load a ROM from bytes
let rom = std::fs::read("game.gb").unwrap();
let cart = Cartridge::from_bytes(rom);

let mut gb = GameBoy::new(Model::default());
gb.mmu.load_cart(cart);

// Run one frame
gb.mmu.ppu.clear_frame_flag();
while !gb.mmu.ppu.frame_ready() {
    gb.cpu.step(&mut gb.mmu);
}

// Access the 160×144 ARGB framebuffer
let pixels = gb.mmu.ppu.framebuffer();
```

For a full integration example, see the
[vibe-emu-ui](https://github.com/vulcandth/vibeEmu/tree/main/crates/vibe-emu-ui)
crate in the workspace.

## Third-party licenses

See [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) for the licenses of
all third-party code included in this crate.

## License

This crate is licensed under the [MIT License](LICENSE).
