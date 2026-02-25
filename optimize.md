## Performance Optimization Plan

This plan targets the hotspots identified by `perf` profiling (see [hotspots.md](hotspots.md)).
Every change preserves cycle-accurate behaviour and must pass the full test suite
(`cargo test && cargo test --release`) before merging.

Changes are ordered by estimated impact (highest first) and grouped into tiers.

---

## Tier 1 — High Impact, Low Risk

### 1.1 Cache trace-flag environment lookups in PPU render path

**Problem:**
Seven bare `env_bool_or_false(…)` calls inside `render_scanline` and
`render_dmg_bg_window_scanline_with_mode3_fetcher` invoke `std::env::var()`
(heap allocation + libc `getenv`) on every scanline — up to 154 times per
frame, ~9 240 times per second. `perf` shows `getenv` appearing at ~1–2 %
self-time inside the render call graph.

**Affected call sites** (all in [crates/vibe-emu-core/src/ppu.rs](crates/vibe-emu-core/src/ppu.rs)):

| Line | Key |
|-----:|-----|
| 5772 | `VIBEEMU_TRACE_WINDOW_EVENT_GATE` |
| 5811 | `VIBEEMU_TRACE_BG_FETCHER` |
| 5861 | `VIBEEMU_TRACE_DMG_RIGHT_OBJ` |
| 6456 | `VIBEEMU_DMG_BG_WINDOW_USE_POP_SCHEDULE` |
| 6566 | `VIBEEMU_TRACE_WIN_MAP_FETCH` |
| 6761 | `VIBEEMU_TRACE_SCY_RENDER` |
| 7145 | `VIBEEMU_TRACE_DMG_BG_OUTPUT` |

**Fix:**
Replace each bare call with a `define_env_bool_false!` / `define_env_bool_true!`
invocation (the caching macro already exists in the file and uses `OnceLock`).
For example:

```rust
// Before (per-scanline syscall)
if env_bool_or_false("VIBEEMU_TRACE_WINDOW_EVENT_GATE") { … }

// After (one-time cached read)
define_env_bool_false!(trace_window_event_gate, "VIBEEMU_TRACE_WINDOW_EVENT_GATE");
// …inside render_scanline:
if trace_window_event_gate() { … }
```

Similarly, calls in non-render hot paths (lines 1766, 1982, 2532) and other
`env_bool_or_false` / `env_bool_or_true` invocations at hot call sites outside `OnceLock` should be
audited and converted.

**Estimated gain:** ~2–3 % of total runtime removed.
**Risk:** None — behaviour is identical (env vars are process-global and
only read at startup for debug flags).

---

### 1.2 Lazy-evaluate `refresh_pcm_regs` (dirty-flag pattern)

**Problem:**
`Apu::refresh_pcm_regs` is the second-largest self-time symbol at 12–17 %
across all profiled workloads. It recomputes `pcm12` / `pcm34` / `pcm_samples`
/ `pcm_active` / `pcm_mask` by re-reading all four channel states. It is
called:

- Once per dot tick inside `Apu::tick` (the 1 MHz pipeline loop).
- Once per 2 MHz batch inside `Apu::step`.
- After every channel event (trigger, length clock, envelope, sweep, etc.)
  — ~20 additional call sites.

Games rarely read `PCM12` / `PCM34` registers; the vast majority of these
recomputations are wasted.

**Fix:**
Introduce a `pcm_dirty: bool` flag on `Apu`. Every current call site that
invokes `refresh_pcm_regs()` sets `self.pcm_dirty = true` instead. The
actual recomputation moves to a new `ensure_pcm_fresh(&mut self)` method
called only when `PCM12` / `PCM34` are read (in the MMU I/O read handler),
and at the single sample-output point (`mix_output` / audio push).

```rust
#[inline]
fn mark_pcm_dirty(&mut self) {
    self.pcm_dirty = true;
}

#[inline]
fn ensure_pcm_fresh(&mut self) {
    if self.pcm_dirty {
        self.refresh_pcm_regs();
        self.pcm_dirty = false;
    }
}
```

The `Apu::tick` per-dot loop currently calls `refresh_pcm_regs` inside
every iteration. Replace with `mark_pcm_dirty`. The read path in
`Mmu::read_byte_inner` for addresses `0xFF76`/`0xFF77` calls
`ensure_pcm_fresh` before returning.

**Estimated gain:** ~10–15 % of total runtime removed.  `refresh_pcm_regs`
drops from a per-dot cost to a sporadic on-read cost.
**Risk:** Low — PCM register reads remain cycle-accurate because
`ensure_pcm_fresh` is called before the value is observed. The internal
`pcm_samples` / `pcm_active` arrays are only consumed by `mix_output`,
which calls `current_sample()` (pipeline stage 2), not `peek_sample()`.
Tests that validate PCM12/PCM34 reads will catch any regression.

---

### 1.3 Batch PPU `step` idle modes (HBLANK / VBLANK skip-ahead)

**Problem:**
`Ppu::step` processes cycles one dot at a time in a `while remaining > 0`
loop. During HBLANK (~87–204 dots per line) and VBLANK (4560 dots per
frame), each iteration does:

1. `tick_stat_mode_delay` (usually a no-op after 1 cycle)
2. Mode-clock increment
3. `tick_pending_reg_writes` (usually empty)
4. Target comparison + branch

Most of these iterations are pure countdown. A single frame wastes ~70 %
of `Ppu::step` invocations on no-visible-work ticks.

**Fix:**
At the start of each mode branch, when no pending events exist, compute
the number of dots until the next mode transition and consume them in one
step. Specifically:

```rust
MODE_HBLANK if !self.dmg_hblank_render_pending
    && self.stat_mode_delay == 0
    && self.pending_reg_writes_empty() =>
{
    let skip = (target - self.mode_clock).min(remaining);
    self.mode_clock += skip;
    remaining -= skip;
    // fall through to normal transition check
}
```

An identical pattern applies to VBLANK (no LYC compare fires within a
vblank line, so we can skip to the next line boundary). `update_lyc_compare`
is called once after the skip rather than per dot.

The guards (`stat_mode_delay == 0`, no pending writes, no pending render)
ensure correctness whenever mid-mode register writes are in flight.

**Estimated gain:** ~8–12 % of total runtime removed for idle modes.
**Risk:** Medium — careful to preserve `update_stat_irq` / LYC timing.
Must be guarded behind "no mid-mode events" checks. All existing PPU timing
tests will validate.

---

## Tier 2 — Medium Impact, Medium Risk

### 2.1 Batch APU 1 MHz pipeline ticks

**Problem:**
`Apu::tick` loops once per dot tick, calling `tick_1mhz()` on all four
channels and then `refresh_pcm_regs()` (addressed in 1.2) each iteration.
`tick_1mhz()` is a three-stage shift register (`out_latched → out_stage1 →
out_stage2`). After N consecutive ticks with no state changes, the pipeline
converges — the shift can be computed in O(1).

**Fix:**
When the APU is enabled but no channel event fires, batch the 1 MHz
pipeline advancement:

```rust
if ticks <= 3 {
    // Pipeline is only 3 stages deep; after 3+ identical ticks
    // all stages hold the current compute_output() value.
    for _ in 0..ticks { ch.tick_1mhz(); }
} else {
    let sample = ch.compute_output();
    ch.out_latched = sample;
    ch.out_stage1 = sample;
    ch.out_stage2 = sample;
}
```

This is valid because `compute_output` is a pure function of
`enabled`, `dac_enabled`, `sample_surpressed`, `duty_pos`, and
`envelope.volume` — none of which change between 1 MHz ticks.

**Estimated gain:** ~5–8 % of total runtime (the `Apu::tick` self-time
minus the `refresh_pcm_regs` portion).
**Risk:** Medium — must verify that no channel event can fire between
1 MHz ticks within the same `Apu::tick` call. Currently, sweep ticks are
interleaved with the 1 MHz loop, so the batch boundary must stop before
a sweep tick fires.

---

### 2.2 Inline and flatten DMG background pixel helpers

**Problem:**
`dmg_bg_color_for_pixel`, `dmg_bg_en_for_pixel`, and
`dmg_lcdc_for_bg_fetch_t` together account for ~15 % of self-time.
They are called 160 times per scanline (once per visible pixel) inside the
`render_dmg_bg_window_scanline_simple` loop. Each call performs:

- Array lookups into `dmg_line_lcdc_at_pixel`, `dmg_line_mode3_t_at_pixel`
- Iterating `mode3_lcdc_events` slice (up to ~10 entries)
- Multiple env-var–cached bias lookups (via `OnceLock`, cheap but not free)
- Branching on CGB/DMG compat mode, sprite count, revision, ly == 0, etc.

**Fix:**

1. Add `#[inline]` to `dmg_bg_color_for_pixel`, `dmg_bg_en_for_pixel`,
   `dmg_bgp_for_pixel`, and `dmg_lcdc_for_bg_fetch_t`. Several of these
   already have `#[inline]` on adjacent helpers but not on themselves.

2. In `render_dmg_bg_window_scanline_simple`, hoist invariant per-scanline
   state outside the `for x in 0..SCREEN_WIDTH` loop.  Currently each
   pixel re-evaluates:
   - `self.is_dmg_mode()` (constant for the line)
   - `self.sprite_count > 0` (constant)
   - `dmg_bg_en_left_raw_threshold()` and friends (OnceLock, but function
     call overhead per pixel)
   - `self.mode3_lcdc_event_count` (constant)
   - Bias values from env helpers

   Move these to local variables before the loop.

3. For `dmg_lcdc_for_bg_fetch_t`, when `mode3_lcdc_event_count == 0`
   (the common case — no mid-mode LCDC writes), short-circuit immediately:

   ```rust
   #[inline]
   fn dmg_lcdc_for_bg_fetch_t(&self, _t: u16) -> u8 {
       if self.mode3_lcdc_event_count == 0 {
           return self.mode3_lcdc_base;
       }
       self.dmg_lcdc_for_bg_fetch_t_slow(t)
   }
   ```

**Estimated gain:** ~5–8 % of total runtime.
**Risk:** Low — pure refactoring with no behavioural change.

---

### 2.3 Reduce `Timer::step` per-cycle loop overhead

**Problem:**
`Timer::step` loops once per CPU cycle (typically 4–8 per M-cycle).
Each iteration checks `pending_reload`, increments `div`, evaluates
`signal()` (falling-edge detection), and handles `tma_latch`. Most
iterations are pure `div` increment with no edge.

**Fix:**
When no reload is pending and no TMA latch is queued, calculate the number
of cycles until the next falling edge of the selected TAC bit and skip
ahead:

```rust
if self.pending_reload.is_none() && self.tma_latch.is_none() {
    let bit_mask = self.timer_bit_mask(); // e.g. 1 << 9 for TAC=0
    let cycles_to_edge = /* distance to next falling edge of bit_mask in div */;
    let skip = cycles_to_edge.min(remaining);
    self.div = self.div.wrapping_add(skip);
    remaining -= skip;
    self.last_signal = self.signal();
    continue;
}
```

This reduces the timer from O(cycles) to O(edges), which is a large win
for long M-cycle batches.

**Estimated gain:** ~2–3 % of total runtime.
**Risk:** Medium — falling-edge calculation must be exact.  Existing timer
tests (blargg, mooneye) will validate.

---

## Tier 3 — Build / Toolchain Improvements

### 3.1 Enable thin LTO for release builds

**Problem:**
The workspace `Cargo.toml` has no `[profile.release]` customization.
Cross-crate inlining (e.g. `vibe_emu_core` functions called from
`vibe_emu_ui::main`) is limited without LTO.

**Fix:**
```toml
[profile.release]
lto = "thin"
codegen-units = 1
```

`thin` LTO gives most of the cross-crate inlining benefit with
significantly faster link times than `fat` LTO.  `codegen-units = 1`
further helps the optimizer see the full picture.

**Estimated gain:** ~10–20 % overall runtime improvement (well-established
for Rust emulators with many small, hot functions across crate boundaries).
**Risk:** None for correctness. Increases release build time.

---

### 3.2 Profile-Guided Optimization (PGO)

**Problem:**
The compiler makes branch-prediction and layout decisions without runtime
data. Emulators have highly predictable hot paths that benefit enormously
from PGO.

**Fix:**
Add a PGO build script or Makefile target:

```bash
# Instrument
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" \
  cargo build --release -p vibe-emu-ui

# Collect profile (headless run with representative ROM)
target/release/vibe-emu-ui --headless --frames 3600 path/to/rom.gb

# Merge
llvm-profdata merge -o /tmp/pgo-data/merged.profdata /tmp/pgo-data

# Rebuild with PGO
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data/merged.profdata" \
  cargo build --release -p vibe-emu-ui
```

**Estimated gain:** ~10–15 % on top of LTO, based on published Rust
emulator benchmarks.
**Risk:** None for correctness. Adds a two-pass build step.

---

### 3.3 Add `#[inline]` annotations to critical hot functions

**Problem:**
Several top-of-profile functions lack `#[inline]` hints:

- `Ppu::step` (6080)
- `Ppu::render_scanline` (5698)
- `Apu::refresh_pcm_regs` (3024)
- `Apu::tick` (2488)
- `Apu::step` (3086)
- `Cpu::tick` (260) — already has `#[inline]`
- `Timer::step` (93)
- `Mmu::dma_step` (1054)

**Fix:**
Add `#[inline]` (not `#[inline(always)]`) to the above. With LTO enabled
(3.1), these hints become cross-crate effective.

**Estimated gain:** ~2–5 % incremental with LTO.
**Risk:** None.

---

## Tier 4 — Longer-Term Architectural

### 4.1 PPU scanline rendering: tile-row caching

**Problem:**
`render_dmg_bg_window_scanline_simple` fetches two VRAM bytes per pixel
(low plane + high plane). For an 8-pixel tile row, this means 16
`vram_read_for_render` calls instead of 2. Combined with the per-pixel
`dmg_bg_color_for_pixel` / `dmg_bg_en_for_pixel` overhead, this is the
single largest cost center.

**Fix:**
Restructure the BG rendering loop to iterate by tile column (20–21 tiles
per line) instead of by pixel. Read tile data once per tile, then decode
all 8 pixels from the cached bytes:

```rust
for tile_col in 0..21 {
    let tile_index = self.vram_read_for_render(0, map_addr + tile_col);
    let lo = self.vram_read_for_render(0, addr_lo);
    let hi = self.vram_read_for_render(0, addr_hi);
    for bit in (0..8).rev() {
        let color_id = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
        // pixel output…
    }
}
```

This reduces VRAM reads from 320 to ~42 per line and keeps the color /
priority logic per-pixel for accuracy.

**Caveats:**
- SCX fine scroll means the first and last tile are partial.
- Mid-scanline LCDC writes (tile data select bit 4) require falling back
  to per-pixel mode for affected tiles.
- Window overlay complicates tile boundaries.

The existing `use_fetcher` flag already detects lines with mid-scanline
register writes and routes them to the dot-accurate fetcher path. The
tile-cached path only needs to handle the simple/common case.

**Estimated gain:** ~8–12 % of total runtime.
**Risk:** Medium-high — must carefully handle edge cases. The fetcher
fallback path is the safety net.

---

### 4.2 `Mmu::dma_step` loop elimination

**Problem:**
When DMA is active, `Cpu::tick` enters a per-dot loop that calls both
`dma_step(1)` and `ppu.step(1, …)` individually. This forces the PPU into
single-dot mode even during modes that could otherwise be batched.

**Fix:**
Compute the number of DMA bytes that will transfer during this M-cycle
(typically 1–2), perform the DMA copy in a tight loop, then call
`ppu.step(dot_cycles, …)` with the full batch. The DMA transfer is a
simple memcpy from source to OAM with a per-byte address increment — it
does not interact with PPU mode transitions.

Guard: if a VRAM DMA source overlaps with the PPU's current VRAM access
window, fall back to per-dot stepping.

**Estimated gain:** ~2–3 % of total runtime.
**Risk:** Medium — DMA bus conflicts need careful handling.

---

## Implementation Order

| Phase | Items | Test Gate |
|------:|-------|-----------|
| 1 | 1.1 (env cache), 3.1 (LTO), 3.3 (inline hints) | `cargo test && cargo test --release` |
| 2 | 1.2 (lazy PCM) | `cargo test && cargo test --release` — specifically watch APU PCM read tests |
| 3 | 1.3 (PPU batch idle), 2.3 (timer skip-ahead) | Full PPU/timer test suites |
| 4 | 2.1 (APU pipeline batch), 2.2 (pixel helper flatten) | `cargo test --release`, APU channel activation tests |
| 5 | 3.2 (PGO build script) | Benchmark before/after with `--headless --frames` |
| 6 | 4.1 (tile-row cache), 4.2 (DMA batch) | Full test suite + Gambatte compatibility |

Each phase should be benchmarked with `perf` before and after to validate
the expected improvement.

---

## Expected Total Improvement

Conservative estimate for Phases 1–4: **30–45 % reduction in CPU time** for
the core emulation loop, validated by headless frame timing.

With LTO + PGO (Phase 5): an additional **15–25 %** on top.

Tile-row caching (Phase 6) is the highest-effort item but targets the single
largest remaining cost center after Phases 1–4.
