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

A final audio-enabled core profile still places about 45% of samples in APU
stepping, staging, and wave updates, versus about 5% in CPU instruction
execution. This makes the APU deadline work below the next measured priority.

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

## Further work toward 3DS

These host measurements do not establish full speed on either 3DS model.
Measure the same ROM/input sequence on hardware, including presentation and
audio submission, before drawing that conclusion.

1. **APU event deadlines and lazy synchronization.** Stop repeatedly visiting
   channels whose waveform, envelope, and pipeline cannot yet change. Bound
   each batch by waveform edges, pipeline settlement, sample output, DIV edges,
   sweep transitions, and pending wave-RAM commits. Synchronize before register
   reads/writes and speed changes. Keep the present path as a differential
   oracle. mGBA describes batching with splits at concurrent interactions as a
   way to retain cycle accuracy. [mGBA's design discussion](https://mgba.io/2017/04/30/emulation-accuracy/)
2. **Predict interrupts before batching HALT or CPU work.** GameRoy describes
   lazy peripheral updates and executing compiled blocks only when the next
   interrupt cannot occur inside them, with an interpreter fallback. Apply
   conservative deadlines first; a CPU-only JIT has limited upside while the
   APU/PPU dominate this workload. DMA, timer reloads, STAT edges, serial, and
   memory-mapped writes must invalidate the relevant deadlines.
   [GameRoy's implementation discussion](https://rodrigodd.github.io/2023/09/02/gameroy-jit.html)
3. **Measure the ARM build and frontend separately.** Rust documents the
   `armv6k-nintendo-3ds` target and its devkitARM/build-std requirements. Inspect
   generated ARM code and measure word-sized staging, variable divisions,
   code size, and cache behavior before choosing further specializations.
   [Rust target documentation](https://doc.rust-lang.org/rustc/platform-support/armv6k-nintendo-3ds.html)
   The frontend should record whether New 3DS speedup is enabled through
   libctru's `osSetSpeedupEnable`; this belongs outside the portable core.
   [libctru API](https://github.com/devkitPro/libctru/blob/master/libctru/include/3ds/os.h)

Avoid inferring ARM performance from x86 instruction costs. For example, an
experimental branch that avoided division on single waveform edges regressed
the desktop workload and was not retained. Likewise, larger lookup tables or
aggressive inlining need target measurements before accepting their cache cost.
