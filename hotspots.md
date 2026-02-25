## Perf Hotspot Analysis (core crates)

## Scope

This report profiles the workspace's core runtime crates using `perf` in this dev container:

- `vibe-emu-core` (directly via integration test workload)
- `vibe-emu-ui` (headless runtime workload)
- `vibe-emu-mobile` path (via `vibe-emu-ui --mobile --headless`)

The repository and profiler environment on this machine are from 2026-02-24.

## Profiling setup

### Environment notes

- `perf` was initially blocked by `kernel.perf_event_paranoid=4`.
- Profiling was enabled for this session with:

```bash
sudo sysctl -w kernel.perf_event_paranoid=1
```

- Kernel symbols remained restricted (`kptr_restrict`), so kernel-side attribution is incomplete.
- Hardware counters (`cycles`, `instructions`) were unavailable in this container; analysis is based on sampled `task-clock` time.

### Commands used

```bash
# core workload (runtime-heavy integration tests)
perf record -F 199 -g --call-graph dwarf -o /tmp/perf-vibe-emu-core.data -- \
  cargo test --release -p vibe-emu-core --test cpu_instrs_rom

# ui workload (headless deterministic run)
perf record -F 199 -g --call-graph dwarf -o /tmp/perf-vibe-emu-ui.data -- \
  target/release/vibe-emu-ui --headless --frames 1200 \
  crates/vibe-emu-core/test_roms/mbc3-tester/mbc3-tester.gb

# mobile path workload (mobile mode enabled)
perf record -F 199 -g --call-graph dwarf -o /tmp/perf-vibe-emu-mobile.data -- \
  target/release/vibe-emu-ui --headless --mobile --frames 1200 \
  crates/vibe-emu-core/test_roms/mbc3-tester/mbc3-tester.gb

# symbol summaries
perf report --stdio -n --no-children -i /tmp/perf-vibe-emu-core.data
perf report --stdio -n --no-children -i /tmp/perf-vibe-emu-ui.data
perf report --stdio -n --no-children -i /tmp/perf-vibe-emu-mobile.data
```

## Hotspot results

### `vibe-emu-core` (cpu_instrs_rom workload)

Samples: ~3.5k (`task-clock`).

| Overhead | Symbol |
|---:|---|
| 26.40% | `vibe_emu_core::ppu::Ppu::step` |
| 17.20% | `vibe_emu_core::apu::Apu::refresh_pcm_regs` |
| 6.54% | `vibe_emu_core::ppu::Ppu::render_scanline` |
| 6.31% | `vibe_emu_core::apu::Apu::step` |
| 6.31% | `vibe_emu_core::apu::Apu::tick` |
| 5.66% | `vibe_emu_core::ppu::Ppu::dmg_bg_color_for_pixel` |

Call-chain view also shows a large aggregate under:

- `Cpu::step -> Cpu::tick -> Ppu::step`
- `Cpu::step -> Cpu::tick -> Apu::tick/step -> refresh_pcm_regs`

### `vibe-emu-ui` (headless)

Samples: 746 (`task-clock`).

| Overhead | Symbol |
|---:|---|
| 33.24% | `vibe_emu_core::ppu::Ppu::step` |
| 12.47% | `vibe_emu_core::apu::Apu::refresh_pcm_regs` |
| 8.98% | `vibe_emu_core::ppu::Ppu::render_scanline` |
| 7.37% | `vibe_emu_core::apu::Apu::tick` |
| 6.03% | `vibe_emu_core::ppu::Ppu::dmg_bg_color_for_pixel` |
| 5.90% | `vibe_emu_core::cpu::Cpu::tick` |
| 4.29% | `vibe_emu_core::ppu::Ppu::dmg_bg_en_for_pixel` |
| 3.75% | `vibe_emu_core::ppu::Ppu::dmg_lcdc_for_bg_fetch_t` |

Notably, nearly all time in this UI headless run still lands in `vibe_emu_core` symbols, which means frontend overhead is minimal for this workload.

### `vibe-emu-mobile` path (`--mobile --headless`)

Samples: 761 (`task-clock`).

| Overhead | Symbol |
|---:|---|
| 33.25% | `vibe_emu_core::ppu::Ppu::step` |
| 14.45% | `vibe_emu_core::apu::Apu::refresh_pcm_regs` |
| 11.30% | `vibe_emu_core::ppu::Ppu::render_scanline` |
| 6.04% | `vibe_emu_core::cpu::Cpu::tick` |
| 5.78% | `vibe_emu_core::apu::Apu::tick` |
| 5.52% | `vibe_emu_core::ppu::Ppu::dmg_bg_color_for_pixel` |
| 3.81% | `vibe_emu_core::ppu::Ppu::dmg_bg_en_for_pixel` |
| 3.29% | `vibe_emu_core::timer::Timer::step` |

The mobile-enabled profile remains dominated by `vibe_emu_core`; no major standalone hotspot from `vibe_emu-mobile` itself surfaced in this bounded headless run.

## Problem areas

### 1) PPU scanline pipeline is the top cost center

Across all workloads, the combined PPU path (`Ppu::step`, `render_scanline`, DMG background pixel helpers) consistently takes the largest share. This is the primary CPU hotspot group and should be optimization target #1.

### 2) APU PCM refresh cost is high and stable

`Apu::refresh_pcm_regs` alone sits around 12-17% depending on workload, plus additional `Apu::tick/step` overhead. This indicates repeated PCM register refresh work is a significant per-tick cost center.

### 3) Per-tick scheduler pressure in `Cpu::tick`

Even when not the top single symbol, `Cpu::tick` is the fan-out site for PPU/APU/Timer stepping and remains prominent in both self and children views.

### 4) MMU DMA/read paths are secondary but persistent

`Mmu::dma_step` and `Mmu::read_byte` show up repeatedly as smaller but non-trivial hotspots. These are likely worthwhile after PPU/APU work.

### 5) Environment lookup appearing in render path

Call graphs include `Ppu::env_bool_or_false -> std::env::_var -> getenv` under rendering. Any environment-variable read in a hot inner loop is a red flag and should be hoisted/cached outside per-pixel/per-scanline execution.

## Recommended optimization order

1. PPU inner loops (`Ppu::step`, `render_scanline`, pixel helper functions).
2. APU PCM refresh frequency and dataflow (`refresh_pcm_regs`).
3. Remove or cache runtime env lookups from rendering paths.
4. Revisit `Cpu::tick` fan-out overhead and MMU DMA/read micro-costs.

## Post-Optimization Results (2025-02-25)

All 14 optimizations from `OPTIMIZATION_RESEARCH.md` were applied on the `optimizations` branch.
All existing tests pass. The same three workloads were re-profiled with identical `perf` parameters.

### Wall-time improvement

| Workload | Baseline samples | Optimized samples | Baseline wall-time | Optimized wall-time | Speedup |
|---|---:|---:|---:|---:|---:|
| `cpu_instrs_rom` | 3 534 | 2 408 | 5.80 s | 4.47 s | **~23%** |
| UI headless | 746 | 514 | — | — | ~31% fewer samples |
| Mobile headless | 761 | 524 | — | — | ~31% fewer samples |

### `vibe-emu-core` (cpu_instrs_rom) — Before vs After

| Symbol | Before | After | Delta |
|---|---:|---:|---:|
| `Ppu::step` | 26.40% | 27.49% | +1.1 pp (flat; others shrank around it) |
| `Ppu::render_scanline` | 6.54% | 11.79% | +5.3 pp (inlined helpers now attributed here) |
| `Apu::tick_steps` (was `Apu::tick`) | 6.31% | 9.39% | +3.1 pp (batched tick work consolidated) |
| `Ppu::dmg_bgp_for_pixel` (was `dmg_bg_color_for_pixel`) | 5.66% | 7.64% | +2.0 pp (cached palette; renamed) |
| `Apu::step` | 6.31% | 7.31% | +1.0 pp |
| `Mmu::dma_step` | — | 4.98% | newly visible |
| `SquareChannel::tick_1mhz_batch` | — | 4.90% | new batched function |
| `Ppu::dmg_lcdc_for_bg_fetch_t` | — | 3.90% | newly visible |
| `Apu::tick_frame_sequencer_steps` | — | 3.74% | new batched function |
| `Cpu::tick` | — | 3.65% | down from ~6% |
| `Cpu::step` | — | 3.32% | marginal |
| `Apu::clock_wave_channel_2mhz_inner` | — | 2.53% | newly visible |
| `Timer::step` | — | 2.08% | stable |
| `Mmu::read_byte` | — | 1.74% | reduced |
| **`Apu::refresh_pcm_regs`** | **17.20%** | **<0.5%** | **eliminated as hotspot** |
| **`Ppu::dmg_bg_en_for_pixel`** | ~4% | **<0.5%** | **eliminated** |
| **env lookups in render path** | visible | **gone** | **eliminated** |

### `vibe-emu-ui` (headless) — After optimization

| Overhead | Symbol |
|---:|---|
| 38.33% | `Ppu::step` |
| 13.04% | `Ppu::render_scanline` |
| 9.53% | `Apu::tick_steps` |
| 6.03% | `Ppu::dmg_bgp_for_pixel` |
| 5.64% | `SquareChannel::tick_1mhz_batch` |
| 4.86% | `Ppu::dmg_lcdc_for_bg_fetch_t` |
| 4.09% | `Cpu::step` |
| 3.89% | `Cpu::tick` |
| 3.70% | `Mmu::dma_step` |
| 2.33% | `Apu::step` |

`refresh_pcm_regs` and `dmg_bg_en_for_pixel` no longer appear. Frontend overhead remains negligible.

### `vibe-emu-mobile` path — After optimization

| Overhead | Symbol |
|---:|---|
| 36.26% | `Ppu::step` |
| 16.41% | `Ppu::render_scanline` |
| 6.68% | `Ppu::dmg_bgp_for_pixel` |
| 6.30% | `Apu::tick_steps` |
| 5.15% | `Ppu::dmg_lcdc_for_bg_fetch_t` |
| 4.39% | `SquareChannel::tick_1mhz_batch` |
| 3.82% | `Apu::step` |
| 3.63% | `Cpu::tick` |
| 3.63% | `Mmu::dma_step` |
| 2.67% | `Apu::tick_frame_sequencer_steps` |
| 2.67% | `Cpu::step` |
| 2.48% | `Mmu::read_byte` |
| 2.29% | `Timer::step` |

No `vibe-emu-mobile`-specific hotspot surfaces; the mobile adapter path remains lightweight.

### Key takeaways

1. **`refresh_pcm_regs` eliminated** — the lazy dirty-flag pattern removed the
   second-largest hotspot entirely (17 % → <0.5 %).
2. **Environment lookups gone** — cached via `OnceLock`, no longer visible in profiles.
3. **`dmg_bg_en_for_pixel` fast path** — event_count == 0 shortcut dropped it below
   measurement noise.
4. **~23 % wall-time speedup** on the cpu_instrs_rom workload (5.80 s → 4.47 s).
5. **PPU remains the dominant cost center** — `Ppu::step` + `render_scanline` +
   pixel helpers still account for ~50 % of self-time across all workloads. Further
   PPU gains would require structural changes (scanline caching, tile-row rendering).
6. **APU work is now spread across batched functions** — `tick_steps`,
   `tick_frame_sequencer_steps`, `SquareChannel::tick_1mhz_batch`, and
   `clock_wave_channel_2mhz_inner` together total ~20 %, but each is individually
   smaller than the old monolithic paths.
7. **Next optimization targets** (if pursued):
   - `Ppu::step` inner loop (27–38 %) — mode-3 pixel FIFO is the remaining hot path.
   - `Ppu::render_scanline` (12–16 %) — tile-row caching or SIMD pixel conversion.
   - APU batched tick functions (~20 % combined) — further coarsening or conditional
     skip when channels are inactive.
   - `Timer::step` (2 %) — skip-ahead when timer is disabled (optimize.md Tier 2.3).

## Confidence and limitations

- Confidence is high for relative user-space hotspots in emulator runtime paths.
- Confidence is lower for kernel/CPU microarchitectural diagnosis due unavailable hardware counters in this container.
- Percentages will vary with ROM and mode (DMG/CGB, audio mix, UI vs headless), but hotspot ranking is very consistent across the measured runs.
- Post-optimization percentages were collected under identical conditions to baseline, enabling direct comparison.
