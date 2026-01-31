# egui Migration Plan

Migration checklist for replacing imgui/pixels with egui and upgrading wgpu per [issue #346](https://github.com/vulcandth/vibeEmu/issues/346).

## Dependencies

- [x] Add `egui` and `eframe` crates with `wgpu` feature enabled
- [x] Add `egui-wgpu` for custom wgpu rendering integration
- [x] Remove `imgui`, `imgui-winit-support`, and `imgui-wgpu` dependencies
- [x] Remove `pixels` crate
- [x] Upgrade `wgpu` from 0.17 to 23.x (via eframe 0.30)
- [x] Remove vendored `imgui-wgpu-0.24.0` patch from workspace `Cargo.toml`

## Framebuffer Rendering

- [x] Create egui texture handle for the 160×144 framebuffer and update each frame
- [ ] Replace `pixels::Pixels` with direct wgpu texture + surface for Game Boy framebuffer
- [ ] Port `GameScaler` shader (`shaders/game_scale.wgsl`) to work with egui's wgpu integration using `egui::PaintCallback`

## Window Management

- [x] Replace `UiWindow` struct with eframe's window/viewport management (stubbed)
- [x] Migrate from `imgui` context to single `egui::Context`
- [x] Replace `WinitPlatform` (imgui-winit-support) with eframe's built-in winit integration

## UI Widgets: Main Window

- [x] Port main menu bar (File, Debug, Emulation, Options)
- [x] Port ROM loading dialogs (integrated with `rfd`)
- [ ] Port Mobile Adapter menu
- [ ] Port keybind capture modal
- [ ] Port status bar / overlay rendering

## UI Widgets: Options Window

- [x] Options window placeholder created
- [ ] Port Keybinds tab with action-key mapping table
- [ ] Port Emulation tab with bootrom path inputs and browse buttons
- [ ] Port window size and scaler options

## UI Widgets: Debugger Window

- [x] Debugger window placeholder created
- [ ] Port disassembly list view with virtual scrolling (replace `imgui::ListClipper`)
- [ ] Port breakpoint markers and toggle UI
- [ ] Port register/state display panels
- [ ] Port memory hex view
- [ ] Port symbol file loading and goto-address input
- [ ] Port step/continue/pause toolbar

## UI Widgets: VRAM Viewer Window

- [x] VRAM Viewer window placeholder created
- [ ] Port BG Map tab with 256×256 texture display
- [ ] Port Tiles tab with tile grid rendering
- [ ] Port OAM tab with sprite list and sprite preview textures
- [ ] Port Palettes tab with color picker grid
- [ ] Replace `imgui_wgpu::Texture` with `egui::TextureHandle` for all VRAM textures

## UI Widgets: Watchpoints Window

- [x] Watchpoints module stubbed with state structures
- [ ] Port watchpoint list table
- [ ] Port add/edit/remove watchpoint UI

## Rendering Pipeline

- [ ] Set up egui-wgpu renderer with correct blend mode (premultiplied alpha)
- [ ] Integrate custom wgpu render passes for game framebuffer via `egui::PaintCallback`
- [ ] Ensure surface format compatibility between egui and game scaler

## Platform Integration

- [x] Update window icon loading to use `eframe::IconData`
- [x] Migrate keyboard/input handling from winit events to egui's input system
- [x] Port keybinds.rs to use `egui::Key` instead of `winit::keyboard::KeyCode`
- [ ] Preserve multi-window support (debugger, VRAM viewer, watchpoints, options)

## Testing

- [x] Project compiles successfully with new dependencies
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [x] `cargo test` passes (all existing tests pass)
- [x] `cargo fmt --all` applied
- [x] Headless mode works (`--headless --frames 10`)
- [ ] Verify all UI windows open and render correctly
- [ ] Verify game framebuffer renders with correct scaling and aspect ratio
- [ ] Verify keyboard input works for both game controls and UI navigation
- [ ] Verify debugger stepping and breakpoints function correctly
