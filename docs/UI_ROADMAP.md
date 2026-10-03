# Remaining UI work

## Implementation follow-ups

- [ ] Complete customizable application/controller bindings and keyboard-only navigation.
- [ ] Add typed Android JNI frontend operations and capability metadata.
- [ ] Make Android instance details responsive across window sizes.
- [ ] Measure before/after Play-mode performance with the same ROMs and settings.
- [ ] Add content-derived ROM identities, global/game overrides, inherited-value UI,
  migrations, and defaults for the remaining settings categories.
- [ ] Add controller profiles, player assignment/remapping, rapid fire, motion
  sources, and editable Android touch layouts/haptics.
- [ ] Finish palette import/export, LCD-off colors, aspect/scaling/border policies,
  screenshot raw/filtered/border/copy/share choices, and Android thumbnails.
- [ ] Add memory/register-edit history, conditional break/watch expressions,
  symbol navigation, a diagnostic console, and opt-in hardware violation events.
- [ ] Add audio device/rate/latency/channel monitoring and dirty-aware periodic saves.
- [ ] Preserve conflicting battery saves and add explicit desktop save import/export.

## Save states

- [x] Implement core-owned versioned snapshots for all models and mapper state,
  CPU/MMU/PPU/APU/timer/DMA/serial/input, RTC, SGB and hybrid borders.
- [x] Restore all cartridge SRAM by default; atomically preserve a pre-load
  recovery state for undo and conflict recovery, including after restart.
- [x] Add ten slots, quick/recovery slots, metadata, import/export and undo-load
  on desktop and Android. Reject link/Mobile Adapter sessions in the core.
- [x] Add automated round-trip, continuation, SRAM/recovery, failure-path, desktop
  worker/render and Android JNI/dialog coverage.
- [x] Bound imported audio phases, cover storage/import failure paths, and keep
  States/Help visible at default desktop 2x size and in wrapped layouts.
- [x] Make Android Quick actions discoverable, confirm replacement/load, return to
  gameplay after restore, and retain visible errors and pending work across rotation.
- [x] Offer a validated per-instance resume choice, explicit start without loading,
  and safe cancellation; suggest the latest compatible manual save, excluding Recovery.
- [ ] Complete remaining manual device/picker verification in [Save states](SAVE_STATES.md).

## Verification requiring Matthew or another tester

- [ ] Verify drag/rearrangement and layout restoration with a mouse in the final build.
- [ ] Run native macOS/Linux interactive checks: detached panels, Dock back/close,
  shortcuts, minimum windows, high DPI, and keyboard-only navigation.
- [ ] Finish Android lifecycle/controller/TV checks, including process recreation,
  large text, rotation, split windows, controller reconnects, and picker cancellation.
- [ ] Complete release smoke tests on supported desktop and Android devices.
- [ ] If the SGB2 TRN Stress crash recurs, provide its crash report and reproduction
  steps. The prior title-screen and controller-input probes did not reproduce it.

## Deferred implementation

- [ ] Add bounded rewind, deterministic recording/replay for reverse execution,
  cheats, and cheat search.
- [ ] Add advanced display/color/audio effects, WAV/stem and PNG-sequence recording.
- [ ] Implement network link transport, paired local machines, printer, camera,
  MBC7 motion/rumble, infrared, SGB PAL, and replacement boot ROMs.
