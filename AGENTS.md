# AGENT Instructions

This repository contains **vibeEmu**, a cycle-accurate Game Boy / Game Boy Color emulator written in Rust. The core emulation
lives in `src/` (modules for the CPU, MMU, PPU, APU, timer, cartridge, input, serial, audio, and the `GameBoy` facade). The
`src/ui/` module provides the `winit` + `pixels` frontend, while integration tests reside under `tests/`. Assets such as logos
and screenshots live in `gfx/` and `extra_screenshots/`, and a patched copy of `imgui-wgpu` is vendored under `vendor/`.

## Before you write code
- Read `README.md`, the relevant modules, and existing tests to understand the current architecture and command-line options.
- Keep changes scoped to the subsystem you are improving. Coordinate updates that touch shared scheduling/state (e.g.
  `gameboy.rs`, `hardware.rs`, or timing-critical code) to avoid regressions in other modules.
- Maintain the separation between platform-agnostic emulator logic and platform-specific frontends. Windowing, input, and audio
  plumbing should stay outside of the core emulation modules.

## Coding guidelines
- Use idiomatic Rust (edition 2024). Document new public APIs with Rustdoc where it clarifies hardware behaviour and annotate
  tricky timing or hardware quirks with inline comments.
- If you add dependencies, update `Cargo.toml`, `Cargo.lock`, and any related documentation. Changes to vendored crates should be
  isolated to `vendor/` with a clear explanation of why the patch is necessary.
- Update documentation or CLI help when you change user-facing behaviour.

## Testing & tooling
Run the full suite whenever you touch Rust code or modify build configuration:
1. `cargo fmt --all`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test`
4. `cargo test --release`
5. `python scripts/update_test_status.py` (updates `TEST_STATUS.md` with the latest pass/fail data, including ignored ROM suites)

Integration tests automatically download required ROM bundles into `test_roms/` on first run; ensure network access is available
and leave the archive intact for subsequent runs. Investigate failing third-party ROMs instead of deleting or disabling tests.

## Additional expectations
- Add or update unit/integration tests when fixing bugs or introducing behaviour, especially around CPU/APU/PPU timing and other
  hardware edge cases.
- Keep commits and PRs focused, and use descriptive prefixes such as `ppu: fix mode 3 timing`. Reference the tests you ran in the
  PR description.
- Preserve determinism where possible—avoid introducing sources of randomness that would break reproducibility.

Following these guidelines keeps vibeEmu maintainable, accurate, and easy for future contributors to navigate.
