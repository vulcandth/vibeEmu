# Gambatte compatibility review

Base: `e50ee9707bd5e216d0580957f63af3e3981506c8` (`main`). This work is independent of save-state PR #437.

## Measurement and scope

The v7.0 c-sp ROM bundle contains 3,429 runnable Gambatte cases. Runs below include the existing ignored cases. A case may have DMG and CGB expectations; the runner stops that case at its first failing model.

| Configuration | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Original runner and original core | 2,192 | 1,237 | 0 |
| Corrected runner, original core | 2,256 | 1,173 | 0 |
| Corrected runner, changed core | 2,271 | 1,158 | 0 |

The core changes fix 15 cases with no regression against the corrected-runner baseline. Relative to the original measurement, 81 failures pass and two previously passing cases fail because the runner now selects CGB-C rather than CGB-E. These two are listed below; they are not hidden or added to an ignore list. Six false audio passes exposed by the runner correction were investigated and fixed in the core. Forty-nine newly passing cases are removed from the existing ignore list; no ignores are added.

Every original failure has matching assembly in [the upstream test tree](https://github.com/pokemon-speedrunning/gambatte-core/tree/d819bad196/test/hwtests). The tree is unchanged at source checkout `5a41a68c25402421fb1983ddadc9faf2418ddb0f`. The accompanying CSV records each case's expectations, source link, normalized source hash, explicit I/O accesses, fixed-address timing sections, and before/after outcome. This is a complete static source inventory, **not an exhaustive causal diagnosis** of the remaining failures. Detailed source analysis and implementation in this pass focus on the runner, pulse retriggering, and VRAM DMA.

## Runner corrections

The [upstream runner](https://github.com/pokemon-speedrunning/gambatte-core/blob/d819bad196/test/testrunner.cpp) is the expectation contract:

- A DMG-only `dmg08_out` expectation does not imply a CGB expectation. Explicit split and shared markers remain supported. PNG expectations are independent.
- Attach audio output before emulation, drain it once per frame, and inspect both channels from the final frame. The old runner enabled output after execution and therefore compared an empty buffer. Empty capture is now an error, not silence.
- Convert the core's expanded RGB555 output to the upstream PNG color space. Fixtures, pixel tolerances, and hexadecimal output patterns are unchanged.
- Select CGB-C for the `cgb04c` hardware expectations rather than the frontend's CGB-E default. The ROM bundle's how-to identifies this target; the [hardware database](https://gbhwdb.gekkio.fi/consoles/cgb/) also documents CGB-CPU-04 with CPU CGB C. Testing CGB-E with working audio exposes different pulse-retrigger behavior, confirming that the distinction matters. Core model defaults and real revision distinctions remain intact.
- Report the observed hexadecimal tiles when an output mismatches, to make subsequent source-level timing investigations reproducible.

## Shared pulse-channel behavior

`trigger_square` already serves both channels and all models. Its unconditional suppression clearing and pending-duty application made repeated triggers act like waveform edges. The fix preserves the initial suppressed sample and latched duty until an actual period edge. Existing CGB D/E phase quirks remain localized in the same function.

The six affected assembly cases repeatedly trigger before a new waveform edge. In the duty-zero-to-duty-three pair, the duty write occurs at a carefully selected phase; making it immediately effective on retrigger produces modulation that hardware does not produce. In the phase-zero tests, retriggering must not expose the initially suppressed waveform sample.

Source cases:

- `sound/ch1_duty0_to_duty3_pos3_dmg08_cgb04c_outaudio0`
- `sound/ch1_duty0_to_duty3_pos3_2_dmg08_cgb04c_outaudio0`
- `sound/ch1_duty1_pattern_pos0_dmg08_cgb04c_outaudio0`
- `sound/ch1_duty2_pattern_pos0_dmg08_cgb04c_outaudio0`
- `sound/ch2_init_pos_1_dmg08_cgb04c_outaudio0`
- `sound/ch2_init_pos_2_dmg08_cgb04c_outaudio0`

The adjacent duty-switch `_0` case requires audio and continues to pass. The new integration tests exercise both channels and multiple models, checking suppression retention, eventual output, and duty latching. SameBoy's [pulse trigger implementation](https://github.com/LIJI32/SameBoy/blob/213a12ce93d66b105a113debd9396306066a7cfc/Core/apu.c) independently retains suppression across ordinary active retriggers; the duty-latch change here is grounded in the Gambatte assembly cases above.

## One VRAM DMA block copier

GDMA and HDMA previously duplicated their copy loops and used the OAM DMA reader. They now share the source-bus selection and full address-counter progression. OAM DMA keeps its separate hardware bus behavior; normal/double-speed stall costs remain shared and unchanged for complete blocks.

- `dma/ff51_bits`, `ff52_bits`, `ff53_bits`, and `ff54_bits` write DMA addresses then read back `FF`. These ports are write-only, also documented in [Pan Docs](https://github.com/gbdev/pandocs/blob/master/src/CGB_Registers.md). Internal programmed addresses still drive transfers.
- `dma/dma_hiram_read`, `dma_oam_read`, and `dma_vram_read` compare initialized source bytes with VRAM and expect the comparison to fail immediately. `dma_hiram_read_result` further expects `FF - FE = 1`. VRAM DMA must not copy from these disconnected source buses as if it were CPU/OAM DMA access. The implementation returns `FF` for these disconnected sources; it does not attempt a new electrical bus-retention model.
- `dma/dma_dst_wrap_1` and `_2` differ in destination high byte `DF` versus `FF`. Both address VRAM `9FF0`; the first continues into `8000`, while the second stops at full 16-bit counter overflow. Masking the counter itself discarded this distinction. The copier now masks only the VRAM write address. The source-wrap counterpart continues to pass. This agrees with the full-counter overflow termination in SameBoy's [DMA implementation](https://github.com/LIJI32/SameBoy/blob/213a12ce93d66b105a113debd9396306066a7cfc/Core/memory.c); a blanket stop at the 8-KiB VRAM boundary would fail the paired case.

New tests verify write-only readback without losing programmed addresses, disconnected-source handling in both transfer modes, and both overflow outcomes. No new production path duplicates DMG/CGB behavior.

## Remaining evidence and limits

The largest remaining families concern OAM scan/DMA interactions, sprite-fetch stalls, window activation/restart, scroll/register fetch latches, STAT/LYC transitions, and speed-switch phases. The CSV retains exact case names and source locations. No global timing offset, ROM-name condition, fixture edit, or added ignore entry is used to force them to pass.

Two cases exposed by selecting CGB-C remain unresolved:

- `lcd_offset/offset1_lyc99int_m2stat_count_1_cgb04c_out91`: performs two speed switches, synchronizes on LYC=153, then samples STAT at fixed addresses `1065` and `10D6` and prints LY. Final output is `00`, expected `91`. The CGB-C/E end-of-VBlank STAT distinction is already modeled and independently exercised by AGE. Flattening that distinction merely to retain this pass is not justified.
- `oam_access/preread_ds_lcdoffset1_2_cgb04c_out3`: performs three speed switches, synchronizes on LYC=1, then reads OAM at fixed address `10D7`; the expected low bits are `3` from a blocked read, but final output is `0`. The existing early OAM-lock distinction between CGB-C and CGB-E is also covered by AGE. A coherent clock-phase investigation is still required.

Both cases were already in the original ignore list, but are included in every complete Gambatte measurement here. No hardware capture was performed. Analog audio, per-dot VRAM DMA contention, and all remaining timing families are not claimed solved.

## Validation

The status generator now identifies a test by `(suite, name)` rather than name alone. Mooneye and Wilbertpol share eight relative ROM names, so the previous generator silently lost seven passing results and one failure. Repeated nested output within one command still counts once. Two Python regression tests cover both situations. This reporting correction must be accounted for when comparing the old checked-in report with the new one.

`TEST_STATUS.md` is generated by the repository's actual `python scripts/update_test_status.py`, which includes ignored tests and reports their real failures.

| Local validation | Original base | Final code |
| --- | --- | --- |
| Full workspace debug | 1,531 passed, 0 failed, 41 ignored | 1,539 passed, 0 failed, 41 ignored |
| Full workspace release | 1,531 passed, 0 failed, 41 ignored | 1,539 passed, 0 failed, 41 ignored |
| Complete Gambatte release | 2,192 passed, 1,237 failed | 2,271 passed, 1,158 failed |
| Complete Gambatte debug | Not separately run | 2,271 passed, 1,158 failed |
| Formatting / workspace Clippy | Not separately measured | Pass / pass with warnings denied |
| Dependency policy (`cargo deny --locked check`) | Dependency graph unchanged | Pass |
| Python status-generator regressions | Tests did not exist | 2 passed |

Final debug/release Gambatte outcomes agree for all 3,429 cases. Full workspace runs have no removed cases and add eight Rust regression tests. Manual gameplay, hardware captures, and analog audio listening were not performed.

### Reproduction

Use the repository's c-sp v7.0 ROM bundle at `crates/vibe-emu-core/test_roms` and the same Rust toolchain (local runs used Rust/Cargo 1.98.1 on Windows). The original baseline is commit `e50ee9707bd5e216d0580957f63af3e3981506c8`. To reproduce the corrected-runner baseline independently, use the final test runner and expectation helper with `src/apu.rs` and `src/mmu.rs` from that base in a separate checkout.

```text
cargo test --locked --workspace --no-fail-fast
cargo test --locked --workspace --release --no-fail-fast
cargo gambatte_test -- --include-ignored
cargo test --locked -p vibe-emu-core --release --test gambatte -- --include-ignored
python scripts/update_test_status.py
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo deny --locked check
python -B -m unittest discover -s scripts -p "test_*.py"
```

The complete Gambatte commands deliberately return exit 101 for the documented remaining failures. The status generator itself returns success after writing the report; its `Combined exit code` records underlying Cargo failures and must also be checked. Default workspace tests do not execute the custom Gambatte target and retain the repository's existing ignored-test policy, hence the separate complete runs and include-ignored report.

## Failure-family comparison

| Source family | Original failures | Final failures |
| --- | ---: | ---: |
| `bgen` | 2 | 1 |
| `bgtiledata` | 22 | 22 |
| `bgtilemap` | 24 | 24 |
| `cgbpal_m3` | 28 | 28 |
| `display_startstate` | 2 | 0 |
| `div` | 4 | 2 |
| `dma` | 105 | 96 |
| `dmgpalette_during_m3` | 17 | 17 |
| `enable_display` | 14 | 13 |
| `halt` | 7 | 7 |
| `irq_precedence` | 12 | 12 |
| `lcd_offset` | 9 | 10 |
| `lcdirq_precedence` | 3 | 3 |
| `ly0` | 8 | 6 |
| `lyc153int_m2irq` | 1 | 1 |
| `lycEnable` | 31 | 31 |
| `lycint_ly` | 1 | 1 |
| `lycint_lycflag` | 1 | 1 |
| `lycm2int` | 2 | 2 |
| `m0enable` | 19 | 19 |
| `m0int_m0stat` | 1 | 1 |
| `m0int_m3stat` | 1 | 1 |
| `m1` | 25 | 25 |
| `m2enable` | 21 | 21 |
| `m2int_m0irq` | 3 | 3 |
| `m2int_m2irq` | 2 | 2 |
| `m2int_m3stat` | 11 | 11 |
| `miscmstatirq` | 22 | 11 |
| `oam_access` | 5 | 3 |
| `oamdma` | 314 | 314 |
| `scx_during_m3` | 45 | 45 |
| `scy` | 43 | 43 |
| `serial` | 14 | 14 |
| `sound` | 66 | 36 |
| `speedchange` | 33 | 18 |
| `sprites` | 175 | 171 |
| `tima` | 3 | 2 |
| `vram_m3` | 3 | 3 |
| `vramw_m3end` | 3 | 3 |
| `window` | 135 | 135 |

The per-case source inventory is [GAMBATTE_FAILURES.csv](GAMBATTE_FAILURES.csv). I/O columns record explicit literal FFxx accesses; indirect accesses through C are present in the linked source, not guessed by the static audit. Hashes cover decoded source with normalized newlines.

## Complete include-ignored suite comparison

Counts below use suite-scoped identities for both baseline and final logs. All 35 non-Gambatte baseline failures remain unchanged; no case is removed. The generated report totals 3,760 passes and 1,193 failures (4,953 cases), versus a corrected baseline of 3,673 passes and 1,272 failures (4,945 cases). The eight additional cases are the new Rust regression tests.

| Suite | Baseline pass/fail | Final pass/fail | New failures |
| --- | ---: | ---: | ---: |
| `additional_roms` | 39/7 | 39/7 | 0 |
| `age` | 119/0 | 119/0 | 0 |
| `apu` | 69/0 | 71/0 | 0 |
| `apu_quirks` | 3/0 | 3/0 | 0 |
| `bgb_protocol` | 24/0 | 24/0 | 0 |
| `boot_handoff_state` | 2/0 | 2/0 | 0 |
| `bullygb_hacktix` | 2/0 | 2/0 | 0 |
| `cartridge` | 3/0 | 3/0 | 0 |
| `cgb_acid2_rom` | 1/0 | 1/0 | 0 |
| `cgb_acid_hell_rom` | 1/0 | 1/0 | 0 |
| `cgb_sound_roms` | 12/0 | 12/0 | 0 |
| `channel_activation` | 2/2 | 2/2 | 0 |
| `cpu` | 18/0 | 18/0 | 0 |
| `cpu_instrs_rom` | 11/0 | 11/0 | 0 |
| `daid` | 8/0 | 8/0 | 0 |
| `debugger_memory` | 3/0 | 3/0 | 0 |
| `dmg_acid2_rom` | 1/0 | 1/0 | 0 |
| `dmg_sound_roms` | 12/0 | 12/0 | 0 |
| `doc` | 16/0 | 16/0 | 0 |
| `gambatte` | 2192/1237 | 2271/1158 | 2 |
| `gambatte_harness` | 0/0 | 3/0 | 0 |
| `gbmicrotest` | 510/3 | 510/3 | 0 |
| `halt_batch` | 4/0 | 4/0 | 0 |
| `halt_bug_rom` | 1/0 | 1/0 | 0 |
| `instr_timing_rom` | 1/0 | 1/0 | 0 |
| `interrupt_time_rom` | 2/0 | 2/0 | 0 |
| `latch_rtc_test` | 1/0 | 1/0 | 0 |
| `lib` | 138/0 | 138/0 | 0 |
| `little_things` | 4/0 | 4/0 | 0 |
| `mealybug_tearoom` | 62/15 | 62/15 | 0 |
| `mem_timing_rom` | 3/0 | 3/0 | 0 |
| `mmu` | 19/0 | 22/0 | 0 |
| `model_boot` | 4/0 | 4/0 | 0 |
| `mooneye_acceptance` | 87/0 | 87/0 | 0 |
| `mooneye_extended` | 30/2 | 30/2 | 0 |
| `oam_bug_rom_singles` | 1/0 | 1/0 | 0 |
| `pinobatch` | 5/0 | 5/0 | 0 |
| `ppu` | 27/0 | 27/0 | 0 |
| `prehistorik_probe` | 0/1 | 0/1 | 0 |
| `rtc_invalid_banks_test` | 1/0 | 1/0 | 0 |
| `same_suite` | 77/1 | 77/1 | 0 |
| `serial` | 17/0 | 17/0 | 0 |
| `sgb` | 8/0 | 8/0 | 0 |
| `strikethrough` | 1/0 | 1/0 | 0 |
| `timer` | 13/0 | 13/0 | 0 |
| `version_api` | 1/0 | 1/0 | 0 |
| `wilbertpol` | 118/4 | 118/4 | 0 |
