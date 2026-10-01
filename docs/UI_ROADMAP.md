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

- [ ] Implement core-owned, versioned save states before exposing state controls.
  Cover every model/mapper and CPU/MMU/PPU/APU/timer/DMA/serial/input, RTC, SGB,
  and hybrid-border state; test determinism, invalid data, wrong ROM/model, and atomic writes.
- [ ] Capture and restore all cartridge SRAM by default, including mapper-specific
  persistent memory. Preserve pre-load SRAM for undo/conflict recovery; verify
  save/change/load restores every bank and the subsequently flushed battery file.
  Any preserve-current-SRAM option must be explicit and opt-in.
- [ ] After snapshot tests pass, add ten slots, quick/recovery slots, metadata,
  import/export, and undo-load. Reject restoration during external link/Mobile Adapter sessions.
- [ ] Add bounded rewind, deterministic recording/replay for reverse execution,
  cheats, and cheat search.
- [ ] Add advanced display/color/audio effects, WAV/stem and PNG-sequence recording.
- [ ] Implement network link transport, paired local machines, printer, camera,
  MBC7 motion/rumble, infrared, SGB PAL, and replacement boot ROMs.
