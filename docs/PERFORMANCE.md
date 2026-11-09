# Core performance and New 3DS readiness

Updated 2026-11-09. Core checkpoint: `1fab122`; the documentation/tooling
follow-up does not change emulation. Host optimization is paused pending
New Nintendo 3DS/2DS access. The limited benchmark corpus meets our provisional
host instruction-budget gate, but **console speed is unverified**.

To resume, use [the workflow guide](PERFORMANCE_WORKFLOW.md). It contains
baseline/build commands, the fixed corpus, gameplay inputs, profiling,
accuracy invariants, validation and handoff steps. Superseded pass-by-pass
logs and `NEW_3DS_RESEARCH.md` were consolidated here; historical evidence
remains in Git history rather than competing with current guidance.

## What changed

- CPU execution shares safe device deadlines across ordinary instructions.
  Observable bus accesses flush queued work; fresh PPU deadlines can be
  established after an access only when no dots remain queued. Timer, IRQ,
  serial, DMA, STOP/HALT and frame boundaries retain their original ordering.
- PPU work advances through proven intervals, with timed CGB background and
  window tile transactions. Stable OBJ-size captures avoid repeated work;
  DMG/CGB quirks, pending writes, fetch timestamps and raster effects retain
  independent original-path references in tests.
- Timer/RTC and audio updates share scheduling intervals. Exact sample timing,
  floating-point filter recurrence, stereo output and PCM rounding are
  preserved. Filters skip arithmetic only at an exact settled fixed point.
- Ordinary ROM/WRAM/HRAM reads use a smaller dispatcher; DMA, boot overlays,
  bus latches, watchpoints and side-effectful reads retain their behavior.
- CPU hot paths are inlined into the bounded runner while boundary handling
  stays separate. This helps desktop measurements but expands ARM code size.
- Filesystem slot storage and `tempfile` are behind the default-enabled
  `state-store` feature. The complete core and buffer save/load APIs remain
  available with `--no-default-features`, allowing the ARM core to compile.

No reduced-accuracy mode, frame skipping, lower sample rate or save-schema
change is introduced. Transient caches and queues are flushed/reconstructed
at observations and save boundaries. Platform-specific frontend work remains
outside the core.

## Whole-branch comparison against main

Direct measurement: fetched `origin/main` at `b7938e4` → core checkpoint
`1fab122`. Same host/compiler/build and workload policy as the final-pass
table below. Seven alternating pairs per case; all 112 timing signatures and
16 counter signatures match. The original seven-case geometric mean is
**1.757× throughput (75.7% faster)**. Synthetic window is excluded
from that mean. These are measured endpoints, not multiplied pass gains.

| Workload | Main baseline | Candidate | Speedup |
| --- | ---: | ---: | ---: |
| polished | 1.115590 s | 0.850798 s | 1.311× |
| silent | 0.876660 s | 0.661750 s | 1.325× |
| dmg | 0.562251 s | 0.295610 s | 1.902× |
| cpu | 0.577203 s | 0.304413 s | 1.896× |
| cgb | 0.190942 s | 0.119232 s | 1.601× |
| wave | 0.280939 s | 0.137335 s | 2.046× |
| hell | 0.775748 s | 0.307569 s | 2.522× |
| window | 0.907769 s | 0.314455 s | 2.887× |

No case in this whole-branch comparison regresses. The final-pass table below
keeps its smaller local regression visible. Timings are core-only Linux
measurements; hardware and frontend validation remain separate.

## Final scheduling pass

Direct comparison: `885cf94` → `1fab122`. Linux x86-64 Intel Core i7-4790,
Rust 1.98.1, opt-level 3, thin LTO, one codegen unit, same pinned dependencies,
no native-CPU flags or PGO. Seven alternating pairs per workload, no concurrent
builds/tests. Polished uses 1800 measured / 300 warmup frames; the remaining
cases use 600/120. Audio is 48 kHz except silent. Values are medians.

| Workload | Baseline | Candidate | Speedup | Fewer instructions | Remaining 3DS factor |
| --- | ---: | ---: | ---: | ---: | ---: |
| polished | 0.875429 s | 0.803402 s | 1.090× | 1.50% | 0.93× |
| silent | 0.668755 s | 0.640859 s | 1.044× | 1.93% | 0.72× |
| dmg | 0.302191 s | 0.300274 s | 1.006× | -1.51% | 0.97× |
| cpu | 0.343741 s | 0.297084 s | 1.157× | 11.28% | 0.97× |
| cgb | 0.110430 s | 0.113214 s | 0.975× | -2.96% | 0.44× |
| wave | 0.163904 s | 0.134016 s | 1.223× | 13.24% | 0.53× |
| hell | 0.348214 s | 0.308221 s | 1.130× | 11.88% | 1.05× |
| window | 0.361725 s | 0.315888 s | 1.145× | 15.25% | 1.07× |

Original seven-case geometric-mean throughput improves **8.63%**; synthetic
window is excluded. CGB acid2 takes 2.52% longer and executes 2.96% more
instructions. DMG acid2 executes 1.51% more instructions with almost unchanged
elapsed time. All 112 timing outputs and 16 counter outputs have exact matching
model, dots, video/audio hashes, sample counts and final PCs.

The current process instruction counts for 600 measured + 120 warmup frames,
in table order, are 3,217,118,667 / 2,495,014,712 / 3,357,366,511 /
3,358,328,530 / 1,518,226,364 / 1,842,526,360 / 3,609,467,049 /
3,685,434,508. These include startup, warmup, hashing and draining, unlike the
elapsed core-only timing region. Never multiply separate pass medians to
claim a cumulative speedup.

## Gameplay evidence

The Polished Crystal 3.2.3 indoor movement sequence is screenshot-verified and
now preserved as `examples/profile_polished_room.rs` in the core crate. The
[workflow](PERFORMANCE_WORKFLOW.md#repeatable-indoor-gameplay) records the ROM
hash, exact inputs and reproduction commands. Use 6600 setup frames and 3600
measured frames, with screenshots disabled during timing.

Seven alternating pairs, `885cf94` → `1fab122`: **2.039695 s → 1.979765 s
(1.030×)**, with all 14 signatures matching. Matched long-run minus prefix
counters cover 3599 frames: 18,464,861,676 → 17,633,204,397 instructions
(4.50% fewer), about 0.2926 billion instructions per emulated second.
Both long-run and prefix signatures match. This is one indoor scene, not
battle, outdoor, sprite-heavy or general gameplay coverage.

## Accuracy and validation

At the current checkpoint and again after preserving the gameplay harness:

- `cargo fmt --all` and strict workspace/all-target Clippy pass.
- `cargo test` and `cargo test --release` each report **1593 passed, 0 failed,
  41 existing ignored**, including one nested tuning subprocess test.
- The all-features core check and minimal-feature library/buffer tests pass;
  the latter report 150 aggregate passes at the core checkpoint.
- The actual core builds to an ARMv6K library and assembly with filesystem
  storage disabled. No console executable has been linked or run.
- Differential tests compare original dot/scalar paths, complete observable
  timing state, fetch/register boundaries, IRQ/DMA behavior, all audio samples
  and save/restore continuation. See the workflow for test entry points.

Gambatte is separate and was not rerun in this pass. The existing
[Gambatte review](GAMBATTE_REVIEW.md) and failure inventory remain useful
accuracy work; passing default tests does not erase those known failures.
Windows behavior is checked by PR CI; the recorded speed measurements are
Linux-only. Exact benchmark signatures strengthen regression evidence but do
not establish exhaustive hardware accuracy.

## Hardware validation

The planning model is:

```text
remaining speedup = host instructions per emulated second
                  * assumed ARM/x86 instruction ratio
                  * assumed ARM cycles per instruction
                  / (target clock * core CPU budget)
```

Using 1.5 instructions per x86 instruction, 1.5 CPI, and 80% of one 804 MHz
core, the provisional near-speed gate is **at most 1.1×**. All eight host
cases now meet it: raster stress is about **1.05×**, synthetic window **1.07×**,
and indoor gameplay **1.02×**. Below 1 means headroom only under these
assumptions. Instruction expansion and CPI are uncalibrated; these estimates
are neither confidence bounds nor measured console speeds. The original 3DS
is a different, substantially tighter target.

The ARM library build used Rust nightly 1.100.0 (`feaadeeac`, LLVM 23.1.1):

```bash
cargo +nightly rustc --offline --locked -p vibe-emu-core --lib --release \
  --no-default-features --target armv6k-nintendo-3ds \
  -Z build-std=std,panic_abort --target-dir target/arm-audit -- --emit=asm
```

Install matching nightly `rust-src` for `build-std`; omit `--offline` when
first obtaining dependencies. See the official [Rust target setup](https://doc.rust-lang.org/rustc/platform-support/armv6k-nintendo-3ds.html)
for devkitARM/linking requirements. This checks code generation, not SDK
installation, a linked homebrew application, or a device run.

Inlining expanded the bounded runner from 263 to 8221 static ARM instruction
sites and single-step from 5264 to 6937; the new boundary helper has 131.
These are static sites, not dynamic counts. The larger footprint can hurt
ARM11 instruction-cache behavior even when x86 gets faster. The target uses
VFPv2 rather than NEON; inspect software division, barriers, 64-bit arithmetic,
spills and cache footprint. The [ARM11 timing reference](https://documentation-service.arm.com/static/5e8e1cd9fd977155116a4a7a)
explains why equal instruction counts do not imply equal execution costs.

When hardware is available:

1. Build/link a minimal benchmark application using the portable core; request
   New 3DS speed/cache mode through [osSetSpeedupEnable](https://github.com/devkitPro/libctru/blob/master/libctru/source/os.c).
2. Verify deterministic outputs against the host before assessing performance.
3. Compare the current expanded CPU layout with the smaller pre-layout decoder
   on the same device, compiler and workload. Measure actual frame deadlines.
4. Include frontend presentation, audio backend and OS costs. Keep exact audio
   enabled and test sustained operation, rather than only average core speed.
5. Broaden deterministic gameplay to battles, scrolling, sprites/windows and
   active audio, and include DMG as well as CGB.
6. Update this report with measured console results before claiming readiness.

Do not add more host-only optimizations merely to strengthen an uncalibrated
prediction. PGO remains a possible later release experiment; older PGO numbers
are obsolete for this core and must be remeasured using the current Cargo
harness and target toolchain.

## Experiment lessons and evidence

Forced MMU-read inlining regressed Polished by about 4% in exploratory runs
and lost most raster gains. Splitting PPU event processing into another
non-inlined function reduced capture-only gains. General window transactions
hurt background workloads until background/window paths were specialized.
Extra run-limit checks in the per-dot loop were not a reliable gain. Retain
these lessons, but profile the actual future revision before repeating them.

The durable evidence bundle in `docs/benchmarks/2026-11-09.json` records the
whole-branch comparison and final-pass measurements, inputs, signatures and
hashes. Build snapshots and raw profiles are reproducible using the workflow;
local copies under `/tmp/vibe-perf-next/`, `/tmp/vibe-perf-november/` and
`/tmp/vibe-pr-*` are temporary and may disappear. Git history retains the
superseded reports if a historical investigation needs them.
