# Downloaded ROM coverage

`additional_roms.rs` covers the remaining runnable ROMs from the pinned c-sp
v7.0 bundle. Run it with `cargo test -p vibe-emu-core --test additional_roms`.
Use `-- --ignored` to investigate the exceptions listed with individual reasons
in `additional_roms_ignored.txt`. Unknown or duplicate ignore entries are errors.

## Added cases

The harness registers 46 cases: 39 passing, three initially failing, and four
diagnostic entries without a supplied automated verdict. Models are DMG-C and
CGB-D; DMG cartridges run in CGB compatibility mode on the latter.

| ROMs | Check |
| --- | --- |
| Scribbl lycscx, lycscy, palettely, scxly | Complete frame against bundled PNG after 30 frames' worth of dots, on both models |
| Scribbl statcount-auto | Bundled PNG after 300 frames' worth of dots, covering all 255 NOP counts |
| Scribbl statcount | Script Start, then require the ROM's `OK` result for its default one-NOP probe, on both models |
| Turtle window_y_trigger and window_y_trigger_wx_offscreen | Bundled PNG after 40 frames' worth of dots, on both models |
| MBC3 Bank Tester | Bundled PNG after 60 frames' worth of dots, on both models |
| RTC3test | Script A, Down/A, or Down/Down/A; check each of the three bundled result screens on both models |
| Blargg CPU instructions, memory timing, memory timing v2 aggregates | Final bundled PNG on both models |
| Blargg DMG/CGB sound aggregates | Final bundled PNG on the corresponding model |
| Blargg OAM aggregate | Final bundled PNG on DMG |
| Blargg memory timing v2 singles | Documented RAM signature/result or serial verdict on both models |
| Blargg OAM 7-timing_effect single | Documented RAM signature/result on DMG |
| Bundled Bully | Real boot ROM, then the serial `All tests OK!` verdict on both models |

The bundled Bully how-to reports a `Bad Echo RAM Reads` failure on real DMG-C;
this older ROM currently reports success here on both models. Its DMG result
should therefore not be treated as proof of hardware-accurate echo-RAM behavior.
The existing BullyGB v1.2 checks remain in place.

All waits use bounded emulated time, including frame completion and scripted
input. RTC tests advance the emulated clock, with no wall-clock sleeps. Aggregate
Blargg ROMs require their final screen; an individual subtest's passing RAM status
must not end the aggregate early. No reference images are generated from vibeEmu.

Three reference exports need explicit color normalization, confined to those
files and colors: Scribbl `scxly-cgb.png` uses green black/white shades
(`#0F380F`/`#98C00F`), MBC3's CGB PNG uses `#7BFF4A` for the documented
`#7BFF31` shade, and the OAM aggregate PNG has `#FEFEFE`/`#FDFDFD` white pixels
on its last row. Pixel locations must still match exactly; no general image
tolerance or ignored region is used.

### Remaining exceptions

* Turtle's `window_y_trigger` fails on both models: its source sets WY to 144
  in VBlank, then to zero at LY=56. This must not trigger the window. The
  rendered screen differs from the reference by 1,716 pixels. The separate
  offscreen-WX test passes.
* Standalone Blargg `7-timing_effect` continues printing OAM dumps beyond
  60 emulated seconds, without a valid final RAM signature/verdict. The
  aggregate's final screen passes. A separate 240-second probe also found no
  `Passed` or `Failed` text in either tilemap. This discrepancy needs further
  investigation; it is not classified as interactive or asserted to be a faulty
  ROM.
* Scribbl `fairylake` is an animated graphics demo, **not interactive**. Neither
  an assertion nor a fixed reference frame is supplied.
* Scribbl `winpos` is a visual debugger whose arrow buttons change WX/WY. Its
  source has no pass/fail assertion and the bundle supplies no reference image.
  Merely scripting buttons would not validate the displayed window behavior.

The last two ROMs remain visible as ignored diagnostics on each model. Running
them explicitly reports the missing validation criterion rather than a false
pass. Manual `statcount` and RTC3test do have verifiable results and are automated.

Sources inspected alongside each bundled `game-boy-test-roms-howto.md`:

* [Scribbl source and per-ROM README files](https://github.com/Hacktix/scribbltests/tree/96dd2f14bc8cce1fd5df25427056e059a175e9f7)
* [Turtle window trigger source](https://github.com/Powerlated/TurtleTests/blob/b341ff54ec1e6a501d37dd309c556b6968a07eec/src/window_y_trigger/window_y_trigger.asm)
* [RTC3test menu and tests](https://github.com/aaaaaa123456789/rtc3test/tree/80ae792bf1b3c3387929b912e6df001af3511e24/src)
* [Blargg OAM source and result protocol](https://github.com/retrio/gb-test-roms/tree/c240dd7d700e5c0b00a7bbba52b53e4ee67b5f15/oam_bug)

## Existing coverage and non-test assets

| Download directory | Harness or disposition |
| --- | --- |
| age-test-roms | `age.rs` |
| blargg | Existing `*_rom.rs`, `*_sound_roms.rs`, `oam_bug_rom_singles.rs`, plus `additional_roms.rs` |
| bootroms | Firmware fixtures used by boot/hardware tests; not standalone test ROMs |
| bully | `additional_roms.rs`; distinct older binary from BullyGB v1.2 |
| bullygb | `bullygb_hacktix.rs`; `bully.gb` and `bullygb-v1.2.gb` are byte-identical |
| cgb-acid-hell, cgb-acid2, dmg-acid2 | Corresponding acid ROM harnesses |
| daid | `daid.rs` |
| gambatte | Existing opt-in harness; deliberately excluded from default runs |
| gameboy_cpu_tests_v2 | JSON CPU vectors, not ROMs; require a separate flat-memory, prefetched-CPU adapter |
| gbeshootout | `rtc_invalid_banks_test.rs`, `latch_rtc_test.rs` |
| gbmicrotest | `gbmicrotest.rs` |
| hacktix, strikethrough | `strikethrough.rs`; both downloaded ROM copies are byte-identical |
| little-things-gb | `little_things.rs`, `pinobatch.rs` (firstwhite/tellinglys) |
| mbc3-tester, rtc3test, scribbltests, turtle-tests | `additional_roms.rs` |
| mealybug-tearoom-tests | `mealybug_tearoom.rs`, including DMA and MBC3 RTC cases |
| mooneye-test-suite | `mooneye_acceptance.rs`, `mooneye_extended.rs` |
| mooneye-test-suite-wilbertpol | `wilbertpol.rs` |
| same-suite | `same_suite.rs` |

Existing suites retain their documented model and diagnostic exclusions. The
duplicate downloads do not need duplicate tests. The root `actual_output.png`
is generated output, not an additional ROM.
