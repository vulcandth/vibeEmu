![GitHub License](https://img.shields.io/github/license/vulcandth/vibeEmu)

# vibe-emu-core

`vibe-emu-core` is the platform-agnostic Game Boy and Game Boy Color emulation
library at the heart of [vibeEmu](https://github.com/vulcandth/vibeEmu). It
aims for cycle-accurate emulation of the CPU, MMU, PPU, and APU.

## Features

- Cycle-accurate LR35902 CPU emulation
- Memory Management Unit (MMU) with full cartridge type support (MBC1, MBC2,
  MBC3 with RTC, MBC5)
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
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

// Load a ROM from bytes
let rom = std::fs::read("game.gb").unwrap();
let cart = Cartridge::load(rom);

let mut gb = GameBoy::new();
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
