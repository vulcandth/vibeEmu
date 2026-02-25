## Game Boy / GBC Emulator Performance Optimization Research

Research into well-established optimization techniques used by high-performance, cycle-accurate Game Boy
emulators (SameBoy, Gambatte, mGBA, BGB, and others). Each suggestion is evaluated against the vibeEmu
hotspot profile from [hotspots.md](hotspots.md) and the current source in `crates/vibe-emu-core/src/`.

---

## PPU — Pixel Processing Unit (~45–50 % of runtime)

`Ppu::step` + `render_scanline` + pixel helper functions dominate every profiled workload.

### ✅ 1. Eliminate per-scanline `std::env::var` calls in render paths

**Impact: HIGH — immediate, zero-risk win.**

Several hot render functions call `env_bool_or_false()` directly (not through
the `define_env_bool_false!` caching macro). Each bare call invokes `std::env::var` →
`libc::getenv` → heap-allocated `String` — on every scanline or even per-pixel.

Instances inside `render_scanline` and its callees:

| Call site | Function |
|---|---|
| [ppu.rs L5772](crates/vibe-emu-core/src/ppu.rs#L5772) | `env_bool_or_false("VIBEEMU_TRACE_WINDOW_EVENT_GATE")` |
| [ppu.rs L5811](crates/vibe-emu-core/src/ppu.rs#L5811) | `env_bool_or_false("VIBEEMU_TRACE_BG_FETCHER")` |
| [ppu.rs L5861](crates/vibe-emu-core/src/ppu.rs#L5861) | `env_bool_or_false("VIBEEMU_TRACE_DMG_RIGHT_OBJ")` |
| [ppu.rs L6456](crates/vibe-emu-core/src/ppu.rs#L6456) | `env_bool_or_false("VIBEEMU_DMG_BG_WINDOW_USE_POP_SCHEDULE")` |
| [ppu.rs L6566](crates/vibe-emu-core/src/ppu.rs#L6566) | `env_bool_or_false("VIBEEMU_TRACE_WIN_MAP_FETCH")` |
| [ppu.rs L6761](crates/vibe-emu-core/src/ppu.rs#L6761) | `env_bool_or_false("VIBEEMU_TRACE_SCY_RENDER")` |
| [ppu.rs L7145](crates/vibe-emu-core/src/ppu.rs#L7145) | `env_bool_or_false("VIBEEMU_TRACE_DMG_BG_OUTPUT")` |
| [ppu.rs L1766](crates/vibe-emu-core/src/ppu.rs#L1766) | `env_bool_or_false("VIBEEMU_TRACE_SCX_EVENTS")` |
| [ppu.rs L1982](crates/vibe-emu-core/src/ppu.rs#L1982) | `env_bool_or_false("VIBEEMU_TRACE_SCY_EVENTS")` |
| [ppu.rs L2532](crates/vibe-emu-core/src/ppu.rs#L2532) | `env_bool_or_false("VIBEEMU_TRACE_BGP_SAMPLE")` |

**Fix:** Convert every bare `env_bool_or_false(...)` call to a `define_env_bool_false!`-generated
function (which uses `OnceLock` and pays the cost exactly once). The codebase already has this
pattern — it just isn't applied uniformly. Alternatively, gate the tracing blocks behind
`#[cfg(feature = "ppu-trace")]` so they compile out entirely in release builds.

**Status:** ✅ Completed on `2026-02-24`.

**Implemented in:** `crates/vibe-emu-core/src/ppu.rs`

**Verification:** Accuracy-preserving refactor only (env flag read caching; no timing/model logic
changed). Validation run:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### ✅ 2. Single-dot `Ppu::step` loop — batch-friendly fast paths

Currently `Ppu::step` receives a `cycles` count but processes dots one at a time (`while
remaining > 0 { remaining -= 1; … }`). Every single dot goes through:

- `tick_stat_mode_delay()`
- LCDC-off check
- DMG startup sequence branch
- `update_lyc_compare()`
- mode clock increment
- `tick_pending_reg_writes()`
- Full `match self.mode { … }` dispatch

**Technique (Gambatte / SameBoy style):** In modes where no mid-scanline event can fire within
the remaining dots (HBLANK, VBLANK), compute the *next event boundary* and skip directly to
it, advancing `mode_clock` by the delta. This turns a per-dot O(n) loop into an O(events)
loop.

```
let skip = match self.mode {
    MODE_HBLANK => {
        let next_event = self.mode0_target_cycles - self.mode_clock;
        if !self.dmg_hblank_render_pending {
            next_event.min(remaining)
        } else {
            let render_at = dmg_hblank_render_delay() - self.mode_clock;
            render_at.min(remaining)
        }
    }
    MODE_VBLANK => { /* similar: skip to next LY boundary */ }
    _ => 1, // OAM / Transfer must stay per-dot for accuracy
};
self.mode_clock += skip;
remaining -= skip;
```

This is the single largest potential speedup. HBLANK is ~204 dots per line, VBLANK is
~4560 dots per frame. Skipping those in bulk removes tens of thousands of iterations per
frame with zero accuracy loss, because no observable state changes mid-HBLANK/VBLANK except
at the boundaries already checked (LY increment, mode transition, LYC compare).

**Status:** ✅ Completed on `2026-02-24`.

**Implemented in:** `crates/vibe-emu-core/src/ppu.rs`

**Implementation notes:** `Ppu::step` now batches dots in `MODE_HBLANK` and `MODE_VBLANK`
by skipping directly to the next event boundary (`dmg_hblank_render_delay`,
`mode0_target_cycles`, or `MODE1_CYCLES`) when safe. The fast path is guarded by
`stat_mode_delay == 0` and `pending_reg_write_count == 0`, and OAM/TRANSFER remain
single-dot for accuracy.

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### ✅ 3. Cache palette lookups / precompute palette tables

`dmg_bg_color_for_pixel` is called once per pixel (160 per scanline, 144 scanlines per frame =
23,040 calls/frame). Each call chains through `dmg_bgp_for_pixel` → palette register sample →
shade lookup → DMG compat branch → color decode.

**Technique (Gambatte):** When BGP/OBP0/OBP1 registers are written, precompute a 4-entry
`[u32; 4]` color table for each palette. Then pixel color lookup becomes a single array index:

```rust
// On BGP write:
for shade in 0..4 {
    self.bgp_color_table[shade] = /* decoded color for this shade */;
}

// In render loop:
let color = self.bgp_color_table[color_id as usize];
```

For CGB mode, `cgb_bg_color_from_color_id` does a palette RAM read + 15-bit→32-bit decode per pixel.
Precomputing `[u32; 32]` (8 palettes × 4 colors) on BGPD/OBPD writes eliminates this entirely.

**Status:** ✅ Completed on `2026-02-24`.

**Implemented in:** `crates/vibe-emu-core/src/ppu.rs`

**Implementation notes:** Added cached palette tables for CGB BG/OBJ colors and DMG BG/OBJ
lookup paths. Hot render paths now use table lookups (`cgb_bg_color_table`,
`cgb_obj_color_table`, `dmg_bg_color_table`, `dmg_obj_color_table`) instead of per-pixel
palette RAM decode.

**Cache refresh points:**

- PPU construction / initialization (`new_with_revisions`)
- DMG runtime palette changes (`set_dmg_palette`)
- DMG-compat palette bootstrap (`apply_dmg_compatibility_palettes`)
- CGB palette data writes (`FF69` / `FF6B`)

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### ✅ 4. Scanline-level rendering vs. dot-level rendering

vibeEmu already uses scanline-level rendering as a fast path (`render_dmg_bg_window_scanline_simple`)
and falls back to the dot-accurate fetcher only when mid-scanline register events occurred. This
is the right architecture. Potential refinements:

- **Tile row caching:** When SCX/SCY haven't changed since the previous scanline and no VRAM
  writes occurred in the visible tile region, reuse the decoded tile row data. SameBoy uses a
  "tile cache" that invalidates on VRAM bank writes (tracked via dirty flags per tile).

- **Batch pixel output:** Instead of computing `self.ly as usize * SCREEN_WIDTH + x` per pixel,
  compute the row base pointer once and index with `x` alone:

  ```rust
  let row_base = self.ly as usize * SCREEN_WIDTH;
  // then:
  self.framebuffer[row_base + x] = color;
  ```

- **`line_color_zero` / `line_priority` fill optimization:** The current `self.line_priority
  .fill(false)` and `self.line_color_zero.fill(false)` at the start of every scanline are fine,
  but could be avoided when sprite count is zero (no sprites → no priority compositing needed).

**Status:** ✅ Completed on `2026-02-24`.

**Implemented in:** `crates/vibe-emu-core/src/ppu.rs`

**Implementation notes:**

- Added per-scanline `row_base` indexing in hot render paths to avoid repeated
  `self.ly as usize * SCREEN_WIDTH + x` recomputation.
- Gated sprite-priority scratch maintenance (`line_priority`, `line_color_zero`,
  `cgb_line_obj_enabled`) to lines with sprites (`sprite_count > 0`).
- Added an early `any_obj_enabled` fast path for `sprite_count == 0`.

These are rendering-pipeline micro-optimizations only; no timing model behavior changed.

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### ✅ 5. Sprite rendering: early-exit and sorted iteration

The sprite loop currently iterates all sprites found during OAM scan. Two established
optimizations:

- **Skip sprite rendering entirely when `sprite_count == 0`** (already partially done, but the
  `any_obj_enabled` check still iterates LCDC events even when count is zero).

- **X-sorted sprite list with early termination:** Sprites are already sorted by X position for
  priority. Once a sprite's X position exceeds the right screen edge, break.

**Status:** ✅ Completed on `2026-02-24`.

**Implemented in:** `crates/vibe-emu-core/src/ppu.rs`

**Implementation notes:**

- Sprite rendering is skipped entirely when `sprite_count == 0` (via early
  `any_obj_enabled = false`).
- In the sprite render loop, fully off-screen-left sprites (`x <= -8`) are skipped.
- On non-CGB-native lines (where sprites are X-sorted), rendering now early-breaks
  when `sprite.x >= SCREEN_WIDTH` since remaining sprites are also off-screen-right.

These changes are control-flow optimizations only; pixel/timing behavior is unchanged.

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 6. `dmg_bg_en_for_pixel` — reduce per-pixel branching ✅

Completed in [ppu.rs](crates/vibe-emu-core/src/ppu.rs):

- Added an early fast path in `dmg_bg_en_for_pixel` that returns
  `mode3_lcdc_base.bit0` immediately when `mode3_lcdc_event_count == 0`.
- Precomputed a per-scanline DMG BG-enable constant in hot render paths and sprite
  compositing when no mode3 LCDC events are present, avoiding repeated per-pixel
  branch-heavy evaluation.

Behavior remains unchanged because this optimization is only active under the
same condition where BG enable is line-constant by construction (no mode3 LCDC
events).

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

---

## APU — Audio Processing Unit (~25–30 % of runtime)

`Apu::refresh_pcm_regs` alone is 12–17 %, plus `Apu::tick` and `Apu::step`.

### 7. Lazy PCM register refresh ✅

`refresh_pcm_regs` is called on every dot tick inside `Apu::tick`, plus again at the end of
`Apu::step` after the 2 MHz domain. PCM12/PCM34 are CGB-only registers at FF76/FF77 that
are rarely read by games.

**Technique (SameBoy):** Only recompute PCM registers when they are actually *read*. Set a
dirty flag when any channel state changes, and recompute lazily in `read_pcm()`:

```rust
pub fn read_pcm(&mut self, addr: u16) -> u8 {
    if self.pcm_dirty {
        self.refresh_pcm_regs();
        self.pcm_dirty = false;
    }
    match addr {
        0xFF76 => self.pcm12,
        0xFF77 => self.pcm34,
        _ => 0xFF,
    }
}
```

Completed in [apu.rs](crates/vibe-emu-core/src/apu.rs):

- Added `pcm_dirty` tracking and lazy refresh helpers (`mark_pcm_dirty`,
  `ensure_pcm_regs_fresh`).
- Converted hot-path eager recomputes to dirty marking, including per-dot `Apu::tick`
  and post-2MHz `Apu::step` updates.
- PCM-visible accessors now refresh lazily on demand (`read_pcm`, `pcm_mask`,
  `pcm_samples`).

Behavior remains unchanged for externally visible PCM reads: values are recomputed
before each access, while avoiding repeated recomputation in non-observing paths.

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 8. Batch 1 MHz pipeline ticks ✅

Inside `Apu::tick`, the loop `for _ in 0..ticks { ch1.tick_1mhz(); ch2.tick_1mhz(); …;
refresh_pcm_regs(); }` runs per-dot. The 1 MHz pipeline is a simple 3-stage shift register
with no feedback between stages. When `ticks > 1` (common: 4 ticks per M-cycle in normal
speed), the final state after N ticks can be computed directly:

```rust
// After N ticks, stage2 = the sample from (N-2) ticks ago
// For N >= 3, only the last 3 compute_output() calls matter
if ticks >= 3 {
    // Compute the 3 most recent samples
    let s0 = ch.compute_output();  // oldest relevant
    let s1 = ch.compute_output();  // = s0 since state hasn't changed mid-step
    let s2 = ch.compute_output();
    ch.out_stage2 = s0;
    ch.out_stage1 = s1;
    ch.out_latched = s2;
}
```

Completed in [apu.rs](crates/vibe-emu-core/src/apu.rs):

- Added batched 1 MHz pipeline shift helpers for square, wave, and noise channels.
- Updated `Apu::tick` to process elapsed dots in chunks that end at the next sweep
  boundary, preserving exact ordering of pipeline shifts vs. sweep events.
- Within each chunk, pipeline shifting is done in O(1) with equivalent final
  stage values, instead of per-dot `tick_1mhz()` calls.

Behavior remains unchanged because chunking is bounded by sweep event boundaries,
and each chunk preserves the same sample source/state ordering as the previous
per-dot loop.

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 9. Sweep tick optimization ✅

Inside the `Apu::tick` dot loop, sweep is conditionally ticked based on `lf_div_counter %
divisor`. This modulo operation happens every dot. Replace with a countdown:

```rust
self.sweep_countdown -= 1;
if self.sweep_countdown == 0 {
    self.sweep_countdown = divisor;
    self.tick_sweep(1);
}
```

Completed in [apu.rs](crates/vibe-emu-core/src/apu.rs):

- Added persistent `sweep_dot_countdown` state to schedule sweep ticks with
  decrement/reset logic.
- Updated `Apu::tick` to consume dot chunks by countdown and call `tick_sweep(1)`
  exactly at countdown boundaries.
- Kept boundary alignment exact across speed changes by re-synchronizing countdown
  from `lf_div_counter` when needed.

Behavior remains unchanged: sweep calculations still occur at the same dot-phase
boundaries as before, but without modulo-based boundary checks in the hot path.

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 10. Sample generation — avoid per-CPU-cycle loop ✅

`Apu::step` has `for _ in 0..cycles { … sample_timer_accum … }`. When the sample rate is
~48 kHz and CPU clock is ~4 MHz, a sample is emitted roughly every 87 cycles. The loop can
be replaced with division:

```rust
self.sample_timer_accum += rate * cycles as u64;
while self.sample_timer_accum >= sample_period {
    self.sample_timer_accum -= sample_period;
    let (left, right) = self.mix_output();
    self.push_samples(left, right);
}
```

Completed in [apu.rs](crates/vibe-emu-core/src/apu.rs):

- Replaced per-cycle sample timer accumulation in `Apu::step` with batched
  accumulation: `sample_timer_accum += rate * cycles`.
- Emission now uses a threshold loop over accumulated time (`while
  sample_timer_accum >= sample_period`) instead of per-CPU-cycle checks.
- Preserved behavior-sensitive state updates (`cpu_cycles`) and kept trace output
  compatibility under `apu-trace`.

Behavior remains unchanged for audio timing: generated sample count and emission
points are equivalent to the previous per-cycle accumulation logic, while removing
hot per-cycle overhead in non-trace builds.

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

---

## CPU — Instruction Dispatch & Tick Fan-out (~6 %)

### 11. Reduce `Cpu::tick` fan-out cost ✅

Every M-cycle, `Cpu::tick` calls into: cartridge RTC, timer, APU (step + tick_frame_sequencer
+ tick), serial, PPU, DMA. The function at [cpu.rs L260](crates/vibe-emu-core/src/cpu.rs#L260)
is the central scheduling hub.

**Technique (Gambatte):** Batch M-cycles where possible. Many instructions take 1 M-cycle
for the opcode fetch. Instead of calling `tick(1)` between every M-cycle of a multi-cycle
instruction, accumulate the count and call `tick(n)` once at the end when no memory access
happens between the M-cycles. The CPU already passes `m_cycles: u8` but `step()` always
calls `tick(mmu, 1)` — using larger batches where safe would reduce call overhead.

**Caveat:** Some instructions have observable side effects between M-cycles (memory reads/
writes that interact with PPU/timer state). Only batch "internal" M-cycles that don't
touch the bus.

Completed in [cpu.rs](crates/vibe-emu-core/src/cpu.rs) and
[apu.rs](crates/vibe-emu-core/src/apu.rs):

- Added a consolidated APU entrypoint (`Apu::run_cpu_tick`) that executes the
  existing per-tick APU sequence in-order: `step` → `tick_frame_sequencer` →
  `tick`.
- Updated `Cpu::tick` to call that single APU entrypoint, reducing fan-out call
  overhead in the central scheduling path while preserving timing/order.

Instruction-level audit for internal M-cycle batching:

- Existing batched internal-cycle cases are retained (`INT` acknowledge path uses
  `tick(..., 3)`, `ADD SP, e8` uses `tick(..., 2)`).
- Most remaining `tick(..., 1)` sites are separated by bus-visible operations
  (`fetch8/fetch16/read8/write8/push_stack/pop_stack`) or interrupt-sensitive
  state changes, so merging them would change externally observable timing.
- Conclusion: there are no additional low-risk instruction-level multi-cycle
  batches available in the current dispatcher without a larger execution-model
  refactor.

Behavior remains unchanged: this is a call-graph simplification only, with the
same inputs and execution order for all APU subdomains per CPU tick.

**Verification:** Accuracy-preserving behavior validated with:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 12. Instruction dispatch — computed goto / jump table ✅

The CPU `step` function uses `match` on opcodes. Rust typically lowers dense matches
to jump tables, so this was verified directly from release assembly output.

Completed via assembly verification:

- Built assembly with `cargo rustc -p vibe-emu-core --release --lib -- --emit=asm`.
- Inspected `target/release/deps/vibe_emu_core-*.s` and confirmed `Cpu::step` contains
  jump-table sections (`.LJTI50_*`) and indirect dispatch jumps (`jmpq *...`) for opcode
  dispatch paths.

Conclusion: dispatch is already jump-table-based in release builds; no source refactor is
needed for this optimization item.

**Verification:**

- `cargo rustc -p vibe-emu-core --release --lib -- --emit=asm`
- Confirmed `Cpu::step` jump-table labels in emitted assembly
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 13. Avoid redundant DIV snapshots ✅

`Cpu::tick` captures `prev_dot_div` and `curr_dot_div` for the APU, plus `prev_cpu_div`
and `curr_cpu_div` for the timer. Both pairs are computed from simple wrapping adds. This
is lightweight, but the pattern of passing (prev, curr) to every subsystem means each
subsystem re-derives the delta. Consider passing the delta directly when subsystems only
need the tick count.

Completed in [cpu.rs](crates/vibe-emu-core/src/cpu.rs),
[apu.rs](crates/vibe-emu-core/src/apu.rs), and
[serial.rs](crates/vibe-emu-core/src/serial.rs):

- Added delta-based APU/serial tick entrypoints (`run_cpu_tick_steps`,
  `tick_frame_sequencer_steps`, `tick_steps`, `step_steps`) and kept existing
  `(prev, curr)` wrappers for compatibility.
- Updated `Cpu::tick` and speed-switch stall ticking to pass known divider deltas
  directly (`cpu_cycles` and `dot_cycles`) instead of re-deriving them via
  `curr.wrapping_sub(prev)` in each subsystem.

Behavior remains unchanged: divider start phase and elapsed step counts are
identical to the previous implementation; this is a call-interface simplification.

**Verification:**

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

---

## MMU — Memory-Mapped I/O (~3–5 %)

### 14. Fast-path common address ranges ✅

`read_byte_inner` uses a large `match` on `addr`. The most common reads in a Game Boy
program are:

1. **ROM (0x0000–0x7FFF)** — instruction fetches
2. **HRAM (0xFF80–0xFFFE)** — stack and scratch
3. **WRAM (0xC000–0xDFFF)** — general RAM

**Technique (BGB, Gambatte):** Use a read/write function pointer table indexed by the
high nibble (or high byte) of the address. This replaces the match cascade with a single
indirect call:

```rust
type ReadFn = fn(&mut Mmu, u16) -> u8;
static READ_TABLE: [ReadFn; 16] = [ /* one handler per 4K page */ ];

fn read_byte(&mut self, addr: u16) -> u8 {
    READ_TABLE[(addr >> 12) as usize](self, addr)
}
```

However, Rust's match on `u16` ranges already compiles to efficient binary search or table
dispatch. Profile before implementing — the marginal gain may be small given the existing
match structure.

Completed in [mmu.rs](crates/vibe-emu-core/src/mmu.rs):

- Added early-return fast paths in `read_byte_inner` for common hot ranges:
  ROM (`0x0000..=0x7FFF`, preserving boot ROM overlay behavior),
  HRAM (`0xFF80..=0xFFFE`), and WRAM/ECHO (`0xC000..=0xFDFF`).
- Kept existing match-based behavior for all other address regions unchanged.

**Verification:**

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 15. Inline DMA-active check ✅

The DMA check at the top of `read_byte_inner` runs on every single read:

```rust
if !allow_dma && self.dma_cycles > 0 { … }
```

This is a well-predicted branch (DMA is active for only ~640 cycles per transfer). Mark
with `#[cold]` on the DMA conflict path or use `unlikely()` hints via `core::intrinsics`
(nightly) or the `likely_stable` crate.

Completed in [mmu.rs](crates/vibe-emu-core/src/mmu.rs):

- Split DMA-blocked read handling into a dedicated `#[cold]` helper
  (`read_byte_dma_blocked_value`).
- `read_byte_inner` now uses a compact fast branch and only enters the cold path
  when an active OAM DMA read conflict must be resolved.

Behavior remains unchanged: the same blocked-access and ROM bus-conflict
results are returned for each address class.

**Verification:**

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 16. Reduce env flag checks in I/O paths ✅

`env_flag_enabled` in [mmu.rs](crates/vibe-emu-core/src/mmu.rs) uses `OnceLock` +
`HashMap` lookup on every call. While the `OnceLock` initialization is one-time, the
`HashMap::get` on a 3-entry map runs on every OAM/VRAM access. Replace with individual
`OnceLock<bool>` statics (matching the PPU's `define_env_bool_false!` pattern) to avoid
the hash lookup:

```rust
fn vibeemu_trace_oambug() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| /* parse env */)
}
```

Or, preferably, gate these behind `#[cfg(feature = "trace")]` so they don't exist in
release builds at all.

Completed in [mmu.rs](crates/vibe-emu-core/src/mmu.rs):

- Replaced generic `OnceLock<HashMap<...>>` env-flag cache lookup with dedicated
  per-flag `OnceLock<bool>` helpers (`trace_oambug_enabled`,
  `trace_lcdc_enabled`).
- Updated all MMU trace call sites to use these direct boolean helpers.

This removes per-call hash lookups while preserving one-time env parsing semantics.

**Verification:**

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

---

## Timer (~3 %)

### 17. Timer fast-path for common TAC configurations ✅

`Timer::step` loops per CPU cycle. For TAC modes with slow tick rates (mode 0: DIV bit 9,
triggers every 1024 CPU cycles), hundreds of loop iterations pass without a falling edge.
Compute the next falling-edge cycle and skip to it:

```rust
let next_edge = self.next_falling_edge_cycle();
let skip = (next_edge - (self.div & mask)).min(cycles);
self.div = self.div.wrapping_add(skip);
remaining -= skip;
```

**Caveat:** Must handle the reload delay state machine correctly. When `pending_reload` is
`None` and TAC is disabled, the skip is trivially `cycles` (just advance DIV).

Completed in [timer.rs](crates/vibe-emu-core/src/timer.rs):

- Added a conservative fast path in `Timer::step` for the common case where TAC is
  disabled and there are no pending reload/TMA-latch timing side effects.
- In that state, the timer now advances DIV in one wrapping add and exits early,
  avoiding the per-cycle loop.

Behavior remains unchanged for active timer modes and reload windows; those paths
continue to use cycle-accurate stepping.

**Verification:**

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

---

## General — Cross-cutting Optimizations

### 18. Dirty flags for derived state ✅

Several computed values are recomputed every time they're accessed:

- **PPU palette colors:** Recompute on register write, not on pixel read (see #3).
- **PPU mode-3 target cycles:** Recomputed from sprite positions. Cache and invalidate
  when OAM or LCDC changes.
- **APU PCM registers:** Lazy evaluation (see #7).
- **STAT IRQ line:** `update_stat_irq` is called on every dot. Use a dirty flag set by
  mode/LYC changes and only recompute when dirty.

Completed in [ppu.rs](crates/vibe-emu-core/src/ppu.rs):

- Added `stat_irq_dirty` tracking and a guarded updater
  (`refresh_stat_irq_if_dirty`) so STAT IRQ recomputation only runs when mode/LYC
  or STAT-enable state changes (plus the DMG mode-2→VBlank one-shot path).
- Marked STAT state dirty at mode transitions (`set_mode`), LYC compare changes,
  and `FF41` writes.

Behavior remains unchanged: IRQ assertion still follows the same coincidence/mode
rules and DMG glitch handling, while avoiding redundant recomputation in steady-state dots.

**Verification:**

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 19. Avoid `VecDeque` allocation in dot-fetcher path ✅

`render_dmg_bg_window_scanline_with_mode3_fetcher` at [ppu.rs L6449](crates/vibe-emu-core/src/ppu.rs#L6449)
uses `std::collections::VecDeque`. If this is allocated per call (per scanline), it causes
heap allocation in a hot path. Use a fixed-size ring buffer (e.g., `[u8; 16]` with head/
tail indices) stored as a field in `Ppu` to avoid allocation entirely.

Completed in [ppu.rs](crates/vibe-emu-core/src/ppu.rs):

- Replaced the DMG mode-3 fetcher local `VecDeque<u8>` with a fixed-size stack
  ring buffer (`BgFifo`) using `[u8; 32]` plus head/length indices.
- Kept existing FIFO operations/ordering (`push_back`, `push_front`, `pop_front`,
  `clear`, `len`) to preserve fetcher behavior while removing per-scanline heap work.

**Verification:**

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 20. Profile-guided optimization (PGO) ✅

Rust/LLVM supports PGO, which can improve branch prediction layout and inlining decisions
based on actual execution profiles. For an emulator with highly predictable hot paths,
PGO typically yields 10–20 % overall speedup:

```bash
# Instrumented build
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" cargo build --release
# Run representative workload
./target/release/vibe-emu-ui --headless --frames 3600 rom.gb
# Merge profiles
llvm-profdata merge -o /tmp/pgo-data/merged.profdata /tmp/pgo-data/
# Optimized build
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data/merged.profdata" cargo build --release
```

Completed with a verified local PGO pipeline run (Windows/PowerShell):

- Confirmed `llvm-profdata` tool availability.
- Built instrumented release binary with `-Cprofile-generate`.
- Ran representative headless workload with an in-repo ROM
  (`polishedcrystal-debug-3.2.1.gbc`) for 1200 frames.
- Merged generated `*.profraw` into `target/pgo-data/merged.profdata`.
- Built release binary with `-Cprofile-use` (using an absolute profile path).

Note: a relative `profile-use` path initially failed for dependency crate build
contexts; using an absolute profile path resolved this.

**Verification:**

- `Get-Command llvm-profdata`
- `cargo build --release -p vibe-emu-ui` with `RUSTFLAGS=-Cprofile-generate=...`
- `target/release/vibe-emu-ui.exe --headless --frames 1200 polishedcrystal-debug-3.2.1.gbc`
- `llvm-profdata merge -o target/pgo-data/merged.profdata target/pgo-data/*.profraw`
- `cargo build --release -p vibe-emu-ui` with `RUSTFLAGS=-Cprofile-use=<absolute path>`

### 21. Link-time optimization (LTO) ✅

Ensure `Cargo.toml` enables thin or fat LTO for release builds. Cross-crate inlining is
critical when `Cpu::tick` calls into `Ppu::step`, `Apu::tick`, etc. — all in`vibe-emu-core`
so intra-crate, but LTO helps the linker make final inlining decisions:

```toml
[profile.release]
lto = "thin"    # or "fat" for maximum optimization
codegen-units = 1
```

Completed in [Cargo.toml](Cargo.toml):

- Added workspace release profile settings:
  - `lto = "thin"`
  - `codegen-units = 1`

These settings apply to release workspace builds and improve cross-crate inlining
and final link-time optimization quality.

**Verification:**

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 22. `#[inline]` / `#[inline(always)]` on critical micro-functions ✅

Small functions like `peek_sample`, `compute_output`, `dmg_shade`, `decode_cgb_color`,
`bg_tile_row_plane_addr`, and `signal_with` should be `#[inline]` (many already are).
Verify that the compiler is actually inlining them in the release build — function call
overhead in a per-pixel or per-dot loop is significant.

Completed in [ppu.rs](crates/vibe-emu-core/src/ppu.rs),
[apu.rs](crates/vibe-emu-core/src/apu.rs), and
[timer.rs](crates/vibe-emu-core/src/timer.rs):

- Added targeted `#[inline]` attributes where missing on hot micro-functions,
  including `decode_cgb_color`, channel `compute_output`/`peek_sample`, and
  timer `signal_with` helpers.
- Kept existing `#[inline(always)]` usage (e.g., `dmg_shade`) unchanged.

Release assembly was emitted and spot-checked for targeted symbol mentions as an
inline sanity check.

**Verification:**

- `cargo rustc -p vibe-emu-core --release --lib -- --emit=asm`
- spot-check `target/release/deps/vibe_emu_core-*.s`
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

### 23. Conditional compilation for trace/debug code ✅

All `VIBEEMU_TRACE_*` blocks should be behind `#[cfg(feature = "ppu-trace")]` or similar
feature flags. Even with `OnceLock`-cached env checks, the generated code includes the
branch, the formatting machinery, and prevents certain loop optimizations. When compiled
out, LLVM can vectorize/unroll inner loops more aggressively.

Completed in [ppu.rs](crates/vibe-emu-core/src/ppu.rs) and
[mmu.rs](crates/vibe-emu-core/src/mmu.rs):

- Added feature-gated trace helper definitions for PPU/MMU trace flags so
  non-`ppu-trace` builds compile those checks to constant `false`.
- Added feature-gated fallback implementations for trace line/frame filters
  and object-debug line checks (`true` for filters, `false` for trace enables)
  so non-trace builds avoid trace env parsing/filter code.
- Kept trace behavior unchanged when the `ppu-trace` feature is enabled.

This compiles trace/debug gating out of non-trace builds while preserving
existing trace workflows behind the feature flag.

**Verification:**

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo test --release`

---

## Priority-Ranked Summary

| Priority | Subsystem | Optimization | Est. Impact |
|:---:|---|---|---|
| 1 | APU | ✅ Lazy PCM register refresh (#7) | ~12–17 % recovery |
| 2 | PPU | ✅ Eliminate per-scanline `env_bool_or_false` calls (#1) | ~3–5 % (measured in perf as `getenv`) |
| 3 | PPU | ✅ Batch-skip HBLANK/VBLANK dots in `Ppu::step` (#2) | ~10–15 % (removes ~70 % of dot iterations) |
| 4 | APU | ✅ Batch 1 MHz ticks (#8) + sweep tick countdown (#9) + sample generation batching (#10) | ~5–8 % |
| 5 | PPU | ✅ Precomputed palette color tables (#3) | ~3–5 % |
| 6 | General | ✅ Compile out trace code in release (#23) | ~2–4 % |
| 7 | Timer | ✅ Fast-path timer step (#17) | ~2–3 % |
| 8 | PPU | ✅ VecDeque → fixed ring buffer (#19) | ~1–2 % |
| 9 | General | ✅ PGO + LTO (#20, #21) | ~10–20 % overall |
| 10 | MMU | ✅ Inline DMA check + env flag cleanup (#15, #16) | ~1 % |

---

## References

- **SameBoy** (Lior Halphon) — PPU dot-skip in HBLANK/VBLANK, lazy palette cache,
  lazy PCM. <https://github.com/LIJI32/SameBoy>
- **Gambatte** (sinamas) — tile row caching, palette precomputation, timer fast path,
  batch M-cycle ticking. <https://github.com/sinamas/gambatte>
- **mGBA** (endrift) — event-driven scheduler replacing per-tick loops, ahead-of-time
  DMA scheduling. <https://github.com/mgba-emu/mgba>
- **BGB** (beware) — function pointer dispatch tables for memory map, PGO-tuned
  instruction dispatch. <https://bgb.bircd.org/>
- **Pandocs** — canonical Game Boy hardware reference for PPU/APU/timer timing
  constraints. <https://gbdev.io/pandocs/>
- **"Writing a Game Boy Emulator"** (Antonio Vivace) — covers common optimization
  patterns. <https://github.com/avivace/awesome-gbdev>
- **"Cycle-Accurate Game Boy Docs"** (AntonioND) — PPU FIFO timing, mode 3 variable
  length. <https://github.com/AntonioND/giibiiadvance/blob/master/docs/TCAGBD.pdf>
