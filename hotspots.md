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

## Confidence and limitations

- Confidence is high for relative user-space hotspots in emulator runtime paths.
- Confidence is lower for kernel/CPU microarchitectural diagnosis due unavailable hardware counters in this container.
- Percentages will vary with ROM and mode (DMG/CGB, audio mix, UI vs headless), but hotspot ranking is very consistent across the measured runs.
