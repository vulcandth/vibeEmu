# New 3DS performance research

Research pass against `f0f47e4`, September 19, 2026. This pass changes
documentation only. It adds an actual ARM assembly audit and refreshes the
optional host PGO experiment; it does not enable PGO in release builds.

## What the remaining gap really means

The previous working estimate needs **1.07× for Polished Crystal's CGB intro
and 1.54× for DMG acid2** on one New 3DS application core. Those correspond
to about **6.5% and 35% less work**, respectively. Neither number is measured
on a console. The ARM/x86 instruction ratio, ARM CPI, and 80% core budget are
assumptions; a more expensive ARM scenario puts the same CGB workload at
2.03× required speedup. See [the calculation](PERFORMANCE.md#rough-3ds-budget-without-hardware-testing).

There is now evidence for specific ARM costs that the desktop profiles hide.
The best next implementation is **moving immutable PPU configuration out of
hot `OnceLock` accesses**, followed by **caching cartridge bank mappings**.
Both remove work without relaxing emulation timing. PGO remains a substantial
additional opportunity, but its host speedup cannot be credited directly to
the 3DS budget. DMG also needs a larger reduction in FIFO scheduling work.

## Actual target assembly

Installed a separate nightly with `rust-src`; the default stable compiler is
unchanged. This command successfully built the core library for Nintendo 3DS:

```bash
rustup toolchain install nightly --profile minimal --component rust-src
cargo +nightly rustc -p vibe-emu-core --lib --release \
  --target armv6k-nintendo-3ds -Z build-std=std,panic_abort \
  --target-dir /tmp/vibe-3ds-assembly -- --emit=asm
rg --files /tmp/vibe-3ds-assembly | rg 'vibe_emu_core-.*\.s$'
```

Compiler: `rustc 1.100.0-nightly (feaadeeac 2026-09-19)`, LLVM 23.1.1.
The target uses ARM11 MPCore, ARMv6K, VFPv2 and hard-float ABI, as specified
in [Rust's target definition](https://github.com/rust-lang/rust/blob/master/compiler/rustc_target/src/spec/targets/armv6k_nintendo_3ds.rs).
This produced an rlib and assembly, **not a linked console executable**.
It establishes code-generation hazards, not runtime cycles, executable size,
or an ARM/x86 dynamic instruction ratio. Final linking/LTO can change code.
Rust's [3DS target documentation](https://doc.rust-lang.org/rustc/platform-support/armv6k-nintendo-3ds.html)
describes the extra SDK and support libraries needed for executable linking.

Static sites in the emitted function bodies:

| Function | ARM memory barrier sites | Software division calls |
| --- | ---: | --- |
| `Ppu::step_inner::<true>` | 10 | 0 |
| `Ppu::render_scanline` | 21 | 0 |
| `Ppu::dmg_sprite_match_x` | 1 | 0 |
| `Ppu::advance_dmg_fifo_run` | 1 | 0 |
| `Ppu::dmg_bg_en_for_pixel` | 7 | 0 |
| `Ppu::dmg_bg_color_for_pixel` | 15 | 0 |
| `Ppu::dmg_lcdc_for_bg_fetch_t` | 7 | 0 |
| `Cartridge::read_with_open_bus` | 0 | 5 × `__aeabi_uidivmod` |
| Each `Apu::run_machine_cycles_at_speed` specialization | 0 | 4 × `__aeabi_idivmod` |
| `Apu::clock_noise_regular_batch` | 0 | `__aeabi_idiv`, `__aeabi_idivmod` |
| `Apu::emit_audio_samples` | 2 | 0 |

These are **static sites**, not counts per invocation. Branches select paths,
loops repeat sites, and called functions have their own costs. The barriers
are `mcr p15, #0, <register>, c7, c10, #5`; the division counts exclude memory
copy/clear helpers. To inspect the relevant sites:

```bash
rg -n 'mcr.*c7, c10, #5|bl[[:space:]]+__aeabi_[ui]div' /path/to/vibe_emu_core-HASH.s
```

For example, `dmg_sprite_match_x` loads the `DmgObjSizeTuning` initialization
state and executes that barrier before testing whether initialization is
needed. Initialization being complete does not remove the recurring barrier.
The [Rust atomic memory model discussion](https://doc.rust-lang.org/nomicon/atomics.html)
explains why acquire/release operations can be cheap on x86 and materially
different on weakly ordered ARM. The generated code confirms that distinction
in this core. ARM's [MPCore technical reference manual](https://documentation-service.arm.com/static/5e8e1e0388295d1e18d368b2)
is the architecture reference; no fixed barrier latency is assumed here.

## Ranked implementation opportunities

### 1. Capture immutable PPU tuning once

`ppu.rs` defines many process-wide environment settings through `OnceLock`.
Their getters appear in sprite matching, FIFO run advancement, pixel replay,
scanline rendering, and mode transitions. Capturing the initialized settings
in a PPU-owned configuration value or reference lets ordinary loads replace
repeated synchronization. Start with `DmgObjSizeTuning`, then cover the other
non-trace timing settings. Keep the hot subset small to avoid replacing a
barrier problem with extra cache traffic.

Preserve all defaults and quirks; this is not an opportunity to delete timing
options. Define configuration capture at construction/reset explicitly, since
the current settings initialize on first use. Preserve process-wide semantics
where required, and test non-default settings in separate processes. Dynamic
log-sink installation is a different API and must not be frozen accidentally.

Acceptance: relevant ARM hot paths lose their tuning-related barrier sites;
default and non-default configurations retain dot-path/FIFO equivalence,
register-write timing, video hashes, and ROM test results. Desktop timing may
understate the value. **No percentage gain is measured for this change yet.**

### 2. Resolve cartridge banks on bank changes

`Cartridge::read_with_open_bus` computes `bank % rom_bank_count` on ordinary
ROM reads. The ARM build lowers these operations to software division. Cache
the effective lower/upper ROM-window offsets when mapper registers change,
leaving a bounds-checked indexed read and existing bus-latch update in the
read path. A power-of-two mask with an exact irregular-size fallback is a
smaller first step; moving the calculation to infrequent writes removes more
repeated work.

Account for MBC1 mode/multicart remapping, MBC2/3 bank-zero rules, MBC30/5,
missing/truncated bytes, open bus, and RAM/RTC selection. `rom` is publicly
mutable today: a cached mapping must detect length changes or introduce a
deliberate API transition. Loading/restoring state must also rebuild caches.
Keep mapping logic centralized and compare every address across randomized
mapper writes and irregular ROM sizes against the current implementation.

Acceptance: ROM reads stop calling `__aeabi_uidivmod`, mapper differential
tests pass, and CPU-heavy workloads improve. The old desktop profile's 2.5%
cartridge share is **not an ARM upper bound**, because ARM uses different code.

### 3. Finish DMG interval scheduling

DMG acid2 still spends 41.0% of desktop samples in PPU stepping, another 5.9%
in FIFO projection, and 2.9% calculating run limits. Split a requested interval
at its next real boundary, advance the stable middle in bulk, and reuse the
already computed boundary when entering advancement. A startup/sprite quirk
at one end should not force the entire interval through the dot loop.

Preserve fetcher stalls, same-X sprite behavior, window starts, pixel timestamps,
and raster register replay. Extend existing dot-versus-batch comparisons over
every offset around these transitions and randomized MMIO sequences. Shared
configuration and boundary machinery can help both models; merging hardware
timing state machines indiscriminately does not establish greater accuracy.

For scale only, halving the combined 49.8% desktop region would produce
`1 / (1 - 0.498 / 2) = 1.33×`, short of the working 1.54× DMG target by itself.
Eliminating 70% of that region gives about 1.54×. These are Amdahl scenarios,
not predicted gains, and overlap with opportunity 1.

### 4. Extend exact reciprocal division to channel periods

The last pass removed sample-deadline division, but square/wave edge batching
and regular noise batching still generate software division on ARM. Cache a
small exact divider when the relevant period changes, using multiply/shift
and correction as appropriate. [libdivide](https://libdivide.com/) describes
the runtime-constant divisor technique; its advertised gains are not estimates
for this emulator. No approximate audio or timing math is necessary.

Use the period actually latched by the channel, not merely the latest frequency
register. Cover sweeps, triggers, reloads, signed intermediates, and state
restoration. Exhaustively compare bounded quotient/remainder domains, then
compare channel state, PCM reads, and audio against individual M-cycles. An
earlier single-edge shortcut regressed x86; target assembly gives a reason to
revisit the cost model, not a reason to assume the regression is harmless.

### 5. Share peripheral deadlines and use bounded local clocks

CPU tick orchestration is 10.5% of the CGB desktop profile and timer stepping
another 3.3%. Advance timer/RTC and other eligible peripherals to the next
observable event rather than updating all of them at every M-cycle. Keep a
bounded 32-bit local elapsed counter within a run and reconcile absolute
64-bit clocks at required observation points. This also targets ARM's cost
of manipulating wide counters.

[mGBA's timing implementation](https://github.com/mgba-emu/mgba/blob/master/src/core/timing.c)
provides a concrete reference for relative cycles, next-event deadlines, and
ordered events. For vibeEmu's small fixed peripheral set, cached minimum
deadlines may be cheaper than a general allocated event queue.

Synchronize before DIV/TIMA/TAC access, timer overflow/reload collisions,
interrupt polling, RTC reads/latches, serial edges, DMA interactions, debug
observations, and public stepping returns. Keep a simple reference path for
differential tests. Halving just the 13.8% desktop tick/timer share would mean
1.074× overall; this is an illustrative bound on that scenario, not an ARM
prediction or a gain additive to PGO.

### 6. Batch audio transport without reducing sample quality

Successful stereo queue pushes and pops each require acquire/release
synchronization. At 48 kHz that is approximately 192,000 barrier executions
per second across the two endpoints, before occupancy polls. Publishing and
consuming blocks of 64 frames could reduce the block coordination component
to roughly 3,000/s, adding up to 1.33 ms of staging latency. This does **not**
mean a 98% reduction in total audio cost or emulator time.

Retain synchronization and exact samples. Flush partial blocks at observable
boundaries and explicitly preserve or document queue-full/drop-newest behavior.
First enforce the single-producer/single-consumer ownership contract: current
endpoints are `Clone`, share `UnsafeCell` storage, and expose operations through
`&self`, so the type permits callers to violate that assumption. A block API
must address this rather than simply weakening atomic orderings. Validate
wraparound, full/empty cases, partial publication, and concurrent operation.

## Refreshed PGO measurements

Stable `rustc 1.98.1`, x86-64 i7-4790, `opt-level=3`, one codegen unit, thin
LTO for both binaries. Training used Polished Crystal, DMG acid2, Blargg CPU
instructions, and CGB acid2, each with 300 warmup plus 600 training frames.
Five alternating baseline/candidate pairs per workload, no concurrent builds
or tests. Times exclude hashing and queue draining as in the existing runner.

| Workload | Measured / warmup frames | Control | PGO | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Polished Crystal, 48 kHz audio | 1,800 / 300 | 1.001059 s | 0.806293 s | 1.242× |
| Polished Crystal, silent | 1,800 / 300 | 0.756394 s | 0.602817 s | 1.255× |
| DMG acid2 | 600 / 120 | 0.466573 s | 0.368213 s | 1.267× |
| Blargg CPU instructions | 600 / 120 | 0.529879 s | 0.363803 s | 1.456× |
| CGB acid2 | 600 / 120 | 0.157703 s | 0.136995 s | 1.151× |
| Blargg CGB `12-wave` (untrained ROM) | 600 / 120 | 0.242164 s | 0.189422 s | 1.278× |
| CGB acid-hell (untrained ROM) | 600 / 120 | 0.592069 s | 0.514860 s | 1.150× |

All model/dot counts, frame/video hashes, audio hashes, sample counts, and final
PCs matched. Time reductions range from 13.0% to 31.3%; Polished Crystal saves
19.5%. This supports retaining PGO as a release-engineering opportunity.
It does not establish gameplay generality: the two untrained ROMs are test
workloads, and the Polished sequence is still introductory. CPU, DMG and CGB
acid2 were part of training. Broader deterministic gameplay recordings are
needed, including active sound and sprite/window-heavy scenes.

Reproduce the primary comparison:

```bash
python3 scripts/benchmark_pgo.py polishedcrystal-debug-3.2.3.gbc \
  --train-rom polishedcrystal-debug-3.2.3.gbc \
  --train-rom crates/vibe-emu-core/test_roms/dmg-acid2/dmg-acid2.gb \
  --train-rom crates/vibe-emu-core/test_roms/blargg/cpu_instrs/cpu_instrs.gb \
  --train-rom crates/vibe-emu-core/test_roms/cgb-acid2/cgb-acid2.gbc \
  --training-frames 600 --warmup 300 --frames 1800 --runs 5 \
  --output /tmp/vibe-pass9-pgo
```

Use `scripts/benchmark_core.py` with the generated `control/profile_core` and
`optimized/profile_core` for the additional ROMs and silent mode.
[Rust's PGO documentation](https://doc.rust-lang.org/rustc/profile-guided-optimization.html)
explains the instrumentation/training/use pipeline. These profiles and timings
are host-specific; verify profile applicability and target code generation
before adopting an ARM build. Multiplying the previous instruction budget by
the desktop elapsed-time improvement would mix unlike measurements.

## Sequence and stopping criteria

Implement configuration capture and bank caching as separate, reviewable
changes; inspect their ARM output as well as their host benchmarks. Then
address DMG interval boundaries and remaining APU divisions, followed by the
more invasive shared scheduler. Regenerate PGO only after these changes settle
and evaluate it against the same final source without PGO.

For each core change, require full formatting, strict workspace Clippy, debug
and release tests, relevant differential tests, and alternating benchmarks.
Keep output hashes and sample counts exact. Hardware testing remains deferred;
assembly audits and broader workloads can improve prioritization in the meantime.

The eventual frontend must request New 3DS speed/cache mode using
[`osSetSpeedupEnable`](https://github.com/devkitPro/libctru/blob/master/libctru/source/os.c).
Core benchmarks exclude presentation, audio backend, and OS costs, so retain
budget for those. The target has VFPv2, not NEON; desktop SIMD proposals do not
transfer directly. A CPU-dispatch-only rewrite targets just 8.1% of the latest
CGB desktop profile (an impossible complete removal would give only 1.09×).
A larger JIT would also need to remove bus/scheduler work to justify its scope.

## Artifacts and validation

Local artifacts from this run (temporary, not committed):

- `/tmp/vibe-3ds-assembly.log`: successful target library build.
- `/tmp/vibe-pass9-arm-audit.json`: function symbols and static site counts.
- `/tmp/vibe-pass9-pgo/run-xp9zi6xi/`: compiler version, training corpus,
  profiles, binaries, and seven comparison logs.
- `/tmp/vibe-pass9-cargo-test.log`: workspace debug test run.

No Rust source, build configuration, or dependencies changed in this research
pass. `cargo test` passed: **583 passed, 0 failed, 33 existing ignored tests**.
`git diff --check` also passed. Benchmark equivalence applies to the PGO experiment; workspace tests
exercise the ordinary debug build, not a PGO-specific build. Gambatte remains
informational and was not run.
