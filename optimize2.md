## Performance Optimization Plan 2

This plan is based on fresh Linux `perf` profiling of a release build using a
real gameplay workload:

```bash
CARGO_PROFILE_RELEASE_DEBUG=1 RUSTFLAGS='-C force-frame-pointers=yes' \
  cargo build -p vibe-emu-ui --release

/usr/bin/time -v \
  target/release/vibe-emu-ui --headless --frames 6000 \
  polishedcrystal-debug-3.2.3.gbc

perf stat -d -d -d \
  target/release/vibe-emu-ui --headless --frames 6000 \
  polishedcrystal-debug-3.2.3.gbc

perf record -F 999 -g --call-graph fp \
  -o /tmp/vibeemu_polishedcrystal.perf.data \
  target/release/vibe-emu-ui --headless --frames 6000 \
  polishedcrystal-debug-3.2.3.gbc
```

Although the harness is `vibe-emu-ui`, the profile is effectively measuring the
core crate. In the call graph, `vibe_emu_core::cpu::Cpu::step` accounts for
about 97.7% of runtime, so frontend overhead is negligible in this headless run.

Baseline for this workload:

- 6000 frames in 21.90 s wall clock
- about 274 frames/s
- 243.1B instructions, 85.0B cycles, IPC 2.86
- 0.43% branch misses
- very low data-cache miss rates

The workload is primarily compute and control-flow bound, not memory bound.
That means the biggest wins will come from reducing retired instructions and
branch-heavy per-cycle bookkeeping, not from chasing cache locality first.

## Implemented Result

Nine recommendations from this plan have now been implemented:

- `Apu::tick_steps` now takes a fast path when no sweep work is pending, so it
  no longer splits the entire 1 MHz pipeline into small countdown chunks in the
  common no-sweep case.
- The rare sweep-handling path in `Apu::tick_steps` is now outlined into a
  dedicated helper, keeping the hot no-sweep path smaller and reducing
  stack/register pressure in the common case.
- `clock_wave_channel_2mhz_inner` now skips pending-commit bookkeeping when the
  wave channel has no pending wave RAM writes and no bugged-read countdown.
- `Apu::step` now skips sample mixing and DC-block filter work when no audio
  output consumer is attached, while still maintaining sample-timer phase.
- `Ppu::render_scanline` now uses bulk prefill for constant color-0 cases
  instead of recomputing the same value per pixel, and initializes
  `line_color_zero` once for sprite-bearing lines instead of rewriting it for
  each pixel.
- `tick_frame_sequencer_steps` now takes a direct fast path for the common case
  where a CPU batch crosses at most one relevant DIV edge, avoiding repeated
  boundary recalculation in the hot path.
- `Ppu::step` now takes a stable-span fast path in HBlank and VBlank when no
  pending register writes, delayed STAT mode bits, startup quirks, IRQ refresh,
  or render deadlines can become visible inside the current batch.
- The zero-event DMG path in `render_dmg_bg_window_scanline_simple` now renders
  background and window tile rows in chunks, so it fetches each tile row once
  per span instead of re-reading the same tile planes for every covered pixel.
- `Ppu::step` now also fast-paths stable mode-2 spans when OAM DMA contention,
  startup quirks, delayed STAT state, and pending register effects are absent,
  letting `oam_scan_advance` consume the batch without the outer scheduler loop.

Measured on the same workload after those changes:

- 6000 frames in 14.33 s wall clock
- about 419 frames/s
- latest sampled profile puts `Ppu::step` at 18.95% self-time
- latest sampled profile puts `Apu::step` at 18.12% self-time
- latest sampled profile puts `Apu::tick_steps` at 16.60% self-time
- latest sampled profile puts `Cpu::tick` at 12.49% self-time

That is roughly a 35% wall-clock speedup for this profiled workload.

## Current Hotspots

### Latest flat self-time hotspots

| Self | Function | File | Notes |
| ---: | --- | --- | --- |
| 18.95% | `vibe_emu_core::ppu::Ppu::step` | `crates/vibe-emu-core/src/ppu.rs` | PPU scheduling is still the single largest flat hotspot after the retained stable-span work |
| 18.12% | `vibe_emu_core::apu::Apu::step` | `crates/vibe-emu-core/src/apu.rs` | 2 MHz stepping and sample scheduling remain a major control-flow cost |
| 16.60% | `vibe_emu_core::apu::Apu::tick_steps` | `crates/vibe-emu-core/src/apu.rs` | the hot no-sweep path is smaller now, but 1 MHz APU pipeline stepping is still expensive |
| 12.49% | `vibe_emu_core::cpu::Cpu::tick` | `crates/vibe-emu-core/src/cpu.rs` | residual shared scheduling overhead under the core tick loop |
| 9.54% | `vibe_emu_core::ppu::Ppu::render_scanline` | `crates/vibe-emu-core/src/ppu.rs` | scanline rendering remains the main PPU slow-path cost after scheduler wins |

The APU frame-sequencer path no longer appears among the top self-time entries.
The retained PPU work now consists of HBlank, VBlank, and OAM scheduler fast
paths plus a chunked zero-event DMG renderer, and the latest retained APU
`tick_steps` refactor lowered the representative workload further to 14.33 s.

## Original Hotspots

### Flat self-time hotspots

| Self | Function | File | Notes |
| ---: | --- | --- | --- |
| 18.68% | `vibe_emu_core::apu::Apu::tick_steps` | `crates/vibe-emu-core/src/apu.rs` | 1 MHz pipeline stepping and sweep countdown work on every CPU tick |
| 17.50% | `vibe_emu_core::ppu::Ppu::step` | `crates/vibe-emu-core/src/ppu.rs` | mode scheduling, event handling, and per-chunk PPU bookkeeping |
| 13.64% | `vibe_emu_core::apu::Apu::step` | `crates/vibe-emu-core/src/apu.rs` | 2 MHz channel stepping and sample scheduling |
| 12.09% | `vibe_emu_core::apu::SquareChannel::tick_1mhz_batch` | `crates/vibe-emu-core/src/apu.rs` | square channel 1 MHz pipeline updates dominate APU self-time |
| 7.43% | `vibe_emu_core::ppu::Ppu::render_scanline` | `crates/vibe-emu-core/src/ppu.rs` | scanline compositing and DMG/CGB mode-3 rendering logic |
| 5.53% | `vibe_emu_core::apu::Apu::clock_wave_channel_2mhz_inner` | `crates/vibe-emu-core/src/apu.rs` | wave channel stepping plus pending-commit/shadow maintenance |
| 5.00% | `vibe_emu_core::apu::Apu::tick_frame_sequencer_steps` | `crates/vibe-emu-core/src/apu.rs` | DIV-driven frame sequencer edge handling |
| 2.18% | `vibe_emu_core::timer::Timer::step` | `crates/vibe-emu-core/src/timer.rs` | still pays per-cycle overhead around edges and reload windows |
| 1.56% | `vibe_emu_core::mmu::Mmu::dma_step` | `crates/vibe-emu-core/src/mmu.rs` | OAM DMA transfer is still stepped one dot at a time |

### Call-path concentration

The children-inclusive report shows most runtime concentrated in a few core
paths under `Cpu::step`:

- `Cpu::tick`: 88.46% cumulative
- `Apu::tick_steps`: 29.18% cumulative
- `Ppu::step`: 24.73% cumulative
- `Apu::step`: 17.94% cumulative
- `Ppu::render_scanline`: 7.50% cumulative
- `Apu::tick_frame_sequencer_steps`: 4.48% cumulative
- `Timer::step`: 1.80% cumulative
- `Mmu::dma_step`: 1.40% cumulative
- `Mbc3Rtc::add_cycles`: 1.17% cumulative

At the time of the initial profile this was an APU-first optimization problem.
After the retained APU wins, the frame-sequencer fast path, the outlined
`tick_steps` sweep slow path, the stable-span PPU scheduler fast paths, and the
chunked zero-event DMG renderer, the next best optimization target remains the
broader event-driven APU work, with remaining PPU value mostly in mode-3 and
render slow cases.

## What Has Already Been Fixed

The older [optimize.md](optimize.md) singled out eager PCM register refresh as a
major cost. That work appears to have landed already: `apu.rs` now uses
`pcm_dirty`, `mark_pcm_dirty`, and `ensure_pcm_regs_fresh`. It no longer shows
up as a top self-time hotspot.

That is useful context because it means the current profile is not stale. The
remaining hotspots are what is left after one of the previous largest wins.

## Recommendations

### 1. Make the APU genuinely event-driven across both clock domains

**Why this is first:** the APU dominates the profile. `Apu::tick_steps`,
`Apu::step`, `SquareChannel::tick_1mhz_batch`,
`clock_wave_channel_2mhz_inner`, and `tick_frame_sequencer_steps` together are
well over 50% of flat sampled time.

**Current shape:**

- `Apu::tick_steps` in `apu.rs` still visits all four channels for every CPU
  tick batch, even when output is stable.
- Each square channel still runs `tick_1mhz_batch` and recomputes output even
  when the channel is in a long steady state.
- `Apu::step` always walks the 2 MHz channel clocks and sample timer plumbing.
- `tick_frame_sequencer_steps` still iterates through divider edge handling for
  each relevant transition.

**Recommended change:**

- Track the next meaningful event for each channel explicitly:
  duty edge, wave sample edge, noise LFSR step, envelope step, sweep step,
  length expiry, pending commit visibility, and mixer sample boundary.
- Advance each channel by `min(next_event_delta, requested_cycles)` instead of
  invoking per-channel maintenance on every CPU tick.
- For square channels, keep the existing 3-stage pipeline semantics but model
  steady spans as a latched state plus a countdown to the next duty transition.
  Once the pipeline is filled, a stable span can be represented in O(1).
- For the wave channel, split the fast path from the register-hazard path so
  `clock_wave_channel_2mhz_inner` does not always pay for pending-commit and
  shadow-read maintenance when no wave-side quirk is active.
- Replace repeated per-step `if !can_skip_1mhz_batch()` dispatch with a compact
  active-channel bitmask or small fixed dispatch table.

**Why this should preserve accuracy:**

- The timing-sensitive events are already modeled explicitly; the main change is
  to step to those boundaries directly instead of rechecking them every tick.
- PCM register visibility can stay exact because `pcm_dirty` is already used to
  refresh values on observation.
- Audio sample output remains correct as long as boundaries are not crossed
  silently. That makes this an event-scheduling problem, not an approximation.

**Expected impact:** 20-35% total runtime reduction if done well.

### 2. Split the PPU into a true fast scheduler and a true slow scheduler

**Why this is second:** `Ppu::step` plus `render_scanline` are the next large
block, and the counters suggest instruction count matters more than memory.

**Current shape:**

- `Ppu::step` already batches some HBlank, VBlank, and OAM spans, but it still
  runs a branch-heavy loop that repeatedly checks mode state, LYC state,
  register-write queues, and render deadlines.
- `render_scanline` is extremely featureful and branches through DMG, CGB,
  mode-3 event handling, debug gates, sprite priority, and window continuity.
- The simple DMG path still performs per-pixel helper calls such as
  `dmg_bg_color_for_pixel`, `dmg_bg_en_for_pixel`,
  `dmg_lcdc_for_bg_fetch_t`, and `dmg_obp0_for_pixel` under common cases.

**Recommended change:**

- Precompute the next PPU event dot and jump directly to it in the fast path.
  The next event is one of:
  mode transition, STAT delay expiry, pending register write application,
  HBlank deferred render, or LY/LYC boundary.
- Keep the current logic as the slow path for lines with mode-3 register events,
  DMA contention, or debug tracing.
- Add a stricter scanline fast path for the common case:
  no mode-3 events, no debug tracing, stable LCDC, and ordinary BG/window
  rendering. In that path, work per tile or per 8-pixel chunk rather than per
  pixel helper call.
- Hoist per-line invariants out of the inner loops aggressively, especially the
  repeated LCDC/BGP/OBJ enable sampling decisions when there are no mode-3
  writes.
- Replace the dynamic FIFO-based CGB/DMG fetcher path with specialized helpers
  chosen once per line, not re-selected in the middle of the hot loop.

**Why this should preserve accuracy:**

- The slow path remains for lines where exact mid-mode behavior matters.
- The fast path is only taken when the event queues are empty and the line can
  be proven stable.
- This is a structural split, not a simplification of visible timing.

**Expected impact:** 15-25% total runtime reduction.

### 3. Continue PPU scheduler and scanline specialization work

**Why this matters:** the latest profile has `Ppu::step` at 24.04% self-time and
`Ppu::render_scanline` at 9.35%, making PPU control flow the most valuable next
place to spend effort.

**Current shape:**

- `Ppu::step` still loops through mode progression with frequent checks for LYC,
  STAT timing, deferred render points, and queued register effects.
- `render_scanline` still pays a large amount of branch-heavy per-pixel work on
  lines that do not actually need the full slow path.

**Recommended change:**

- Split `Ppu::step` into a stricter fast scheduler for stable spans and keep
  the existing logic as the slow path for lines with mid-mode events.
- Add a more aggressive stable-line rendering path that hoists per-line state
  and works in larger chunks when LCDC, palettes, and mode-3 events are stable.
- Avoid work in the outer PPU loop until the next real event boundary rather
  than rechecking every intermediate chunk.

**Why this should preserve accuracy:** the slow path remains for all lines where
mid-scanline behavior matters, and the fast path is only taken when the line is
provably stable.

**Expected impact:** 10-20% total runtime reduction.

### 4. Finish converting `Timer::step` and `Mmu::dma_step` to bulk stepping

**Why this matters:** these are not top-level hotspots, but together they are
still a few percent of runtime and they sit under `Cpu::tick`.

**Current shape:**

- `Timer::step` has a good fast path when fully idle, but still falls back to a
  per-cycle loop once an edge or reload window is in play.
- `Mmu::dma_step` still iterates `for _ in 0..cycles` and computes destination
  visibility one dot at a time.

**Recommended change:**

- For the timer, split stepping into phases:
  idle span, next falling edge, 4-cycle reload window, then next idle span.
  Only the tiny windows around edge-sensitive behavior need single-cycle logic.
- For DMA, compute how many byte boundaries are crossed by `cycles`, update
  `oam_dma_current_dest` once, and only read/copy the newly materialized bytes.
- Keep the current exact logic behind assertions or tests for tricky edge cases,
  then replace the per-dot outer loop with arithmetic.

**Why this should preserve accuracy:** the visible behavior is defined by edge
times and byte availability points, not by iterating every intermediate cycle.

**Expected impact:** 3-5% total runtime reduction.

### 5. Add an RTC arithmetic fast path for MBC3-heavy games

**Why this matters:** `Mbc3Rtc::add_cycles` shows up at 1.17% cumulative in this
Polished Crystal workload, which is high enough to matter for RTC-heavy titles.

**Current shape:**

- `add_cycles` is efficient for subsecond accumulation, but `advance_seconds`
  still walks time forward in smaller boundary chunks.

**Recommended change:**

- Replace repeated boundary walking with direct quotient/remainder propagation
  across seconds, minutes, hours, and day carry.
- Keep the current latch, halt, and carry semantics exactly the same.

**Why this should preserve accuracy:** this is pure arithmetic normalization,
not a timing-model change.

**Expected impact:** 1-2% total runtime reduction on RTC-heavy games.

### 6. Use PGO for release builds after structural wins land

**Why this matters:** the profile is branch-heavy but cache-friendly, which is a
good candidate for profile-guided optimization.

**Recommended change:**

- Add an optional PGO workflow for local and release benchmarking.
- Rebuild with profile generation, run representative headless ROM workloads,
  merge profiles, then rebuild with `-Cprofile-use`.

```bash
RUSTFLAGS='-Cprofile-generate=/tmp/vibeemu-pgo' \
  cargo build -p vibe-emu-ui --release

target/release/vibe-emu-ui --headless --frames 6000 \
  polishedcrystal-debug-3.2.3.gbc

llvm-profdata merge -o /tmp/vibeemu-pgo/merged.profdata \
  /tmp/vibeemu-pgo

RUSTFLAGS='-Cprofile-use=/tmp/vibeemu-pgo/merged.profdata -Cllvm-args=-pgo-warn-missing-function' \
  cargo build -p vibe-emu-ui --release
```

**Expected impact:** 5-12% on top of structural changes, but measure it.

## Recommended Execution Order

1. PPU fast scheduler plus scanline fast path split
2. APU event scheduling in `tick_steps` and `step`
3. Timer and DMA bulk stepping
4. RTC arithmetic cleanup
5. PGO on the resulting codebase

This order maximizes payoff while keeping the most timing-sensitive work under
continuous measurement.

## Guardrails For Accuracy

Every phase should keep the full existing test suite green, especially:

- `cargo test -p vibe-emu-core`
- `cargo test -p vibe-emu-core --release`
- targeted APU, PPU, timer, DMA, and MBC3 RTC tests

For the first two phases, add before/after profiling on the same ROM workload so
instruction count and sampled time move in the expected direction. Do not rely
on FPS alone.

## Bottom Line

The new profile says the core is no longer bottlenecked by stale PCM refresh
work. The main remaining opportunity is to stop paying per-tick control-flow
cost in the APU and PPU when hardware state is stable. If the APU and PPU are
made more aggressively event-driven while retaining the current slow paths for
edge-heavy lines and register hazards, a large performance jump should be
possible without giving up cycle accuracy.