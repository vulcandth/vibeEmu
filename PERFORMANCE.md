# Core performance and the 3DS target

The latest [New 3DS research pass](NEW_3DS_RESEARCH.md) audits actual ARMv6K
assembly and refreshes PGO measurements against `f0f47e4`. It identifies
recurring PPU configuration barriers and software division in cartridge reads
as concrete next targets, with separate plans for the larger DMG gap.

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
Hardware model selection follows the cartridge header. The runner uses
`Cpu::run_for_dots`, bounded by the remaining frame budget and a 4,096-dot cap.
Desktop and Android normal execution use the same API and cap. Breakpoints and
external link endpoints retain instruction polling; debugger single-stepping
continues to use `Cpu::step`.

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

## Deferred APU counters and static CGB tile spans

Baseline for this pass: `9f20425`, preserved as `/tmp/vibe-core-pass2`.

- **Defer square/wave counter updates within the existing deadline.** Store a
  bounded count of elapsed 2 MHz ticks and materialize it before register
  accesses, DIV events, or standalone clock operations. The wave timer getter
  projects its current value without forcing synchronization. PCM samples,
  noise, clock phase, audio sampling, and filtering remain current. This
  extends the lazy peripheral approach described by
  [GameRoy](https://rodrigodd.github.io/2023/09/02/gameroy-jit.html), while keeping
  the existing APU event boundaries and public observations.
- **Specialize common noise ticks.** Handle zero or one prescaler reload
  directly; retain the old event loop for multiple reloads, delayed starts,
  pending disables, and divisor adjustments. Detect the selected ripple-counter
  bit's rising edge with integer masks, including the non-clocking shifts
  14/15 documented by [Pan Docs](https://gbdev.io/pandocs/Audio_Registers.html).
  The expanded tests exposed an existing signed overflow in the noise trigger's
  alignment calculation. Wrapping that addition now makes debug behavior agree
  with release behavior at the counter boundary.
- **Keep sample production outside the frequent clock-update path.** The
  clock/quiet/noise fast paths are inlined, while mixing/filtering/queue output
  lives in a separate function. No resampling or filtering arithmetic changes.
  Rust's [code-generation attributes](https://doc.rust-lang.org/reference/attributes/codegen.html)
  are compiler hints, so the layout choice was benchmarked rather than assumed
  to help. The noise specialization alone showed little timing benefit; the
  complete APU changes reduced instruction count before the PPU work was added.
- **Render static CGB lines in tile spans.** With no recorded LCDC, SCX, SCY,
  WX, or WY changes, fetch tile attributes and bitplanes once per tile fragment
  and write its pixels directly. This removes the temporary heap FIFO and its
  per-dot fetcher state machine on those lines. Dynamic lines retain the old
  fetcher; sprite composition, PPU timing, interrupts, DMA, and DMG rendering
  are unchanged. The optimization uses the eight-pixel tile structure described
  in [Pan Docs' pixel FIFO documentation](https://github.com/gbdev/pandocs/blob/master/src/pixel_fifo.md),
  with the existing renderer as the behavioral reference, including its window
  clipping and blocked-VRAM behavior.

Accuracy checks now include 940,000 APU tick comparisons, with deferred state
projected for comparison without flushing the live optimized APU. This exercises
multi-tick accumulation, full deadline exhaustion with a frozen divider, and
64-bit clock-counter wrap. Another 147,456 noise cases compare every channel
field against the old event loop. The CGB renderer compares 524,288 scanlines
against the old dot fetcher, including all SCX/WX combinations, window eligibility,
tile addressing modes, maps, flips, palettes, VRAM banks/blocking, sprite
priority arrays, and window-line counter wrap. The existing ROM suites remain
part of validation; these differential checks establish equivalence to the
previous implementation, not proof of perfect hardware accuracy.

Final paired medians versus `9f20425`, five alternating runs per binary:

| Workload | Measured / warmup frames | Before | After | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Polished Crystal, 48 kHz audio | 1,800 / 300 | 3.131796 s | 2.550371 s | 1.228× |
| Polished Crystal, silent | 1,800 / 300 | 3.011827 s | 2.458987 s | 1.225× |
| DMG acid2, audio output enabled | 600 / 120 | 0.872113 s | 0.826380 s | 1.055× |
| Blargg CPU instructions, CGB, audio output enabled | 600 / 120 | 0.879294 s | 0.707105 s | 1.244× |
| CGB acid2, audio output enabled | 600 / 120 | 0.563701 s | 0.410060 s | 1.375× |

Every compared output signature matched. A three-run CGB wave-test comparison
also matched, with a 1.288× median speedup. As before, only Polished Crystal
provides active audio in these measured intervals; the test-ROM audio hashes
represent silence.

A separate five-run comparison against the original `83f3465` binary measured
4.683304 s versus 2.607220 s for Polished Crystal with audio (1,800/300 frames):
**1.796× cumulative throughput, or 44.33% less execution time**, with matching
output signatures. This comparison measures the combined result directly.

The 600/120-frame Polished Crystal hardware-counter workload now retires
**9,706,127,533 instructions** and **1,958,197,581 branches**: 27.50% and 23.73%
fewer than `9f20425`. Instruction count is **49.12% lower** than the original
`83f3465` baseline. The combined executable text is 583,316 bytes, up 6,232 bytes
(1.08%) from `9f20425`; no lookup tables, dependencies, or unsafe code were added.

In the latest sampling profile, `Cpu::tick` has 42.8% of exclusive samples,
including the newly inlined APU paths. This must not be interpreted as CPU
orchestration alone or compared directly with its earlier 13.9% share. PPU
stepping has 19.7%, `Cpu::step` 8.7%, timer stepping 5.1%, the static CGB span
without sprites 3.3%, DMA stepping 3.2%, and audio sample production 2.6%.

Validation on the final combined implementation:

- `cargo fmt --all`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test`: 565 passed, 33 existing ignored tests.
- `cargo test --release`: 561 passed, 33 existing ignored tests.
- `cargo check -p vibe-emu-core --all-features`: passed (separate target directory).
- Gambatte was not run; it remains informational.

## HALT batching with local APU scheduling

Baseline for this pass: `2aa334a`, preserved as `/tmp/vibe-core-pass3`.
Instrumentation of the 2,100-frame Polished Crystal workload found 114,957,248
of 147,470,400 dots spent in halted CPU calls (78.0%), and 57,283,389 of
64,262,136 calls entering with the CPU halted (89.1%). The small interrupt-entry
cost inside a waking HALT call is included. Instrumentation was removed before
timing comparisons.

The main opportunity was to stop revisiting every device during those waits:

- `Cpu::step_with_halt_batch` combines complete halted M-cycles, bounded by the
  caller's budget, PPU events, and timer overflow. It retains `Cpu::step` for
  pending interrupts, STOP/faults, OAM DMA, GDMA, and active serial transfers.
  The PPU deadline stops strictly before mode transitions, register effects,
  sprite latches, and frame delivery, preserving HBlank DMA and interrupt timing.
  Single-step/debugger behavior stays on the original API.
- The APU runs its own event loop inside a CPU batch. Waveform events do not
  require returning to the timer, PPU, cartridge, and CPU scheduler. Settled
  outputs can advance together until the next waveform/DIV event; transition
  intervals retain the original M-cycle ordering of channel clocks, sampling,
  and output pipelines. Sample production and high-pass filtering still run
  for every sample, with unchanged arithmetic and bookkeeping counters.
- Noise prescaler reloads between LFSR edges are counted arithmetically, including
  their counter wrap and final phase. The general loop remains for edges,
  delayed starts, disable effects, and divisor glitches.
- TIMA falling edges are counted directly when they cannot overflow. A pending
  reload or TMA write retains cycle stepping; the HALT deadline ends before an
  overflow. This avoids changing the delayed interrupt and write-collision
  behavior described in [Pan Docs' timer documentation](https://github.com/gbdev/pandocs/blob/master/src/Timer_Obscure_Behaviour.md).

An initial implementation bounded the entire machine by every APU event. Its
median speedup was only 1.16×. Keeping those events local to the APU produced
substantially larger batches. This applies the interaction-boundary approach
in [mGBA's design discussion](https://mgba.io/2017/04/30/emulation-accuracy/) and
[GameRoy's lazy-peripheral scheduling](https://rodrigodd.github.io/2023/09/02/gameroy-jit.html).
HALT still wakes on pending enabled interrupts regardless of IME, as documented
in [Pan Docs](https://github.com/gbdev/pandocs/blob/master/src/halt.md); batching
stops before the existing wakeup path needs to run.

New differential checks compare APU batches and local scheduling with individual
M-cycles across all ten hardware revisions, audio rates, power transitions,
register writes, speed changes, and divider wrap. PPU batches are compared with
M-cycle stepping through three frames, register changes, sprite latches, DMA
contention, and DMG compatibility modes. Timer tests cover 4,576 combinations
of TAC, DIV phase, TIMA, and step length, plus pending writes/reloads. A complete
machine test compares 96,000 observation boundaries across DMG/CGB, both CPU
speeds, and both IME states, including interrupts, DMA, serial transfers, LCD
power changes, audio, memory, and frame boundaries. These establish equivalence
to the reference implementation; they do not establish perfect hardware accuracy.

Final paired medians versus `2aa334a`, five alternating runs per binary:

| Workload | Measured / warmup frames | Before | After | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Polished Crystal, 48 kHz audio | 1,800 / 300 | 2.519030 s | 1.529420 s | 1.647× |
| Polished Crystal, silent | 1,800 / 300 | 2.429197 s | 1.421808 s | 1.709× |
| DMG acid2, audio output enabled | 600 / 120 | 0.812166 s | 0.821816 s | 0.988× |
| Blargg CPU instructions, CGB, audio output enabled | 600 / 120 | 0.700107 s | 0.707019 s | 0.990× |
| CGB acid2, audio output enabled | 600 / 120 | 0.402688 s | 0.187482 s | 2.148× |
| CGB wave test, audio output enabled | 600 / 120 | 0.337653 s | 0.348968 s | 0.968× |

Every output signature matched. Polished Crystal uses 39.29% less measured
execution time with audio and 41.47% less without it. The benefit is
workload-dependent: DMG acid2 and the CPU test regressed about 1%, and the wave
test about 3.35%. These regressions are retained in the report; the new dispatch
and code layout are not a universal speedup. No frontend frame-rate or 3DS
hardware measurement is implied.

A separate five-run comparison against the original `83f3465` binary measured
4.473945 s versus 1.557065 s for Polished Crystal with audio:
**2.873× cumulative throughput, or 65.20% less execution time**. Audio/video
hashes, sample/dot counts, and final PC matched the original baseline.

The 600/120-frame counter workload retires **5,669,258,570 instructions** and
**1,086,820,574 branches**, down 41.59% and 44.50% from the previous pass.
Instruction count is 70.28% lower than the original baseline. Executable text
is 586,932 bytes, up 3,616 bytes (0.62%); no dependencies, large tables, or unsafe
code were added.

The final exclusive sampling profile attributes 24.3% to APU CPU-tick stepping,
20.0% to the bounded CPU runner (including its inlined APU scheduler/quiet work),
10.6% to PPU stepping, 8.3% to ordinary CPU ticks (also including inlined APU
work), 6.9% to instruction execution, 4.5% to static CGB background spans, 4.1%
to audio sample production, 3.2% to OAM scanning, and 2.2% to timer stepping.
These are symbol sample shares, not exclusive subsystem totals. The APU's
remaining M-cycle visits and deadline overhead are the next substantial target.

Validation for the HALT scheduling pass:

- `cargo fmt --all`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test`: 571 passed, 33 existing ignored tests.
- `cargo test --release`: 567 passed, 33 existing ignored tests.
- `cargo check -p vibe-emu-core --all-features`: passed.
- Gambatte was not run; it remains informational.

## Deferred APU execution across CPU instructions

Baseline for this pass: `3dde5eb`, preserved as `/tmp/vibe-core-pass4`.
A fresh 6,000/300-frame Polished Crystal profile collected 9,292 samples with
no lost samples. Exclusive symbol shares were 20.9% in APU CPU-tick processing,
18.4% in the HALT runner (including inlined APU work), 11.4% in PPU stepping,
6.7% in ordinary CPU tick orchestration, and only 6.4% in instruction execution.
Blargg's CPU workload put instruction execution at 12.8%, versus 19.0% in APU
CPU-tick processing, 16.5% in the PPU, and 14.1% in CPU tick orchestration.
DWARF call-chain unwinding was unreliable in this build, so these are flat
symbol shares, not reconstructed subsystem totals. A separate Polished Crystal
counter run showed 2.30 instructions/cycle and a 1.27% branch-miss rate; neither
proves that the same bottleneck will dominate on ARM11.

This makes a CPU-only JIT a poor first investment at this point. Eliminating
all time attributed to instruction execution would give an illustrative
Amdahl ceiling of only about 1.07–1.15× on these profiles. A JIT that also removes
peripheral work has a different ceiling. In
[GameRoy's implementation account](https://rodrigodd.github.io/2023/09/02/gameroy-jit.html),
lazy device updates made the CPU a much larger share of execution before JIT
compilation became valuable. That sequence motivates the broader scheduling
change here. It is an inference from our measurements, not a prediction of
another emulator's speedup on vibeEmu.

Three changes work together:

- **Defer APU clocks across running instructions.** `Cpu::run_for_dots` owns a
  bounded execution scope. CPU instructions, interrupts, timer, PPU, DMA, and
  memory accesses still execute at their existing boundaries. The APU queues
  contiguous clocks and catches up before APU/PCM/wave-RAM register reads and
  writes, divider resets, speed/domain discontinuities, irregular CPU ticks,
  or return to the caller. The desktop, Android, and benchmark runners use a
  4,096-dot budget. Breakpoints retain the previous instruction runner. Custom
  link endpoints default to instruction polling, preserving host timestamp and
  external-clock handling; the self-contained null endpoint opts into batching.
- **Project channel state between observations.** Waveform edges alone no
  longer force every M-cycle to execute. Square and wave phases and noise state
  advance directly until just before an audio sample or DIV event. The last two
  M-cycles are replayed to reconstruct all three digital sample latches and
  per-call wave flags, including CGB double speed. Trigger, sweep, delayed noise,
  and wave-RAM quirks retain the existing path. Constant-output intervals still
  use the older, cheaper batch path when it reaches farther. Audio sample
  timing, mixing, high-pass filtering, and integer/float arithmetic are unchanged.
- **Advance noise feedback in parallel.** Count prescaler/ripple-counter events
  arithmetically, then evaluate up to 14 wide-mode or six narrow-mode XNOR
  feedback bits per word operation block. Before newly generated feedback can
  reach an input tap, each feedback bit depends only on the original state.
  Both narrow-mode feedback destinations are preserved. The implementation
  derives these blocks directly from the existing recurrence and uses no large
  table. This applies the parallel-state reasoning discussed in the primary
  [LFSR derivation](https://www.moria.us/articles/demystifying-the-lfsr/), while
  adapting it to Game Boy XNOR feedback and narrow mode.

This extends the interaction-boundary approach described by
[mGBA](https://mgba.io/2017/04/30/emulation-accuracy/) and GameRoy. It does not
replace the existing audio resampler with a different approximation. The
single-instruction API remains available, and all pending APU state is
synchronized before the new runner returns. No unsafe code or dependencies
were introduced.

Final paired medians versus `3dde5eb`, five alternating runs per binary:

| Workload | Measured / warmup frames | Before | After | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Polished Crystal, 48 kHz audio | 1,800 / 300 | 1.548085 s | 1.140781 s | 1.357× |
| Polished Crystal, silent | 1,800 / 300 | 1.456231 s | 0.864066 s | 1.685× |
| DMG acid2, audio output enabled | 600 / 120 | 0.808138 s | 0.724710 s | 1.115× |
| Blargg CPU instructions, CGB, audio output enabled | 600 / 120 | 0.716760 s | 0.612913 s | 1.169× |
| CGB acid2, audio output enabled | 600 / 120 | 0.188087 s | 0.180377 s | 1.043× |
| CGB wave test, audio output enabled | 600 / 120 | 0.355490 s | 0.311302 s | 1.142× |

All runs matched video/audio hashes, sample and dot counts, and final PC. The
Polished Crystal gain is **1.357× throughput, or 26.31% less execution time**
with audio. Silent execution benefits more because it can project farther
without sample boundaries. Unlike the preceding pass, all six workloads improved
in this comparison. They still do not constitute representative gameplay coverage.

A separate comparison with original `83f3465` measured 4.513909 s versus
1.144577 s: **3.944× cumulative throughput, or 74.64% less execution time**, with
all output signatures matching. These results use the normal Cargo release
build; the optional compiler experiment below is not included.

The 600/120-frame counter workload retires **4,263,910,091 instructions** and
**823,388,898 branches**: 24.79% and 24.24% fewer than `3dde5eb`. Instruction count
is 77.65% lower than the original baseline. Executable text increased from
586,932 to 592,044 bytes (5,112 bytes, 0.87%). These process-wide counters include
warmup and verification; timed benchmark medians exclude verification.

The final 6,000/300-frame exclusive profile shifts to 12.7% in the local APU
scheduler, 11.4% in PPU stepping, 9.5% in ordinary CPU ticks, 8.3% in instruction
execution, 10.1% combined in the two static CGB span renderers, 4.1% each in OAM
scanning and scanline dispatch, and 3.9% in the remaining APU CPU-tick path.
Audio sample emission accounts for 3.6%. Symbol shares include inlined work;
they should not be summed into precise subsystem totals.

Accuracy checks added for this pass:

- Exhaustive noise feedback comparison: all 32,768 states, both widths, and
  lengths 0–64, totaling 4,259,840 cases against individual transitions.
- Noise clock comparison against the original event loop now covers 180,224
  cases, including long intervals, all NR43 values, and counter phases.
- Deferred-versus-eager APU clocks across three hardware revisions and three
  sample rates, with MMIO reads/writes, wave RAM, power changes, DIV resets,
  speed changes, frozen dividers, irregular ticks, and wrapping counters.
- Whole-machine execution across 6,000 observation boundaries compares CPU,
  memory/APU snapshots, framebuffers, flags, and every stereo sample with the
  single-instruction path. A separate test checks conservative external-link
  polling and zero-budget behavior. Existing ten-revision APU scheduler and
  96,000-boundary HALT/DMA/serial tests also pass.

These differential tests establish equivalence to the existing implementation
for the covered states; they are not proof of perfect hardware emulation.

Validation:

- `cargo fmt --all`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test`: **575 passed**, 33 existing ignored tests.
- `cargo test --release`: **571 passed**, 33 existing ignored tests.
- `cargo check -p vibe-emu-core --all-features`: passed.
- Gambatte was not run; it remains informational.

## Optional profile-guided compiler experiment

[Rust's PGO documentation](https://doc.rust-lang.org/rustc/profile-guided-optimization.html)
describes using execution counts to guide inlining, code layout, and register
allocation. This can improve generated code without changing emulation timing.
`scripts/benchmark_pgo.py` implements an isolated host experiment using matching
`rustc`/`llvm-profdata` tools, a fresh profile directory, and the existing thin
LTO/one-codegen-unit/optimization-level-3 settings. It builds the dependency-free
core example directly, trains on supplied ROMs, merges the profiles, and runs
alternating comparisons with output-signature checks. Compiler identity,
training paths, profiles, binaries, and logs are retained in the output directory.
Python 3.11 or later is required.

```bash
rustup component add llvm-tools
python3 scripts/benchmark_pgo.py polishedcrystal-debug-3.2.3.gbc \
  --train-rom polishedcrystal-debug-3.2.3.gbc \
  --train-rom crates/vibe-emu-core/test_roms/dmg-acid2/dmg-acid2.gb \
  --train-rom crates/vibe-emu-core/test_roms/blargg/cpu_instrs/cpu_instrs.gb \
  --train-rom crates/vibe-emu-core/test_roms/cgb-acid2/cgb-acid2.gbc \
  --training-frames 600 --warmup 300 --frames 1800 --runs 5
```

On the `e78952a` source from the APU pass, five paired runs per workload measured:

| Workload | Without PGO | With PGO | Additional speedup |
| --- | ---: | ---: | ---: |
| Polished Crystal, audio, 1,800/300 frames | 1.145432 s | 0.970691 s | 1.180× |
| Polished Crystal, silent, 1,800/300 frames | 0.859159 s | 0.707503 s | 1.214× |
| CGB wave test, audio output enabled, 600/120 frames | 0.333049 s | 0.258445 s | 1.289× |

Every output signature matched. The Polished Crystal audio result saves 15.26%
of execution time. Its ROM is part of the training corpus; silent output mode
and the wave-test ROM were excluded from training. These limited holdout checks
are encouraging but do not establish generalization to arbitrary gameplay.

This workflow builds only the host profiling executable. It does **not** change
Cargo's default release settings, build a PGO desktop/Android/3DS frontend, or
establish a PGO-specific full-test-suite result. The full debug/release test
results above apply to the ordinary Cargo build. Profiles must be regenerated
after source/compiler changes; use a representative gameplay corpus and validate
the actual target/frontend build before adopting PGO there. No speedup from this
experiment is assumed in the 3DS budget below.

## PPU scheduling and decoded tile rows

Baseline for this pass: `e78952a`, preserved as `/tmp/vibe-core-pass5-nextbase`.
A new 6,000/300-frame Polished Crystal sampling run attributed 11.0% of samples
to PPU stepping, 9.4% to the two static CGB tile-span renderers, 4.1% to OAM
scanning, and 4.0% to scanline dispatch. CPU tick orchestration accounted for
9.5%, and instruction execution for 8.6%. These are exclusive symbol shares,
including inlined work, rather than complete subsystem totals.

The implementation addresses three PPU costs together:

- **Defer native CGB PPU clocks across ordinary instructions.** The MMU caches
  the PPU's existing conservative deadline and accumulates clocks only strictly
  before the next event. It synchronizes before VRAM/OAM/register accesses,
  CPU OAM-corruption operations, DMA, speed-switch stalls, HALT batching, and
  return from the bounded runner. The original CPU tick still executes each
  mode transition, STAT/VBlank interrupt, and HBlank DMA boundary. Mutable VRAM,
  OAM, and PPU APIs remain usable outside the bounded scope, where all state is
  synchronized. DMG and CGB compatibility mode retain eager PPU stepping: their
  dot FIFO provides too few skippable intervals to justify the extra deadline
  maintenance. No rendering or frame delivery is skipped.
- **Reuse decoded CGB tile rows by content.** A 64-entry, 2,816-byte cache stores
  eight resolved pixels and their color-zero flags. Each tag includes both
  fetched bitplanes, palette number, and horizontal flip. Lookup compares the
  entire tag; hash collisions cause misses and cannot produce a false hit.
  Palette-table refreshes invalidate the cache. VRAM banks, vertical flip,
  signed addressing, and blocked rendering are resolved before lookup, so
  direct writes to public VRAM arrays remain coherent without write tracking.
  Tile priority stays separate and is applied on every draw. Full tile rows
  use fixed-size copies; clipped BG/window edges use the appropriate slices.
  Lines with mid-line register changes retain the existing fetcher fallback.
- **Process complete OAM scan pairs together.** Where both phases occur without
  an intervening observer or DMG DMA contention, select the sprite and update
  scan/bus state once per pair. Odd boundaries and contention retain the old
  per-dot loop. The ten-sprite limit and the final bus state still apply after
  the selected-sprite list fills.

This follows the interaction-bounded batching principle described by
[mGBA](https://mgba.io/2017/04/30/emulation-accuracy/) and the lazy-peripheral
approach in [GameRoy](https://rodrigodd.github.io/2023/09/02/gameroy-jit.html).
The access barriers are essential because VRAM/OAM availability changes with
PPU mode, and DMA can alter the PPU's OAM input while drawing; see
[Pan Docs on video-memory access](https://github.com/gbdev/pandocs/blob/master/src/Accessing_VRAM_and_OAM.md)
and [OAM DMA](https://github.com/gbdev/pandocs/blob/master/src/OAM_DMA_Transfer.md).
The cache and scan-pair implementation were derived from vibeEmu's existing
renderer/state machine; no external emulator code was copied.

Scheduling alone improved the introductory Polished Crystal workload by only
about 3% in an initial three-run comparison, because earlier work already
batched its long HALTs. Combining scheduling with row reuse and scan pairs
addresses work that remains during those HALTs too. An initial all-model
scheduler also regressed DMG acid2 by about 3%; the final implementation retains
eager stepping for DMG and compatibility mode.

New validation includes 27,500 whole-machine observation boundaries across DMG
revisions 0/B and CGB revisions 0/C/E, both CGB speeds, and compatibility mode.
The reference executes individual instructions. Comparisons cover CPU and timer
state, dot clocks, interrupts, PPU mode/clocks, VRAM/OAM, memory snapshots,
framebuffers, and every produced stereo sample. The synthetic program combines
video-memory accesses, OAM-corruption instructions, LY/STAT polling, palette and
scroll writes, timer interrupts, pending register writes, LCD power changes,
OAM DMA, GDMA, and HBlank DMA.

Two new PPU differential tests compare 2,048 changing rendering configurations
with the original pixel fetcher and 2,048 OAM scan scenarios with the original
dot loop, including odd phases, sprite-height changes, list saturation, and DMA
contention. The existing 524,288-configuration static-span test also passes.
These checks establish equivalence to the reference implementation for covered
states, not perfect hardware accuracy. No unsafe code or dependencies were added.

Final paired medians versus `e78952a`, five alternating runs per binary:

| Workload | Measured / warmup frames | Before | After | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Polished Crystal, 48 kHz audio | 1,800 / 300 | 1.145188 s | 1.037419 s | 1.104× |
| Polished Crystal, silent | 1,800 / 300 | 0.853975 s | 0.735848 s | 1.161× |
| DMG acid2, audio output enabled | 600 / 120 | 0.714892 s | 0.710895 s | 1.006× |
| Blargg CPU instructions, CGB, audio output enabled | 600 / 120 | 0.629614 s | 0.503413 s | 1.251× |
| CGB acid2, audio output enabled | 600 / 120 | 0.174269 s | 0.150844 s | 1.155× |
| CGB wave test, audio output enabled | 600 / 120 | 0.306229 s | 0.234000 s | 1.309× |

Every run matched video/audio hashes, sample and dot counts, and final PC.
Polished Crystal with audio uses **9.41% less execution time**; silent execution
uses 13.83% less. The CPU and wave workloads benefit more from batching ordinary
instructions. DMG acid2 is effectively unchanged: its 0.56% time reduction is
too small for a strong performance conclusion. Row reuse and the balance of running/HALT time
make results workload-dependent; these are introductory/test-ROM sequences,
not broad gameplay coverage.

A separate five-run comparison with original `83f3465` measured 4.554074 s versus
1.017922 s: **4.474× cumulative throughput, or 77.65% less execution time**,
with every output signature matching. All measurements use the normal Cargo
release configuration, without PGO.

The 600/120-frame counter workload retires **3,847,687,098 instructions** and
**756,900,492 branches**, reductions of 9.76% and 8.07% from `e78952a`.
Instruction count is 79.83% lower than the original baseline. Executable text
increased from 592,044 to 592,748 bytes (704 bytes, 0.12%). The row cache adds
2.75 KiB per PPU instance; no per-frame allocation is introduced.

The final 6,000/300-frame profile attributes 13.5% to the APU scheduler, 11.1%
to CPU ticks, 8.4% to instruction execution, 7.9% to PPU stepping, 4.8% combined
to static CGB span rendering, 4.4% to scanline dispatch, 2.8% to PPU deadline
prediction, and 2.6% to OAM scanning. The new cache's miss decoder accounts for
0.8%. Symbol percentages remain a guide rather than precise subsystem totals. A separate 2,400/120-frame DMG acid2 profile
attributes 55.6% to PPU stepping and 4.5% to static DMG tile spans, showing why
native CGB optimizations are not enough to address the DMG FIFO's cost.

Validation:

- `cargo fmt --all`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test`: **578 passed**, 33 existing ignored tests.
- `cargo test --release`: **574 passed**, 33 existing ignored tests.
- `cargo check -p vibe-emu-core --all-features`: passed.
- Gambatte was not run; it remains informational.

## DMG FIFO projection and shared PPU rendering

Baseline for this pass: `1bab0f0`, preserved as
`/tmp/vibe-core-pass6-nextbase`. A fresh 2,400/120-frame DMG acid2 profile
attributed 56.6% of samples to PPU stepping and another 4.1% to static DMG
tile spans. This is a much larger target for that workload than CPU dispatch.

### Why DMG and CGB had different FIFO paths

Both hardware families have background and object pixel FIFOs. CGB adds
palette and priority metadata and can fetch a tile index and its attributes
simultaneously. It also differs in object-fetch cancellation and LCDC bit
semantics. Those differences justify model-specific rules, but not the idea
that only DMG uses a FIFO. See the hardware descriptions in
[Pan Docs: Pixel FIFO](https://github.com/gbdev/pandocs/blob/master/src/pixel_fifo.md)
and [Rendering](https://github.com/gbdev/pandocs/blob/master/src/Rendering.md).

The implementation has two separate concerns:

| Concern | DMG / CGB compatibility | Native CGB |
| --- | --- | --- |
| Live mode-3 timing | Simplified FIFO occupancy, fetcher phases, pixel pop times, and OBJ fetch stages | Simpler clock-based OBJ attribute-latch schedule |
| Final pixel rendering | Static tile spans or a replay fetcher for raster effects | Static tile spans or a replay fetcher for raster effects |

Thus the old comment about CGB not modeling a FIFO described only its live
timing path and was misleading about rendering. It has been corrected.
Neither model is a complete transistor-level simulation. DMG has more detailed
live modeling in these areas; that does not establish greater accuracy across
all games and hardware behavior. A simpler model can be equivalent for a
given observation, while missing an edge case elsewhere. Code alone does not
establish why its authors chose the split.

This pass shares mechanisms with identical semantics and preserves the
different timing rules. Missing native-CGB fetch timing should be addressed
with hardware-backed regression cases, especially around register writes,
window restarts, and OBJ fetch boundaries. Replacing its timing wholesale with
DMG's would import DMG-specific quirks. The following optimizations preserve
the existing model; they do not claim to close those accuracy gaps.

### Changes and correctness boundaries

- **Project uninterrupted DMG FIFO runs.** Once startup and fine-scroll
  discard finish, the ready fetcher repeats an eight-dot cycle. Calculate
  occupancy and phase directly, retaining every pixel-pop timestamp, palette
  capture, and final sprite-match state. Stop before sprite matches, transfer
  housekeeping, or a FIFO stall. Active OBJ fetches, LCDC changes, DMA overlap,
  delayed register writes, and startup quirks keep the original path. This
  projects the existing live timing model; window/register effects still use
  the same captured timing in the replay renderer.
- **Use the same bounded PPU scheduler for both models.** DMG and compatibility
  mode now expose useful safe intervals. Existing MMU synchronization barriers
  apply before VRAM/OAM/MMIO observations and CPU OAM-corruption operations;
  interrupts, DMA, and frame boundaries retain their original timing.
- **Share decoded rows.** Extend the existing 64-entry cache to static DMG
  spans. Full tags distinguish CGB palette/flip data from DMG BGP mappings;
  both color-table refresh paths invalidate entries. Bitplane decoding and
  row copying are shared, while priority rules remain in their model-specific
  callers. Cache capacity remains 2,816 bytes per PPU.
- **Share fixed FIFO storage.** Both replay fetchers now use the same generic
  ring buffer, retaining different pixel payloads and sequencers. This removes
  CGB's per-dynamic-scanline `VecDeque` allocation. Its push rule permits at
  most 16 queued pixels, within the existing 32-slot storage. The storage size
  is an implementation choice, not a claim about hardware FIFO capacity.

Differential coverage includes 4,480 occupancy/fetcher/scroll/run-length
combinations against original dot stepping, comparing internal timing and all
pop records. Existing mixed-access comparisons now cover all four DMG and six
CGB revisions plus three CGB compatibility configurations, both priority
orders, DMA, mode changes, raster writes, and varied clock chunks. Additional
tests cover FIFO wraparound and saturation and 131,072 alternating cache-tag
cases with palette updates and clipped rows. Existing CPU-runner comparisons
exercise the expanded MMU scheduling scope.

### Measurements and validation

Five alternating pairs per workload, with the same release configuration as
the baseline and no concurrent builds or tests:

| Workload | Measured / warmup frames | Baseline | Optimized | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Polished Crystal, 48 kHz audio | 1,800 / 300 | 1.029635 s | 1.039253 s | 0.991× |
| Polished Crystal, silent | 1,800 / 300 | 0.738028 s | 0.746695 s | 0.988× |
| DMG acid2, audio enabled | 600 / 120 | 0.716671 s | 0.442928 s | **1.618×** |
| Blargg CPU instructions, CGB, audio enabled | 600 / 120 | 0.497086 s | 0.502383 s | 0.989× |
| CGB acid2, audio enabled | 600 / 120 | 0.149999 s | 0.155514 s | 0.965× |
| Blargg CGB wave test, audio enabled | 600 / 120 | 0.233122 s | 0.236607 s | 0.985× |
| CGB acid-hell, audio enabled | 600 / 120 | 0.600337 s | 0.579010 s | 1.037× |

All video/audio hashes, sample counts, dot counts, and final PCs matched.
DMG acid2 saves **38.2%** of execution time. This is not an across-the-board
CGB improvement: Polished Crystal is about 1% slower and static CGB acid2
about 3.7% slower, while the dynamic CGB acid-hell workload improves 3.7%.
The changes are retained for the substantial DMG gain and shared machinery;
these CGB costs remain visible rather than being averaged away. An experiment
forcing the shared row-copy helper inline did not improve the Polished Crystal
or DMG result and was not retained.

For 600 measured plus 120 warmup frames, DMG acid2's retired user instructions
fell from **9,684,748,790 to 5,497,285,404** (43.2%), and branches from
1,879,990,385 to 1,014,361,869 (46.0%). Polished Crystal instead changes from
3,847,686,518 to 3,853,066,165 instructions (+0.14%) and from 756,900,316 to
759,521,544 branches (+0.35%). These process counters include initialization,
warmup, checksums, and queue draining. Five fresh pairs against the original
`83f3465` binary put the cumulative Polished Crystal improvement at **4.368×**
(4.501492 s to 1.030618 s). Executable text falls from 592,748 to 592,060 bytes;
combined text/data/BSS remains 617,402 bytes. No extra per-PPU cache is added.

The final DMG profile places 39.2% in PPU stepping, 5.1% in FIFO projection,
3.5% in its run-limit calculation, and 1.5% in static tile spans. This points
to remaining dot-path boundaries and guards rather than more tile decoding.
The final Polished Crystal profile places 13.0% in the APU scheduler, 9.7% in
CPU tick orchestration, 8.4% in PPU stepping, and 7.7% in instruction execution.
These are exclusive symbol shares, not subsystem totals; changing inlining
also changes attribution. Neither final profile lost samples.

Validation:

- `cargo fmt --all`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test`: **581 passed**, 33 existing ignored tests.
- `cargo test --release`: **577 passed**, 33 existing ignored tests.
- `cargo check -p vibe-emu-core --all-features`: passed.
- Gambatte was not run; it remains informational.

## APU pipeline reconstruction and exact sample deadlines

Baseline for this pass: `33261cc`, preserved as `/tmp/vibe-core-pass8-base`.
A fresh 6,000/300-frame Polished Crystal profile attributed 12.7% of samples
to the APU batch scheduler, 11.3% to CPU tick orchestration, and 4.2% to the
APU's ordinary tick path. Disassembly sampling also put 13.7% of the scheduler's
local samples immediately after its sample-deadline division. This is a clue
to instruction latency, not a precise measurement of division cost.

The earlier scheduler projected waveform counters through intervals without
observable events, then replayed two complete CPU M-cycles to restore pipeline
state. Those replay calls repeated sample-clock updates, DIV checks, and
deadline prediction despite the enclosing interval already excluding them.

This pass reconstructs the final pipeline directly. Only three sample latches
survive: normal speed replaces all three in the final M-cycle; double speed
retains one penultimate value and two final values. Advance waveform counters
to one M-cycle before the end, capture that output where needed, then advance
the final M-cycle and fill/shift the latches. The final separate channel step
also preserves wave-RAM access flags and per-call channel bookkeeping. Clock
and sample-accumulator bookkeeping is combined once. Normal and double speed
use compile-time specializations selected once per queued batch.

The interval's existing guards remain in force: no audio sample, DIV edge,
sweep event, register observation, pending wave-RAM effect, or restart quirk
may be crossed by this reconstruction. PCM reads and register accesses still
synchronize queued clocks. This matters because CGB PCM registers expose
channel outputs and DIV writes can trigger the sequencer, as documented in
[Pan Docs: Audio Details](https://github.com/gbdev/pandocs/blob/master/src/Audio_details.md).
No mixer, filter, resampling policy, or hardware timing rules change.

Sample deadlines now use a precomputed reciprocal for the runtime sample rate,
following the general technique of replacing repeated division by a fixed
runtime denominator with multiply/shift operations described by
[libdivide](https://libdivide.com/). This implementation adds no dependency or
copied library code. For numerator `n < 2^32` and rate `d >= 2`,
`q = (n * floor(2^32 / d)) >> 32` is at most one below `floor(n / d)`.
The remainder comparison `n - q*d >= d` supplies the exact correction. Rates
zero and one are handled explicitly; changing or restoring the rate updates
the reciprocal. There is no approximate sample timing or floating-point math.

New tests check all 4,194,304 accumulator phases at each of 44.1, 48, and 96 kHz,
plus boundary and deterministic sampled phases across 1,030 rates, including
zero, one, rates above the emulated clock, and `u32::MAX`. Another test compares
projected channel state and emitted samples against individual M-cycles across
15,360 intervals, normal/double speed, DMG and CGB revisions C/E, low/high
waveform frequencies, noise configurations, DIV boundaries, and counter wrap.
The existing all-revision scheduling and MMIO differential tests also pass.

Five alternating pairs per workload, using the same ordinary release settings
and with no concurrent compilation or tests:

| Workload | Measured / warmup frames | Baseline | Optimized | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Polished Crystal, 48 kHz audio | 1,800 / 300 | 1.036096 s | 0.980635 s | **1.057×** |
| Polished Crystal, silent | 1,800 / 300 | 0.742323 s | 0.740750 s | 1.002× |
| DMG acid2, audio enabled | 600 / 120 | 0.446877 s | 0.458151 s | 0.975× |
| Blargg CPU instructions, CGB, audio enabled | 600 / 120 | 0.501450 s | 0.503331 s | 0.996× |
| CGB acid2, audio enabled | 600 / 120 | 0.156387 s | 0.158203 s | 0.989× |
| Blargg CGB wave test, audio enabled | 600 / 120 | 0.237804 s | 0.240840 s | 0.987× |
| CGB acid-hell, audio enabled | 600 / 120 | 0.578630 s | 0.588159 s | 0.984× |

Every video/audio hash, sample count, dot count, and final PC matched. The
active-audio Polished Crystal workload saves **5.35%** of execution time;
silent Polished Crystal is essentially unchanged. The test-ROM workloads
regress by 0.4–2.5%, with DMG acid2 the largest regression. These are retained
and reported as a tradeoff for the active-audio gain, not a claim of universal
improvement. The test-ROM measurements emit silent audio, so broader games
with active sound remain important holdouts before generalizing this result.

For 600 measured plus 120 warmup frames, Polished Crystal's retired user
instructions fall from **3,853,065,312 to 3,684,248,557** (4.38%), and branches
from 759,521,309 to 738,921,355 (2.71%). DMG acid2 instructions fall from
5,497,286,443 to 5,299,038,136 (3.61%), and branches from 1,014,362,028 to
979,686,536 (3.42%), despite its elapsed-time regression. Fewer instructions
alone do not establish better execution time, especially across architectures.
Counters include startup, warmup, hashing, and queue draining.

Five fresh pairs against the original `83f3465` binary give a cumulative
Polished Crystal speedup of **4.568×** (4.471525 s to 0.978776 s). Executable
text grows from 592,060 to 595,100 bytes (3,040 bytes, 0.51%); total
text/data/BSS grows from 617,402 to 621,498 bytes. The APU stores one cached
32-bit reciprocal; there are no new allocations, tables, or dependencies.

The final Polished Crystal profile places 10.5% in CPU ticks, 9.5% in PPU
stepping, 8.1% in instruction execution, and 3.3% in timer stepping. The
specialized double-speed APU loop takes 5.1%, with another 4.0% in its now
outlined constant-output deadline and 2.3% in the unobserved-interval deadline.
Do not compare the loop's 5.1% alone to the old inlined scheduler's 12.7%.
DMG still spends 41.0% in PPU stepping, 5.9% in FIFO projection, and 2.9% in
its run-limit calculation. Both final profiles report zero lost samples.

Validation:

- `cargo fmt --all`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test`: **583 passed**, 33 existing ignored tests.
- `cargo test --release`: **579 passed**, 33 existing ignored tests.
- `cargo check -p vibe-emu-core --all-features`: passed.
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
emulated seconds. Its latest 3.684 billion x86-64 instructions correspond to
**0.306 billion host instructions per emulated second**, or 5.117 million
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
| Optimistic | 1.0 | 1.0 | 90% | 1.27× | 0.42× |
| Working planning assumption | 1.5 | 1.5 | 80% | 3.21× | 1.07× |
| More costly ARM execution | 2.0 | 2.0 | 75% | 6.08× | 2.03× |

A 1.07× requirement means roughly 94% of real time under that scenario; the old
3DS estimate is about 31%. A value below 1 in the optimistic scenario means
headroom under that assumption, not a measured result. The working interpretation
is **another ~1.07× improvement for New 3DS and ~3.21× for old 3DS**, reduced from
1.12× and 3.35× before the APU pipeline pass. These remain development budgets,
not a claim that either machine has achieved those speeds. The optional PGO experiment is
not credited in this calculation.

This workload remains below the earlier provisional hardware-testing trigger
of **0.43 billion host instructions per emulated second** (about 7.2 million per
frame), corresponding to a working New 3DS requirement within 1.5× of full speed.
Hardware testing remains deferred as requested. The more useful next estimate
improvement is an ARM build and assembly analysis, plus broader gameplay traces.
A 1.0× working budget is ~0.286 billion instructions/s, about 6.5% fewer than the
current count. No ARM binary or physical console was measured in that pass.
The subsequent [ARM assembly audit](NEW_3DS_RESEARCH.md#actual-target-assembly)
successfully cross-compiled the core library, but has not calibrated dynamic
ARM instruction counts or console execution time.

The DMG workload must be assessed separately. Its latest counter rate is
**0.440 billion instructions per emulated second**, or 7.360 million per frame.
Under the same working assumptions, DMG acid2 still needs **1.54× on New 3DS
and 4.61× on old 3DS**. Its instruction proxy improves in this pass while its
desktop time regresses, illustrating the limits of this estimate. It remains
above the provisional hardware-testing trigger despite the preceding FIFO
pass's large improvement. These are different ROMs and
execution paths, so their rates do not establish a hardware-model accuracy
ranking or predict arbitrary gameplay.

## Research and next opportunities

The latest profile separates two priorities:

1. **CGB: reduce peripheral clock updates and remaining APU work.**
   Polished Crystal spends 10.5% of samples in CPU tick orchestration and 3.3%
   in timer stepping. APU scheduling also remains significant: 5.1% in the
   double-speed loop plus 6.3% in its two deadline helpers. Investigate combining
   timer/RTC and other peripheral advances across ordinary instructions, stopping before interrupt
   deadlines and synchronizing on relevant bus accesses. The current PPU/APU
   scopes provide part of this foundation, but timer overflow/reload collisions,
   serial transfers, and DMA must retain their timing. Sample-deadline division
   is now removed; reusing the overlapping DIV/noise eligibility checks is a
   more relevant APU target. Measure those changes before broadening dispatch
   or adding a CPU JIT; instruction
   execution itself still accounts for only 8.1% of this profile.
2. **DMG: reduce repeated work at FIFO boundaries.** Stable FIFO runs are now
   projected, but PPU stepping still takes 41.0% of samples, with another 8.8%
   in projection and run-limit calculation. Investigate splitting calls at safe
   boundaries inside `step_inner`, so an interval that starts or ends in a
   quirk need not keep its entire middle on the dot path. Avoid recomputing the
   same run limit in deadline prediction and advancement where possible. Preserve
   the per-pixel timestamps needed by raster replay, and extend the original
   dot-path comparisons to any newly batched startup or sprite phases.

Before architecture-specific changes, build for ARM and inspect division,
64-bit arithmetic, spills, and instruction-cache footprint. Rust documents the
[`armv6k-nintendo-3ds` target and devkitARM/build-std requirements](https://doc.rust-lang.org/rustc/platform-support/armv6k-nintendo-3ds.html).
This can sharpen the current instruction-expansion assumptions without hardware
access. Broaden deterministic gameplay workloads before selecting training
profiles or claiming general 3DS readiness. The PGO measurements above apply
to `e78952a`; [fresh profiles and comparisons](NEW_3DS_RESEARCH.md#refreshed-pgo-measurements)
now cover `f0f47e4`, with 1.15–1.46× host speedups across seven workloads.
These host gains remain excluded from the 3DS instruction budget.

Avoid large tables or aggressive inlining without measuring their cache cost.
For example, an earlier branch avoiding division on single waveform edges
regressed the desktop workload and was not retained.
