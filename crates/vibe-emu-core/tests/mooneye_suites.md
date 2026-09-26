# Mooneye and Wilbertpol ROM coverage

Both suites come from the existing `game-boy-test-roms` v7.0 archive. The shared
runner follows the `game-boy-test-roms-howto.md` bundled in each directory.
The archive and extracted ROMs remain cached under `test_roms/`.

`mooneye_acceptance` already covers the acceptance directory, seven
`emulator-only` ROMs, and two `misc` boot-DIV ROMs. `mooneye_extended` enumerates
the remaining ROMs without duplicating those tests: 27 pass and five are
ignored. `wilbertpol` independently enumerates its entire bundle: 103 pass and
19 are ignored (nine timing failures, six unsupported hardware cases, and four
diagnostic workloads). Each sprite-priority ROM runs twice, for DMG and CGB.

## Completion and hardware selection

- Modern Mooneye signals completion with `LD B,B` and Fibonacci values
  `3, 5, 8, 13, 21, 34` in B/C/D/E/H/L. Six `0x42` values signal failure.
- Wilbertpol uses undefined opcode `0xED` as the breakpoint, with the same
  success registers. The runner intercepts the opcode before CPU execution.
- Both runners allow 120 emulated seconds. A timeout, CPU fault, or unexpected
  STOP is a failure, never a successful completion.
- Filename hardware restrictions take precedence over the cartridge header.
  Supported profiles use DMG C and CGB E, with CGB 0 selected where requested.
  MGB, SGB, SGB2, and AGB requirements are explicitly ignored; running one of
  these cases with `--ignored` reports the unsupported model.
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
one failing test without running unsupported diagnostics:

```text
cargo test -p vibe-emu-core --test wilbertpol -- --include-ignored ly_lyc_write-C
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

The remaining nine Wilbertpol timing failures are real discrepancies: all 13
initial PPU timing failures were also run successfully with SameBoy 1.0.2 on
the corresponding DMG/CGB profiles. Their specific coincidence, interrupt,
and SCX timing mismatches are recorded in `wilbertpol_ignored.txt`.

Source inspection used
[Mooneye at 31510e1](https://github.com/Gekkio/mooneye-test-suite/tree/31510e12eea6286d36eea060a6adde755e1067aa)
and
[Wilbertpol at b78dd21](https://github.com/wilbertpol/mooneye-gb/tree/b78dd21f0b6d00513bdeab20f7950e897a0379b3),
including their shared completion macros, boot/I/O assertions, sprite-priority
programs, and GPU timing probes. These source revisions are investigation
references; the executed binaries and expected images remain those supplied
by the pinned v7.0 archive.
