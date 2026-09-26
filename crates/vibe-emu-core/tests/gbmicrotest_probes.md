# GBMicrotest probe audit

None of the 31 ROMs formerly called "visual/interactive probes" requires user
input. Their assembly does not read the joypad or wait for a button. Most simply
use an older result channel or run a continuous visual/audio experiment. Absence
of the `$FF82` completion store is not evidence of interactivity.

Sources: [GBMicrotest at 463eb6b](https://github.com/aappleby/gbmicrotest/tree/463eb6bc0fe31d61781ef63060ad6d74090c0255/tests),
including `header.inc` and `macros.inc`. The ROMs are from the pinned c-sp v7.0
bundle. `gbmicrotest/probes.rs` automates the observations below on DMG revision C.

## Observations

Names omit `.gb`. "Result loop" means the ROM actually executes its endless
`LD [$8000],A` / `JR` display loop with the expected accumulator, not merely that
a byte happens to exist in memory. Screenshots compare every pixel of the sixth
completed frame, with the palette white / `$AA` / `$55` / black. Other continuous
probes are also observed after six frames. Faults, STOP and missed observation
points fail; reaching a timeout alone never passes a probe.

| ROM | Source behavior and automated check |
| --- | --- |
| `000-oam_lock` | OAM lock timing, result loop `$FF` (black at the assembled delay). |
| `000-write_to_x8000` | Repeated constant VRAM write, result loop `$55`; its loop also reloads A. |
| `001-vram_unlocked` | OAM ISR writes `$55` to VRAM `$8000`; check that write succeeded. |
| `002-vram_locked` | Samples STAT near VRAM lock, result loop `$84`. |
| `004-tima_boot_phase` | Timer phase assertion, result loop `$55`. |
| `004-tima_cycle_timer` | Timer assertion, result loop `$55`. |
| `007-lcd_on_stat` | Despite its name/comments, the active instruction reads **LY**, not STAT. Check A/VRAM `$00` and the ISR's final loop at `$004D`. |
| `400-dma` | Copies ROM `$0200..$029F` to OAM. Compare all 160 bytes against the loaded ROM. |
| `500-scx-timing` | Adds four TIMA reads after the OAM-to-HBlank wait; result loop `$49`. |
| `800-ppu-latch-scx` | Fixed SCX latch experiment; exact screenshot. |
| `801-ppu-latch-scy` | Fixed SCY latch experiment; exact screenshot. |
| `802-ppu-latch-tileselect` | LCDC tile-data selection changes; exact screenshot. |
| `803-ppu-latch-bgdisplay` | Disables/re-enables BG within each line; exact screenshot. |
| `audio_testbench` | Configures and triggers continuous channel-4 noise, then loops. Check DAC/channel enabled, length disabled, NR42/43/50/51 = `$F0/$5F/$77/$FF`, and both PCM amplitudes 0 and 15. This checks the programmed experiment, not an entire audio waveform. |
| `cpu_bus_1` | Constant HRAM write loop; `$FF80 = $55`. |
| `dma_basic` | VRAM-to-OAM DMA during rendering. Compare all OAM bytes, including PPU/VRAM bus contention. |
| `flood_vram` | Writes through VRAM during rendering; compare all 8192 bytes, including blocked writes. |
| `lcdon_write_timing` | OAM write after LCD restart is blocked; result loop `$00`. |
| `ly_while_lcd_off` | Samples LY with LCD off; result loop `$00`. |
| `minimal` | Same assembled timing experiment as `500-scx-timing`; result loop `$49`. |
| `mode2_stat_int_to_oam_unlock` | OAM unlock timing, result loop `$FF` at the assembled delay. |
| `oam_sprite_trashing` | Repeated OAM/LCD experiment; exact screenshot. |
| `poweron` | Reads NR10 and fills the first 256 VRAM bytes with the result; all `$80`. |
| `ppu_scx_vs_bgp` | Sweeps SCX and clears it early in mode 3; exact screenshot. |
| `ppu_sprite_testbench` | Fixed sprite setup with offscreen sprites and white BG; exact screenshot. |
| `ppu_spritex_vs_scx` | **Self-checking** sprite/scroll sweep with distinct `$55` success and `$FF` failure VRAM loops. Check `$55`. |
| `ppu_win_vs_wx` | Sweeps WX while toggling window enable; exact screenshot. |
| `ppu_wx_early` | Fixed early-WX experiment; exact screenshot. |
| `toggle_lcdc` | Finite LCD toggle sequence, then loop at `$01A0`; check LCDC/LY/mode all zero. |
| `wave_write_to_0xC003` | Despite the name, active code repeatedly writes `$55` to WRAM `$C003`; check that byte. |
| `temp` | **Unfinished**, not interactive: one NOP followed by empty ROM. It has no assertion, useful output, or success condition and eventually faults in both emulators. Remains ignored. |

The `$49` timing result is the sum produced by the four active TIMA loads in
`500-scx-timing`/`minimal`, also reproduced independently below. The source's
"DMG overhead 65" note describes exploratory measurements and is not an assertion
in the assembled program.

## Reference provenance and limits

The nine PNGs in `reference_screenshots/gbmicrotest/`, the 160-byte OAM snapshot
`reference_data/gbmicrotest/dma_basic.bin`, and the 8192-byte VRAM snapshot
`reference_data/gbmicrotest/flood_vram.bin` were generated independently with
[SameBoy 1.0.2](https://github.com/LIJI32/SameBoy/tree/v1.0.2), using its DMG-B model,
a real DMG boot ROM, the GREY palette and six normal VBlank callbacks after boot.
OAM is zeroed once at boot handoff to match vibeEmu's deterministic initial RAM;
these probes do not all initialize OAM themselves. There is no input injection.
The harness runs vibeEmu's normal skipped-boot DMG-C profile.

These are **emulator reference outputs, not hardware captures**. Their expected
patterns were checked against the ROM assembly, and the memory/result probes
were also run independently. They provide reproducible regression coverage;
a hardware capture can further validate revision-sensitive pixels. They must
not be regenerated from vibeEmu output just to accept a mismatch.

`gbmicrotest/reference.c` is the independent capture runner. Compile it with the
SameBoy 1.0.2 Core sources (excluding standalone test programs), `-std=gnu11`,
`-DGB_INTERNAL`, `-D_GNU_SOURCE`, `-DGB_VERSION=\"1.0.2\"`, and the
`GB_DISABLE_TIMEKEEPING`, `GB_DISABLE_DEBUGGER`, `GB_DISABLE_CHEATS`,
`GB_DISABLE_CHEAT_SEARCH`, `GB_DISABLE_REWIND` defines. Add the SameBoy root as an
include path and link `-lm`. Run:

```text
reference <rom.gb> <dmg_boot.bin> <output-prefix>
```

It writes `.gray` (160x144 row-major 8-bit grayscale), `.vram` and `.oam` files.
The PNGs encode the `.gray` bytes without scaling or filtering. Retain only OAM
for `dma_basic` and VRAM for `flood_vram`.

## Remaining suspected test defects

`halt_op_dupe_delay` expects DIV `$55` after roughly 256 dots; both emulators
produce `$01`. `stat_write_glitch_l154_d` expects IF `$E0`, but never clears the
VBlank request after a full frame; both produce `$E1`. These two remain ignored
pending hardware/source reconciliation, in addition to unfinished `temp`.

[GBMicrotest PR #2](https://github.com/aappleby/gbmicrotest/pull/2) correctly fixes
the assembled delays in `line_153_lyc_a/b/c`. The cached and freshly downloaded
c-sp v7.0 ROMs already match its corrected binaries byte for byte. The existing
version guard verifies the corrected delay sequence before running these tests.
