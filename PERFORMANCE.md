# Core performance and the 3DS target

## Reproducing measurements

Build the dependency-free emulation core's profiling example:

```bash
cargo build --release -p vibe-emu-core --example profile_core
target/release/examples/profile_core polishedcrystal-debug-3.2.3.gbc 1800 300 audio
perf record -e cycles:u -F 999 -o /tmp/vibe-core.perf -- \
  target/release/examples/profile_core polishedcrystal-debug-3.2.3.gbc 1800 300 audio
perf report -i /tmp/vibe-core.perf --stdio --no-children
```

Arguments are ROM, measured frames, warmup frames, and `audio` or `silent`.
One benchmark frame is 70,224 dots, including periods with the LCD disabled.
The default workload runs 300 warmup frames and then measures 1,800 frames.
Audio mode exercises the mixer, high-pass filter, and queue at 48 kHz; silent
mode still emulates the APU but has no output consumer. ROM bytes are loaded
without reading or writing save files or synchronizing the RTC to wall time.
Hardware model selection follows the cartridge header.

The reported time covers CPU/core stepping, including audio production. Loading,
hashing, and draining the audio queue are outside the timed region. The hashes
cover every measured framebuffer and stereo sample, rather than only the final
screen. Dot counts, sample counts, and final PC provide additional consistency
checks. This is a deterministic introductory sequence without user input, not
a gameplay or frontend benchmark.

Preserve a baseline binary before modifying the core. Compare binaries with:

```bash
python3 scripts/benchmark_core.py /tmp/vibe-core-before \
  target/release/examples/profile_core polishedcrystal-debug-3.2.3.gbc
```

The script alternates execution order, reports the median of five runs, and
fails if either binary produces different emulation outputs. Run benchmarks
without compilation or tests competing for CPU time. Use several ROMs and
both output modes. Sampling profiles include hashing/draining overhead even
though the example's reported duration excludes it.

## September 2026 optimization pass

Baseline source: `83f3465`; Linux x86-64, Intel Core i7-4790, Rust 1.98.1,
the existing release profile (thin LTO, one codegen unit), no native-CPU flags.
Polished Crystal ROM SHA-256:
`a8dc269c102152be2bd5eb42cf0f5d13fd8d10dadf6fadd88df802caf26e9c42`.

The initial headless sampling profile attributed approximately 42% of CPU
samples to APU stepping/staging/wave updates, 30% to PPU stepping/rendering/OAM,
10% to CPU tick orchestration, and 5% to CPU instruction execution. These are
exclusive sample shares, not subsystem wall-clock measurements.

Hardware counters for a 120-frame warmup plus 600 measured Polished Crystal
frames with audio dropped from 19.08 billion to 16.68 billion retired user
instructions (12.5%), and from 4.03 billion to 3.28 billion branches (18.7%).
Counters include the entire process, including warmup and checksum work.

Final timings, median of five runs per binary with alternating execution order:

| Workload | Measured / warmup frames | Baseline | Optimized | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Polished Crystal, 48 kHz audio | 1,800 / 300 | 4.554574 s | 4.042220 s | 1.127× |
| Polished Crystal, silent | 1,800 / 300 | 4.417284 s | 3.968924 s | 1.113× |
| DMG acid2, audio output enabled | 600 / 120 | 1.007113 s | 0.963817 s | 1.045× |
| Blargg CPU instructions, CGB, audio output enabled | 600 / 120 | 1.127368 s | 0.936803 s | 1.203× |

Every run matched the baseline's video hash, audio hash, sample count, dot count,
and final PC. Polished Crystal with audio improved from 395.21 to 445.30
benchmark frames/s (11.25% less execution time). Its measured output was
`video=03a4cb21c95afd55`, `audio_hash=dc58d484d4802d22`, 1,446,570 stereo
samples, and 126,403,200 dots. The test-ROM workloads' measured audio was
silence; Polished Crystal supplies the active-audio comparison.

The x86-64 benchmark's `size` text measurement decreased slightly, from
575,836 to 575,292 bytes. These changes do not add large lookup tables or
architecture-specific code.

At the end of the first pass, an audio-enabled core profile placed about 45% of samples in APU
stepping, staging, and wave updates, versus about 5% in CPU instruction
execution. This made the APU deadline work below the next measured priority.

Implemented changes:

- Pack the three audio sample latches into one 32-bit word. Advancing a channel
  preserves all three stages, including the two-tick CGB double-speed case.
  Remove duplicate per-channel silence checks and dispatch the common staging
  count once for all four channels. Sweep boundaries still split staging batches.
- Advance native CGB drawing time directly when the next sprite attribute
  latch and end-of-transfer housekeeping lie beyond the requested interval.
  Lines with LCDC writes, pending register writes, STAT delays, DMA overlap,
  DMG mode, and CGB DMG compatibility keep the existing stepping path.
  OAM order and offscreen sprite
  latches retain their existing behavior. HBlank/HDMA timing is unchanged.
- Add a repeatable benchmark with video/audio checksums and a comparison script.

Accuracy coverage includes exhaustive valid audio latch states and partial
flushes, plus differential PPU execution against the original stepping path.
The PPU comparison exercises CGB revisions C/E, both sprite priority orders,
offscreen and overlapping sprites, register writes, DMA overlap, large and
small tick chunks, KEY0 mode changes, DMG compatibility, and DMG hardware.

Validation on the final source:

- `cargo fmt --all` and formatting check: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test`: 561 passed, 33 existing ignored tests.
- `cargo test --release`: 557 passed, 33 existing ignored tests.
- Core compilation with all tracing features enabled: passed.

The test suites require permission to bind a local socket for the frontend's
link-cable handshake test. Initial sandboxed runs hit `PermissionDenied` there;
both complete suites passed when rerun outside the sandbox. Gambatte was not
run in this pass; it remains an optional informational suite.

## APU deadline pass

The first pass is preserved in commit `20583d6` on
`perf/apu-event-scheduling`. This follow-up changes only APU scheduling and its
regression tests; CPU, MMU, and PPU scheduling retain their existing behavior.

The combined CPU/APU tick now caches a conservative deadline for the two square
channels and wave channel. Between their waveform edges, once their three-stage
output latches have settled, it advances counters directly without recomputing
samples or shifting identical latches. The deadline is invalidated by register
accesses and standalone clock operations. DIV edges, CPU speed changes, odd
clock phases, mismatched clock-domain increments, sweep work, restart delays,
and pending wave-RAM operations use the original path. Public timer getters
still observe current counters; no externally visible state is left deferred.

Noise is deliberately independent: its short prescaler frequently expires every
M-cycle. An initial all-channel deadline prototype regressed: its profile showed
costly deadline prediction and a separate call for every sample-clock update.
The retained implementation clocks noise normally while reusing the square/wave
deadline and keeps the common sample-clock update inline. Audio sampling still
runs before the noise pipeline shift, preserving sample timing and the existing
high-pass filter exactly. This adds no tables, dependencies,
unsafe code, or platform-specific behavior. Release executable text grows by
1,792 bytes (0.31%) relative to the first pass.

A differential test compares all private emulation state (excluding the new
cached deadline and the queue handles) against the original three-operation
sequence over 900,000 tick comparisons, and compares every queued audio sample. It covers all
ten DMG/CGB revisions, zero/48/96 kHz rates, subsequent rate changes, single and
double speed, DIV wrap/reset, power cycling, channel retriggers, deterministic
register writes, wave-RAM reads/writes, standalone clocks, frozen dividers,
unequal clock increments, odd/zero ticks, and large batches. Each configuration
must actually exercise the optimized path.

Timings versus the committed first pass, median of five alternating runs:

| Workload | Measured / warmup frames | First pass | APU deadlines | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Polished Crystal, 48 kHz audio | 1,800 / 300 | 4.024329 s | 3.133705 s | 1.284× |
| Polished Crystal, silent | 1,800 / 300 | 4.027387 s | 3.033055 s | 1.328× |
| DMG acid2, audio output enabled | 600 / 120 | 0.974530 s | 0.884210 s | 1.102× |
| Blargg CPU instructions, CGB, audio output enabled | 600 / 120 | 0.956710 s | 0.888713 s | 1.077× |

All output signatures matched. A separate three-run comparison of Blargg's CGB
wave test also matched and improved by 1.080×. Timings vary with host activity;
the paired medians, rather than a single best run, are the reported comparison.

A final five-run comparison against the original `83f3465` binary measured
4.571364 s versus 3.157368 s for Polished Crystal with audio (1,800/300 frames).
The two passes together deliver **1.448× throughput, or 30.93% less execution
time**, with matching output signatures. This is a direct cumulative comparison,
not multiplication of speedups measured in separate sessions.

For the same 600/120-frame Polished Crystal counter workload, retired user
instructions fell from 16,683,850,981 to **13,388,158,840** (19.75%), and branches
from 3,278,634,238 to **2,567,521,701** (21.69%). Relative to the original
pre-optimization binary, the instruction reduction is **29.82%**. Reproduce:

```bash
perf stat -e instructions:u,branches:u -- \
  target/release/examples/profile_core polishedcrystal-debug-3.2.3.gbc 600 120 audio
```

The updated exclusive sampling profile attributes 22.5% to the APU quiet path,
9.6% to noise clocking, 14.3% to scanline rendering, 12.9% to PPU stepping,
13.9% to CPU tick orchestration, and 6.6% to CPU instruction execution. These
shares guide further work; eliminating a percentage of samples is not a
promise of the same percentage improvement in wall time.

Validation on the final APU implementation:

- `cargo fmt --all`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test`: 562 passed, 33 existing ignored tests.
- `cargo test --release`: 558 passed, 33 existing ignored tests.
- `cargo check -p vibe-emu-core --all-features`: passed (separate target directory).
- Gambatte was not run; it remains informational.

## Rough 3DS budget without hardware testing

Use instruction counts as a work proxy, not desktop FPS scaled by clock speed.
[3dbrew's hardware research](https://3dbrew.org/wiki/Hardware) documents roughly
268 MHz for the original 3DS and 804 MHz for New 3DS application cores with
speedup enabled. This estimate budgets **one core** for the serial emulator;
it does not multiply capacity by the number of CPU cores. The New 3DS frontend
must enable the appropriate speed/cache mode through
[libctru's `osSetSpeedupEnable`](https://github.com/devkitPro/libctru/blob/master/libctru/include/3ds/os.h).

The counter workload advances `(600 + 120) × 70,224 / 4,194,304 = 12.05475`
emulated seconds. Its 13.388 billion x86-64 instructions correspond to
**1.111 billion host instructions per emulated second**, or 18.595 million
per benchmark frame. This includes startup, warmup, checksumming, and queue
draining; it is a conservative process-level proxy rather than an isolated
count of core instructions. It still represents an introductory ROM sequence,
not a representative gameplay collection.

Define the estimated remaining speedup requirement as:

```text
required speedup = host instructions per emulated second
                 × ARM instructions per x86 instruction
                 × ARM cycles per instruction
                 / (target cycles per second × fraction available to core)
```

The instruction-expansion and CPI factors below are **uncalibrated assumptions**,
not measurements or confidence bounds. ARM11's instruction latencies, load
interlocks, branches, caches, 32-bit handling of 64-bit counters, and generated
code can move actual results outside this range. See the
[ARM11 MPCore Technical Reference Manual, chapter 15](https://documentation-service.arm.com/static/5e8e1cd9fd977155116a4a7a)
for the underlying execution constraints. Equal instruction counts do not imply
equal cycle costs across these architectures.

| Scenario | ARM/x86 instruction ratio | ARM CPI | Core CPU budget | Old 3DS remaining speedup | New 3DS remaining speedup |
| --- | ---: | ---: | ---: | ---: | ---: |
| Optimistic | 1.0 | 1.0 | 90% | 4.6× | 1.5× |
| Working planning assumption | 1.5 | 1.5 | 80% | 11.7× | 3.9× |
| More costly ARM execution | 2.0 | 2.0 | 75% | 22.1× | 7.4× |

A 3.9× requirement means roughly 26% of real time under that scenario. The
working interpretation is **several-fold more improvement for New 3DS and
roughly an order of magnitude for old 3DS**. These are development budgets,
not a claim that either machine has achieved those speeds.

For a provisional future hardware-testing trigger, reduce this same workload
to about **0.43 billion host instructions per emulated second** (about 7.2 million
per frame), while keeping checksums and tests passing. That would bring the
working New 3DS estimate within 1.5× of full speed, requiring another ~2.6×
reduction in this work proxy from today's result. A 1.0× working budget is
~0.286 billion instructions/s. Revisit these targets after ARM assembly analysis;
a target build and a broader gameplay workload set can improve the estimate
without requiring a physical console. Only the x86-64 Rust target is installed
in the current environment, so no ARM binary was measured in this pass.

## Research and next opportunities

The scheduling approach follows the principle that cycle accuracy requires
correctly timed interactions, not redundant processing in intervals with no
interaction. mGBA describes batching and splitting at interactions;
GameRoy describes lazy peripherals with predicted interrupt boundaries and an
interpreter fallback. This pass applies conservative deadlines inside the APU
without changing the CPU's scheduling granularity.
[mGBA design discussion](https://mgba.io/2017/04/30/emulation-accuracy/),
[GameRoy implementation discussion](https://rodrigodd.github.io/2023/09/02/gameroy-jit.html).

[SameBoy's APU implementation](https://github.com/LIJI32/SameBoy/blob/master/Core/apu.c)
also avoids processing unchanged inputs in its band-limited update functions.
That supports investigating work proportional to signal transitions. Adopting
a different resampler would change audio output, so this pass retains vibeEmu's
existing sample clock and filter and instead skips redundant digital staging.

1. **Reduce counter materialization inside the quiet path.** It now occupies
   more samples than waveform-edge handling. A further lazy timestamp approach
   could defer counter writes until an observable access or deadline. Reads,
   public timer getters, noise alignment, wave-RAM collision flags, and sample
   emission must all remain exact. Keep the existing path and full-state
   differential tests as the oracle.
2. **Predict meaningful noise edges.** Noise clocking is now a measurable 9.6%
   of samples. Its divider counter has many transitions that do not change the
   selected LFSR clock bit. Predict the next selected-bit rising edge and update
   intervening counter arithmetic in closed form, while splitting at NR43
   writes, delayed starts, pending disables, DIV events, and PCM reads. Merely
   adding every prescaler reload to a global deadline was counterproductive.
3. **Predict interrupts before batching HALT or CPU work.** Tick orchestration
   still takes 13.9% of samples. Conservative HALT deadlines could skip repeated
   visits to peripherals while preserving the wakeup cycle. DMA, timer reloads,
   STAT edges, serial, and memory-mapped writes must bound or invalidate batches.
   A CPU-only JIT still has limited upside with instruction execution at 6.6%.
4. **Inspect an ARM build before architecture-specific tuning.** Rust documents
   the `armv6k-nintendo-3ds` target and its devkitARM/build-std requirements.
   Examine instruction expansion, variable division, 64-bit counters, and code
   layout to refine the budget above. Hardware tests can wait for that budget
   to become promising.
   [Rust target documentation](https://doc.rust-lang.org/rustc/platform-support/armv6k-nintendo-3ds.html).

Avoid large tables or aggressive inlining without measuring their cache cost.
For example, an earlier branch avoiding division on single waveform edges
regressed the desktop workload and was not retained.
