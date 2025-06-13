# AGENT Instructions

This repository contains a small Game Boy emulator written in Rust. It is organized with a clear separation between the emulation core and platform-specific frontends, and follows the guidance laid out in the `TODO.md` at the root of the repository.

When making changes:

- **Consult the TODO.md**: Always review `TODO.md` for module responsibilities, implementation priorities, and architectural guidelines before starting work.
- **Modular development**: Assign each AI agent a single module or feature (e.g., CPU, PPU, APU, MMU, Cartridge/MBC, Timer, I/O). Ensure your PR only affects the intended module(s).
- **Follow the design roadmap**: Implement modules in the order specified: project skeleton → cartridge loading → CPU core → MMU integration → PPU → APU → timer & interrupts → input/serial → frontend integration.
- **Maintain cycle accuracy**: When coding core logic (CPU, PPU, APU, timer), verify timing against Pan Docs and Blargg test suites. Write unit tests for opcode timing, PPU mode durations, and APU frame sequencer steps.
- **Adhere to style and tooling**:
  - Format all Rust sources with `cargo fmt --all`.
  - Lint with `cargo clippy` and address warnings before merging.
  - Run `cargo test` in both debug and release profiles; ensure no failures.
- **Front-end separation**: Decouple PPU frame buffer logic from rendering; use `minifb`, `pixels`, or `winit` + `pixels` for windowing. Keep core modules free of OS-specific calls.
- **PR and commit guidelines**:
  - Prefix commits with module name (e.g., `ppu: implement OAM scan`).
  - Reference corresponding TODO.md task (e.g., `TODO.md #PPU: Mode 2 timing`).
  - Include a summary of changes and any new tests or benchmarking results.
- **Testing strategy**:
  - Add unit tests for new functions or modules.
  - Integrate Blargg and MooneyeGB test ROMs where applicable; document any unsupported quirks.
  - Automate CI to run tests, benchmarks, and formatting checks.
- **Documentation and comments**:
  - Document public structs and methods with Rustdoc comments.
  - Add `// TODO:` notes for incomplete behaviors, referencing the task in `TODO.md`.
- **Continuous integration**:
  - Ensure the CI pipeline installs dependencies (e.g., `libasound2-dev` for audio); when adding any new install dependencies, update the project’s setup documentation or scripts and note the change in your PR. Provide clear instructions on environment setup for future runs, then run builds, tests, and lints.
  - Update workflow files when adding new crates or build steps.

By following these instructions, AI agents will maintain code quality, consistency with the project's architecture, and alignment with the comprehensive guidance in `TODO.md`. Merge only after all checks pass and a core maintainer review is complete.

