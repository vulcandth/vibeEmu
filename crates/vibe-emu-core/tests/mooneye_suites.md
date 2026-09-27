# Mooneye and Wilbertpol ROM coverage

Both suites come from the existing `game-boy-test-roms` v7.0 archive. The shared
runner follows the `game-boy-test-roms-howto.md` bundled in each directory.
The archive and extracted ROMs remain cached under `test_roms/`.

`mooneye_acceptance` already covers the acceptance directory, seven
`emulator-only` ROMs, and two `misc` boot-DIV ROMs. `mooneye_extended` enumerates
the remaining ROMs without duplicating those tests. `wilbertpol` independently
enumerates its entire bundle. Each sprite-priority ROM runs twice, for DMG
and CGB. Only non-asserting utilities and logic-analysis workloads are ignored.

## Completion and hardware selection

- Modern Mooneye signals completion with `LD B,B` and Fibonacci values
  `3, 5, 8, 13, 21, 34` in B/C/D/E/H/L. Six `0x42` values signal failure.
- Wilbertpol uses undefined opcode `0xED` as the breakpoint, with the same
  success registers. The runner intercepts the opcode before CPU execution.
- Both runners allow 120 emulated seconds. A timeout, CPU fault, or unexpected
  STOP is a failure, never a successful completion.
- Filename hardware restrictions take precedence over the cartridge header.
  Supported profiles use DMG C and CGB E, with CGB 0 selected where requested.
  MGB, SGB, SGB2, and AGB restrictions select their respective models.
  Acceptance tests also run both SGB versions and both AGB boot revisions.
- Boot-state ROMs execute the real boot ROM from power-on. This matters for
  CGB compatibility-mode registers and palettes at handoff.
- `manual-only/sprite_priority.gb` does not require user interaction. Its two
  supplied PNGs are compared pixel-for-pixel using the bundled palette rules.
  The older ROM breaks immediately after enabling the LCD, so the runner
  freezes the CPU at completion and lets the PPU reach three VBlanks, ensuring
  two complete frames have rendered even if the first frame is partial.
- Dump utilities and logic-analyzer workloads have no pass/fail assertions or
  supplied image expectations. They remain listed and ignored rather than
  being counted as passing tests.

Ignore lists contain reasons and are checked for stale entries. To investigate
a specific timing test without running unsupported diagnostics:

```text
cargo test -p vibe-emu-core --test wilbertpol -- --exact acceptance/gpu/ly_lyc_write-C.gb
```

## Initial failures and fixes

The initial new-Mooneye run had three assertion failures. Wilbertpol had 16
assertion failures and two premature screenshot captures. The recovered cases
are enabled in the normal test run:

- CGB compatibility-mode I/O: SC masks and clock selection, native-only
  registers, VBK's unused bits, and palette index/data access.
- Boot-state tests: actual boot execution, KEY0 locking at handoff, and palette
  uploads still permitted while the boot ROM is mapped.
- VBlank LY reads: apply the CPU-visible advance at every VBlank scanline
  boundary, not only the transition from line 152 to 153. This recovers
  Wilbertpol's `ly143_144_145`, `vblank_if_timing`, `ly_lyc_0-GS`, and
  `ly_lyc_153-GS` tests.
- Sprite-priority screenshots: wait for complete frames as described above.

All nine remaining Wilbertpol timing failures were subsequently fixed and
removed from the ignore list:

- CGB coincidence changes at the physical line boundary, rather than four
  dots early. Writes cannot recompute the match while the comparator is
  holding its previous result during the counter transition.
- CGB LYC writes settle at the end of their CPU write cycle. An intervening
  line transition can still match the previous LYC value; a write in the
  comparator's closed interval must not create a spurious interrupt.
- On physical line 153, CGB starts comparing zero at dot 8 of the PPU clock.
  DMG also blanks coincidence during the last four dots of ordinary VBlank
  scanlines, as it does during visible-line transitions.
- CGB samples pending interrupts at the start of a HALT idle cycle; DMG
  samples halfway through. Sampling CGB at the end dispatched interrupts
  one cycle too early with IME enabled. The corrected phase also preserves
  Daid's IME-disabled wake timing without a separate extra-cycle adjustment.

The full regression run also exposed a previously compensating renderer
error in `cgb-acid-hell`: its left-edge sprite fetch adds six dots to the
normal four-dot fetcher startup, rather than replacing those startup dots.
Correcting this additive delay restores the screenshot with the corrected
LYC timing. The
[cleaned-up source](https://github.com/CelestialAmber/cgb-acid-hell/blob/main/macros/scanline_hell.asm)
describes the sprite stall and the bitplane write that depends on it.

All 13 initial PPU timing failures were independently run successfully with
SameBoy 1.0.2 on the corresponding DMG/CGB profiles. Its `Core/display.c`,
`Core/memory.c`, and `Core/sm83_cpu.c` also informed the investigation of
comparator write conflicts and HALT sampling. The tests retain the supplied
ROM assertions and image expectations; no supported assertion failures
remain ignored in these two new runners.

Source inspection used
[Mooneye at 31510e1](https://github.com/Gekkio/mooneye-test-suite/tree/31510e12eea6286d36eea060a6adde755e1067aa)
and
[Wilbertpol at b78dd21](https://github.com/wilbertpol/mooneye-gb/tree/b78dd21f0b6d00513bdeab20f7950e897a0379b3),
including their shared completion macros, boot/I/O assertions, sprite-priority
programs, and GPU timing probes. These source revisions are investigation
references; the executed binaries and expected images remain those supplied
by the pinned v7.0 archive.

## Added hardware model coverage

The model tests fetch matching boot images from the
[GBDev boot ROM archive](https://gbdev.gg8.se/files/roms/bootroms/), keeping them
in the existing local cache. MGB/SGB/SGB2 register and I/O tests, both SGB DIV
variants, AGB registers/DIV, and MGB serial alignment run normally. The new
models also run the hardware-independent acceptance ROMs (including sprite
interrupt timing), and startup checks exercise both real and skipped boot.

`model_boot.rs` varies SGB headers across several packets and compares divider,
LCD phase, and packet RAM against actual boot execution. The skipped-boot
calculation follows the packet-building, bit transmission, and four-frame
wait loops in [the boot disassembly](https://codeberg.org/ISSOtm/gb-bootroms).
It does not choose a divider based on a ROM name or global checksum alone.

Both `madness/mgb_oam_dma_halt_sprites.gb` cases now run and compare the supplied
screenshot after HALT. They use the reference's grayscale palette. MGB models
the stalled DMA word's measured OAM bus corruption; the source describes
unit-dependent behavior, so this is the tested MGB profile rather than a claim
that every Pocket exhibits the same corruption. The ROM remains halted;
success is determined by pixels, not by treating HALT as a pass.

AGB uses late CGB timing with separate compatibility sprite timing and active
wave RAM access restrictions. Its approximate initial divider phase is
calibrated against `misc/boot_div-A` using both AGB boot images. The AGB wave
RAM rule and initial registers follow
[Pan Docs](https://gbdev.io/pandocs/Audio_Registers.html) and its
[power-up documentation](https://gbdev.io/pandocs/Power_Up_Sequence.html).
