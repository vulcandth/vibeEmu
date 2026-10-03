# Gambatte compatibility review

Base: `e50ee9707bd5e216d0580957f63af3e3981506c8` (`main`). This work is independent of save-state PR #437.

## Measurement and scope

The v7.0 c-sp ROM bundle contains 3,429 runnable Gambatte cases. Runs below include the existing ignored cases. A case may have DMG and CGB expectations; the runner stops that case at its first failing model.

| Configuration | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Original runner and original core | 2,192 | 1,237 | 0 |
| Corrected runner, original core | 2,256 | 1,173 | 0 |
| Corrected runner, changed core | 2,303 | 1,126 | 0 |

The core changes fix 47 cases with no regression against the corrected-runner baseline. Relative to the original measurement, 113 failures pass and two previously passing cases fail because the runner now selects CGB-C rather than CGB-E. These two are listed below; they are not hidden or added to an ignore list. Six false audio passes exposed by the runner correction were investigated and fixed in the core. Seventy-one newly passing cases are removed from the existing ignore list; no ignores are added.

Every original failure has matching assembly in [the upstream test tree](https://github.com/pokemon-speedrunning/gambatte-core/tree/d819bad196/test/hwtests). The tree is unchanged at source checkout `5a41a68c25402421fb1983ddadc9faf2418ddb0f`. The accompanying CSV records each case's expectations, source link, normalized source hash, explicit I/O accesses, fixed-address timing sections, and before/after outcome. This is a complete static source inventory, **not an exhaustive causal diagnosis** of the remaining failures. Detailed analysis covers the runner, pulse retriggering, VRAM DMA, all serial cases, the wave-RAM timing family, noise length, and the cartridge-mode boot-phase conflict. Remaining PPU families have representative source reviews and a static per-case inventory; exhaustive causal diagnosis is unfinished.

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

## Separating measurement corrections from core fixes



The runner corrections were replayed sequentially against the **unchanged base core**. Both endpoint case maps reproduce the original and corrected baselines exactly. Intermediate effects depend on the order because working audio exposes revision-specific behavior.



| Unchanged core, cumulative runner changes | Passed | Failed | Gains / losses from preceding row |

| --- | ---: | ---: | ---: |

| Original runner | 2,192 | 1,237 | — |

| Correct model-specific expectation selection | 2,212 | 1,217 | 20 / 0 |

| Correct PNG color conversion | 2,213 | 1,216 | 1 / 0 |

| Capture actual stereo audio, still CGB-E | 2,226 | 1,203 | 46 / 33 |

| Select the specified CGB-C hardware | 2,256 | 1,173 | 46 / 16 |



At the endpoints, the runner makes 72 original failures pass and exposes eight original false passes: net **64**. The 72 persistent gains partition in this order into 20 expectation-selection, one PNG, 32 audio, and 19 model-selection cases; five of those last 19 are audio cases. These are not independent additive effects of isolated switches. The six exposed pulse-audio failures are fixed by the pulse changes above. The two model-exposed PPU failures remain listed below.



The initial draft's 79 net additional passes were therefore **64 measurement corrections + 15 core fixes**, not 79 core fixes. Continued source-driven investigation adds 13 serial, four wave-RAM, three noise-length and 12 cartridge-mode boot-phase fixes. Final improvement is **64 + 47 = 111 net passes**: 113 original failures now pass, and the same two model-exposed cases fail. No additional Gambatte case regresses relative to the corrected-runner baseline.



## Serial clock: one DIV-driven mechanism



All 46 serial assembly sources were read, including the passing neighbors, fixed-address variations and normal/fast/double-speed pairs. The old implementation used the independent dot divider, ignored FF04 resets, and implicitly treated the second divider stage as free-running. Hardware uses CPU DIV bit 7 (normal serial) or bit 2 (CGB fast serial) to toggle a second stage. SC writes restart that stage low; a falling edge of that stage shifts a bit. Consequently the period remains 512 or 16 CPU clocks regardless of speed mode.



`step_cpu_steps` and `on_div_reset` now call the same edge/shift implementation. The CPU supplies its actual resettable DIV and elapsed CPU clocks. Public dot-domain stepping retains its API by converting once at the boundary. DMG's missing fast-clock capability and CGB compatibility mode remain explicit; the redundant all-zero revision phase adjustment and separate speed-dependent bit-selection paths are removed. Link-port polling, external clocks and transfer completion retain their existing interfaces.



The assembly probes start transfers on opposite halves of DIV, restart them after one/two NOPs, and write DIV immediately before or after its relevant edge. For example, the `div_write_start_wait_read_if` pair samples IF at `054C/054D`, while the late-DIV pairs write at `04EC/04ED` and `052C/052D`; the CGB fast pair uses `0161/0162`. A uniform completion-time offset cannot satisfy these pairs. The two-stage clock fixes 13 cases and leaves **45/46 serial ROMs passing**. Three new Rust tests cover restart phase, reset-generated edges without resetting the second stage, and the actual CPU path across normal/fast/double-speed modes.



The mechanism is independently reflected in SameBoy's [`GB_serial_master_edge` and divider setter](https://github.com/LIJI32/SameBoy/blob/213a12ce93d66b105a113debd9396306066a7cfc/Core/timing.c) and its [SC write handling](https://github.com/LIJI32/SameBoy/blob/213a12ce93d66b105a113debd9396306066a7cfc/Core/memory.c). No ROM identity participates in emulation.



## Wave RAM: correct the byte contract, preserve the access window



All 18 `ch3_reset_nr4init` read/write sources were examined. Four reads of FF30 expect `10`, `32`, `32`, and `54` at their respective playback positions. DMG returned `00`, `22`, `22`, and `44`: its read path duplicated the sample buffer's low nibble. CGB already returned the full current RAM byte.



The shared read path now returns the current full byte when access is permitted. DMG still requires the channel's RAM-read window; CGB redirects to the current byte, and AGB's blocked-read behavior remains intact. This is the byte-oriented contract documented by [Pan Docs](https://gbdev.io/pandocs/Audio_Registers.html#ff30ff3f--wave-pattern-ram) and used by SameBoy's [wave RAM read implementation](https://github.com/LIJI32/SameBoy/blob/213a12ce93d66b105a113debd9396306066a7cfc/Core/apu.c).



**Existing-test correction:** `wave_ram_locked_read_returns_latched_nibble_on_dmg` encoded the incorrect repeated-nibble behavior. Before editing that test, the candidate passed four additional Gambatte cases and failed only this assertion among the 71 APU tests. Its source was inspected, along with the hardware ROMs and references. It is replaced by `wave_ram_locked_read_returns_full_byte_on_dmg`, checking exact full-byte equality for six asymmetric patterns (`9C`, `10`, `32`, `54`, `A5`, `0F`). This is a deliberate contract correction, not an unchanged-regression-suite claim or a relaxed assertion. No external ROM expectation or image was changed.



## Noise length expiry



The six normal/late-DIV noise-length probes initialize length to three (`NR41=3D`), trigger with length enabled and read NR52 one instruction apart. The failing members expected `F0` but observed `F8`. Unlike pulse and wave, noise left its enabled flag set until later waveform processing after length reached zero.



Noise now clears that flag on the length-clock edge itself, preserving the existing waveform suppression. This matches the other channels and SameBoy's length-clock behavior without adding timing constants or another model-specific path. All three failures pass and their earlier-read neighbors remain passing. A new Rust test checks each of the five frame-sequencer edges up to the third length clock, on DMG and CGB-C/E, before another waveform tick can conceal the stale enabled flag.



## Investigated boundaries that remain unresolved



### Interrupt assertion versus acknowledgment



The remaining serial failure, `start_wait_trigger_int8_read_if_2`, and TIMA's `tc00_irq_late_retrigger_2` require DMG to retain a newly asserted interrupt while CGB clears it. All five serial and all five TIMA timing counterparts were read. Serial writes IF at `13E7`, `13E8`, or `13E9`; TIMA uses `11E9`, `11EA`, or `11EB`. Double-speed counterparts further constrain the boundary.



An isolated experiment acknowledged IF two CPU clocks earlier during the final interrupt-entry cycle, following the approximate position in SameBoy's CPU implementation. It fixed three STAT cases but regressed previously passing `serial/start_wait_trigger_int8_read_if_ds_2_cgb04c_outE0` and `tima/tc00_irq_late_retrigger_ds_2_cgb04c_outE0`. The middle normal-speed cases then passed on DMG but failed on CGB with `E8`/`E4` instead of `E0`, so the global shift did not resolve them.



The regressed sources were inspected rather than simply discarding the failures: they demonstrate that source assertion duration and IF set/clear arbitration must be represented coherently. The current plain IF byte does not retain those signal windows, and one-T-clock splits in double speed also need a carried half-dot phase. Gambatte's own `ackIrq` contains source-specific update offsets and a TODO to represent assertion duration instead. Those offsets are not transplanted here. The experimental global shift is not part of the PR; no supported implementation satisfying both boundaries was established.



## Cartridge-mode boot phase: resolving a cross-suite conflict



The two CGB `div/start_inc` sources differ by one NOP and require `1E` then `1F`; the old core read `26` for both. `tima/tc00_start_2` similarly expects `F1` but read `F0`. Gambatte's [reference post-BIOS initialization](https://github.com/pokemon-speedrunning/gambatte-core/blob/d819bad196/libgambatte/src/initstate.cpp) implies DIV `1EA0`. Mooneye's [hardware-verified boot-DIV test](https://github.com/Gekkio/mooneye-test-suite/blob/31510e12eea6286d36eea060a6adde755e1067aa/misc/boot_div-cgbABCDE.s) instead requires the `2678` phase and explicitly covers CGB-C as well as E.



An isolated revision-only `1EA0` change gained 12 Gambatte cases but failed that Mooneye ROM when its wrapper was run on CGB-C. Its six DIV reads and cartridge header were then inspected. The decisive distinction is **cartridge mode**: Mooneye's header has CGB flag `00`, while these Gambatte ROMs have `80`/`C0`. The [CGB boot ROM's `SetupCompatibility`](https://codeberg.org/ISSOtm/gb-bootroms/src/commit/e10154ee7962873d9fa7ec02be6a6d4299bad208/src/cgb.asm) takes an additional palette-installation path for monochrome cartridges. A local actual-boot-ROM trace corroborated separate handoff phases (`267C` versus `1E8C` on the current CGB-C power-on emulation), while also showing that full power-on emulation still has a small phase error. Those trace values are not used as new offsets.



Post-boot initialization now selects the independently tested `2678` compatibility phase or `1EA0` native-CGB phase from the cartridge mode for CGB A–E. The selection lives in the shared initialization path, not in either ROM runner. CGB0 and AGB retain their independently existing seeds because these tests do not establish their native-mode values. Actual boot-ROM execution remains responsible for its own evolving DIV. Both CPU DIV and the dot-divider phase are initialized consistently.



This explains and resolves the regression instead of giving up at a revert. The native-mode change fixes two DIV, one TIMA and nine sound ROMs without a Gambatte loss; Mooneye's compatibility-mode requirement remains satisfied. The remaining initial-position/envelope/length sources were also read, but their unresolved waveform/frame-sequencer state is not claimed fixed by this divider correction. In particular, `apply_post_boot_state` writes NR13/NR14 mirror values `C1`/`87`, while the constructor initializes the internal pulse frequency separately and the bootstrap sets the same duty position on both models. A complete waveform phase/suppression initialization is still needed; no guessed duty index or countdown is introduced here.



## PPU and bus timing work still open



Representative source bodies were followed for the remaining OAM DMA, sprite, window, scroll, tile-map/data, palette, HALT and VRAM-access groups. They exercise different mechanisms: OAM DMA modifies bytes during sprite selection; sprite fetch stalls delay HBlank; window reenable/WX/WY writes alter fetcher state; SCX/SCY and LCDC map/data writes straddle tile-fetch latches; palette and VRAM probes sample separate bus-access boundaries. HALT cases additionally distinguish interrupt wakeup from handler-entry time. PNG cases preserve pixel expectations, not merely total mode-3 duration.



These mechanisms are coupled to the current fetcher and bus scheduling. No candidate shared implementation has been established that satisfies all their passing neighbors and the existing AGE/Mealybug coverage. A global mode-3 offset or CGB-C/E behavior flattening would not constitute a source-supported fix. The CSV is a complete source inventory, **not evidence that every remaining case has received a complete causal diagnosis**. Exhaustive diagnosis of these families remains unfinished; this draft records that limitation explicitly.


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
| Full workspace debug | 1,531 passed, 0 failed, 41 ignored | 1,544 passed, 0 failed, 41 ignored |
| Full workspace release | 1,531 passed, 0 failed, 41 ignored | 1,544 passed, 0 failed, 41 ignored |
| Complete Gambatte release | 2,192 passed, 1,237 failed | 2,303 passed, 1,126 failed |
| Complete Gambatte debug | Not separately run | 2,303 passed, 1,126 failed |
| Formatting / workspace Clippy | Not separately measured | Pass / pass with warnings denied |
| Dependency policy (`cargo deny --locked check`) | Dependency graph unchanged | Pass |
| Python status-generator regressions | Tests did not exist | 2 passed |

Final debug/release Gambatte outcomes agree for all 3,429 cases. Full workspace runs add 13 Rust regression tests. One pre-existing APU test is renamed and corrected to assert exact full-byte wave-RAM reads; the explicit contract correction and evidence are described above. No external ROM expectations are modified. Manual gameplay, hardware captures, and analog audio listening were not performed.

### Reproduction

Use the repository's c-sp v7.0 ROM bundle at `crates/vibe-emu-core/test_roms` and the same Rust toolchain (local runs used Rust/Cargo 1.98.1 on Windows). The original baseline is commit `e50ee9707bd5e216d0580957f63af3e3981506c8`. To reproduce the corrected-runner baseline independently, use the final test runner and expectation helper with the entire core `src` tree from that base in a separate checkout.

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
| `div` | 4 | 0 |
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
| `serial` | 14 | 1 |
| `sound` | 66 | 20 |
| `speedchange` | 33 | 18 |
| `sprites` | 175 | 171 |
| `tima` | 3 | 1 |
| `vram_m3` | 3 | 3 |
| `vramw_m3end` | 3 | 3 |
| `window` | 135 | 135 |

The per-case source inventory is [GAMBATTE_FAILURES.csv](GAMBATTE_FAILURES.csv). Its intermediate runner columns record the cumulative replay above, including working audio before CGB-C selection. `final_diagnostic` preserves the measured failure message for each remaining case; the runner stops at the first failing model. I/O columns record explicit literal FFxx accesses; indirect accesses through C are present in the linked source, not guessed by the static audit. Hashes cover decoded source with normalized newlines.

## Complete include-ignored suite comparison

Counts below use suite-scoped identities for both baseline and final logs. All 35 non-Gambatte baseline failures remain unchanged. The wave-RAM assertion is explicitly corrected and renamed; other pre-existing tests are retained. The generated report totals 3,797 passes and 1,161 failures (4,958 cases), versus a corrected baseline of 3,673 passes and 1,272 failures (4,945 cases). The 13 additional cases are new Rust regression tests; the wave-RAM test is a one-for-one replacement with a stricter corrected contract.

| Suite | Baseline pass/fail | Final pass/fail | New failures |
| --- | ---: | ---: | ---: |
| `additional_roms` | 39/7 | 39/7 | 0 |
| `age` | 119/0 | 119/0 | 0 |
| `apu` | 69/0 | 72/0 | 0 |
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
| `gambatte` | 2192/1237 | 2303/1126 | 2 |
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
| `model_boot` | 4/0 | 5/0 | 0 |
| `mooneye_acceptance` | 87/0 | 87/0 | 0 |
| `mooneye_extended` | 30/2 | 30/2 | 0 |
| `oam_bug_rom_singles` | 1/0 | 1/0 | 0 |
| `pinobatch` | 5/0 | 5/0 | 0 |
| `ppu` | 27/0 | 27/0 | 0 |
| `prehistorik_probe` | 0/1 | 0/1 | 0 |
| `rtc_invalid_banks_test` | 1/0 | 1/0 | 0 |
| `same_suite` | 77/1 | 77/1 | 0 |
| `serial` | 17/0 | 20/0 | 0 |
| `sgb` | 8/0 | 8/0 | 0 |
| `strikethrough` | 1/0 | 1/0 | 0 |
| `timer` | 13/0 | 13/0 | 0 |
| `version_api` | 1/0 | 1/0 | 0 |
| `wilbertpol` | 118/4 | 118/4 | 0 |

The desktop executor briefly disconnected during an intermediate validation run. Persistent logs verified that its debug and release workspace runs passed; its subsequent checks were superseded. Final results above come from a new complete run on the integrated code with bounded compiler concurrency. Dependency policy also passed after allowing Cargo to lock its advisory cache; the initial sandbox-only lock failure was environmental.
