# UI roadmap implementation status

These changes implement part of the supplied four-phase roadmap. They do **not** complete all
four phases. The lists below distinguish working changes from outstanding work;
unimplemented states, rewind, cheats and peripherals are not advertised in menus.

## Delivered in this change

### Shared foundation and desktop

- A platform-neutral `vibe-emu-frontend` crate defines versioned preferences,
  settings categories, action identifiers/availability and opposing-input policy.
- Desktop keeps direct opening, drag-and-drop, CLI launching and recent files.
  Recent files can be pinned and unpinned; clearing recent files keeps pins.
- File, Emulation, View, Tools, Settings and Help menus; a persistent Play/Develop
  switch; a context menu over the game image; an Escape gameplay menu.
- Cascading submenus retain native hover/keyboard behavior. Their content is
  bounded to the space beside the parent, with wrapping/scrolling as needed;
  placement tests cover 320, 360, 514 and 1100 logical-pixel windows, compact
  widths and wrapped-label visibility when an open menu is resized.
- Workspace selection survives restart. Switching workspaces retains the machine
  and pause state. Develop exposes the existing CPU/memory debugger, VRAM tools
  and watchpoints in **release builds** as well as development builds.
- Develop has resizable docking for disassembly, registers/stack, memory, video,
  watchpoints and game preview. Compact tabs appear below 780 pixels wide or 420
  pixels of panel height. Validated layouts persist separately in `workspace.json`;
  panels undock into native desktop windows. Closing a panel docks it back.
  Existing floating layouts migrate automatically; detached native tools remain.
  No full execution history is collected during ordinary Play.
- Every panel has Undock/Dock back buttons. Menus and toolbar share application
  and debugger commands, with platform-aware shortcut hints and a Controls
  reference. Global labels align with addresses; local labels use indented short
  names with their leading dot. Symbol substitution includes 16-bit loads and absolute load/store operands.
- Paused memory editing supports hex bytes, including in-memory ROM and boot-ROM
  patches. Source ROM files are never rewritten. Regression tests cover both MBC1
  windows, boot overlays, memory regions and unchanged source bytes.
- Custom speed spans 1–400%. The empty Play screen displays the project logo,
  and a 320-point minimum permits a snug 2× game window.
- Disassembly uses RGBDS-style lowercase mnemonics/registers, bracketed memory
  operands and `hli`/`hld`. Opcode/8-bit/16-bit operand colors appear in both
  disassembly bytes and analyzed memory; readable assembly also has syntax colors.
  Developer settings include an accessibility toggle and custom colors (#422).
  PC following uses the post-step snapshot, measured row sizes and explicit
  instruction boundaries at jump targets, including `$ffff`.
- Desktop panics record a local backtrace and eight bounded frame/step checkpoints
  containing ROM/model, registers, bank/cycles, PC/SP bytes and hardware registers.
  An intentional subprocess panic verifies capture while the emulator is locked.
  The reported SGB2 crash has not been reproduced: a 36,000-frame title-screen run
  and a separate controller-input probe over roughly 33,000 frames exited normally.
  These diagnostic checkpoints are not save states.
- Video inspectors scroll in both directions; wide memory/disassembly rows scroll
  horizontally. Debugger controls wrap, and narrow Settings windows use a category
  picker. Passive inspection reads preserve watchpoints, bus latches, OAM, palette
  indices and audio state; regression checks cover all seven model families and
  compare execution with and without repeated frontend snapshots.
- Left Shift and Right Shift are individually assignable in Controls. Physical
  events retain their side before reaching egui; bindings persist across restarts
  and focus loss releases held keys. Existing bindings retain their values.
- Settings moved out of the main entrypoint into searchable categories. General,
  Audio, Video and Controls offer category defaults. Machine settings show pending
  changes, explicit reload and discard; existing CLI boot overrides remain visible.
- Existing model/hybrid/boot/border behavior is preserved; implemented DMG and CGB
  revisions are selectable. A four-color editor offers grayscale, LCD-green and
  endpoint interpolation without replacing game-supplied CGB/SGB palettes.
- Output volume and mono controls, normal-speed presets and capped fast-forward.
  Paused/altered-speed output is muted without disabling emulated APU behavior.
- Screenshot destination selection and visible capture errors.
- Focus loss releases keyboard input; background controller input is opt-in.
  Optional automatic focus pausing only resumes a game it paused itself. Text
  editing, remapping and gameplay menus release game input. Opposing directions
  cancel for every SGB player.
- New installations use Escape for gameplay navigation; explicitly saved legacy
  Quit bindings remain intact. Resetting Controls installs the new defaults.
- Existing TOML, binding and save paths are retained. New fields have defaults and
  preference bounds are normalized on load.

### Android

- Settings extracted into their own composable module, with matching desktop
  category names, option-keyword search and two-pane navigation at 840 dp of available width.
- Durable pending model/boot-ROM settings, Apply and reload, and Discard. Imported
  boot images are immutable references so replacing/clearing an image can be
  discarded without losing the original. Legacy boot files and save paths remain.
  Failed machine preparation or settings commit retains the current native machine.
- One gameplay menu across orientations: a phone bottom sheet and a wider/TV
  dialog. Back/Menu navigation and visible buttons reach the same destinations.
- Search, recent/name/favorite sorting, favorites and last-played instance metadata.
  Existing import, replacement, rename, export and deletion remain accessible.
- Sound, volume, mono and speed controls; altered-speed audio is muted.
- Touch controls can hide when controllers connect; TV has no touch overlay.
- Alphabetic keyboards that advertise a D-pad no longer occupy controller slots
  or hide touch controls. Landscape centers GB/SGB images between the controls;
  touch inputs release when the layout or controller visibility changes. Input,
  placement and wrapped SGB sizing are tested on phone and tablet dimensions.
  The manifest supports the TV launcher without requiring a touchscreen.
- Native libraries cover ARM64, ARMv7, x86-64 and x86, including 32-bit TV
  emulators. The Gradle native task tracks the ABI list as an input.
- Activity visibility and focus jointly gate execution. Focus loss releases all
  player inputs. A recreated process returns to instances if no machine survives;
  configuration changes retain the ViewModel-owned emulator.
- Battery saves flush every 30 seconds and when leaving/backgrounding gameplay.
- Instrumented Compose navigation tests run on actual emulator configurations;
  unit tests cover input neutralization and conservative preference defaults.

## Remaining Phase 1 exit requirements

Phase 1 is not yet complete. In particular:

- Finish drag/rearrangement and minimum-window visual checks. Compact tabs, memory
  horizontal scrolling and Settings navigation were checked at 200% UI scale. The
  final release app was also checked in a 602?510 window, and Settings at 438 pixels wide.
- Complete command registration for every debugger action, customizable desktop
  application/controller bindings, and full keyboard-only navigation coverage.
- Android typed JNI frontend operations/capability metadata and complete
  lifecycle/controller/TV validation.
- Responsive Android instance details and visual QA with
  large text, rotation, split windows, controller reconnects and picker cancellation.
- Measured before/after Play-mode performance and comprehensive release smoke tests.

## Remaining Phase 2

- Global/game overrides with content-derived ROM identity, inherited-value UI,
  comprehensive migrations and category defaults for every supported category.
- Controller profiles/player assignment/remapping on desktop, arbitrary command
  shortcuts, rapid fire, motion sources and editable Android touch layouts/haptics.
- Complete palette import/export, LCD-off colors, aspect/scaling/border policies,
  screenshot raw/filtered/border/copy/share choices and Android thumbnails.
- Desktop memory/register-edit history, conditional break/watch expressions,
  symbol navigation, diagnostic console and opt-in hardware violation events.
- Audio device/rate/latency/channel monitoring and dirty-aware periodic saving.
- Battery-save conflict preservation and safe explicit import/export on desktop.

## Phase 3: not implemented

Save states are deferred for now. When implemented, **capturing and restoring
cartridge SRAM is enabled by default** on desktop and Android, including every
RAM bank and mapper-specific persistent memory. A state load must restore the
captured SRAM into the running cartridge, not silently retain newer SRAM from
the current session or a battery-save file. Preserve the pre-load SRAM candidate
for undo/conflict recovery; subsequent battery-save flushing must use the restored
contents. Tests must save a state, change SRAM, load the state, and verify all RAM
banks and the subsequently flushed battery save match the captured state. Any
future preserve-current-SRAM override must be explicit and opt-in.

Implement complete core-owned, versioned machine snapshots first. Validation must
cover every hardware model and mapper, CPU/MMU/PPU/APU/timer/DMA/serial/input state,
RTC, SGB and hybrid-border state, malformed input, wrong ROM/model and atomic writes.
Existing debugger snapshots and battery saves are not save states.

Only after that foundation passes round-trip/determinism tests should the UI expose
ten numbered slots, separate quick/recovery slots, metadata, import/export and
undo-load. Rewind needs a bounded snapshot budget. Reverse execution additionally
needs deterministic event recording and replay. External link/Mobile Adapter
sessions must reject restoration with an explicit reason. Cheats and cheat search
also remain to be implemented.

## Phase 4: not implemented

Advanced display/color/audio effects, WAV/stem and PNG-sequence recording, shared
network link transport, paired local machines, printer, camera, MBC7 motion/rumble,
infrared, SGB PAL and replacement boot ROMs remain outstanding. Existing link cable
and Mobile Adapter controls remain available. Features explicitly deferred by the
supplied roadmap remain outside these four phases.

## Validation

Completed for this change:

| Check | Result |
| --- | --- |
| `cargo fmt --all` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test` | Passed |
| `cargo test --release` | Passed |
| `cargo gambatte_test` | 1,193 passed, 199 failed, 2,037 ignored; the unchanged HEAD baseline has the exact same failure set (no new failures) |
| Clean-cache Daid image provisioning | All 8 tests passed after removing the backed-up cached DMG reference; downloaded bytes match the original |
| Android `assembleDebug`, `testDebugUnitTest`, `lintDebug` | Passed; 7 unit tests |
| Navigation and machine-settings tests on Medium Phone (API 36) | 6 passed after a cold boot; first emulator run suffered a system crash |
| Navigation and machine-settings tests on Medium Tablet (API 35) | 6 passed |
| Navigation and machine-settings tests on Television 1080p (API 36) | 6 passed, including native x86 reload tests |
| Desktop visual smoke checks | Left Shift binding and restart persistence, video scrolling, compact tabs, final release Settings at normal/438-pixel width, Develop at 602?510, and Settings/Controls at 200% scale checked; drag/rearrangement remains unverified because automated drag gestures were unreliable |
| Third-party license manifest | Regenerated for docking and JSON dependencies |

The latest pushed main CI run (`36347765358`, commit `0e2111f7`) failed because
Daid's DMG scanline test requested `ppu_scanline_bgp_1.dmg.png` while test setup
still downloaded `_0`. Setup and the test now share the selected reference path;
PNG-open errors include the path. The expected hardware image is unchanged.
This fix has been verified locally with that reference absent from the cache;
The fix is included in draft PR #435.

Local build/test logs are under `target/validation/`. The Android debug APK is
`android/app/build/outputs/apk/debug/app-debug.apk`.

Required Rust checks are `cargo fmt --all`, workspace Clippy with warnings denied,
`cargo test` and `cargo test --release`. The desktop review adds passive core
inspection APIs without changing CPU read paths; also run `cargo gambatte_test`. Android checks are `assembleDebug`, `testDebugUnitTest`,
`lintDebug` and `connectedDebugAndroidTest`; select an emulator with `ANDROID_SERIAL`.

The instrumented tests use the project's Compose BOM and the
[official Compose testing setup](https://developer.android.com/develop/ui/compose/testing).
They test category/back navigation, option-keyword search, pending-change callbacks,
pause-menu callbacks, durable machine drafts, boot replacement/clear/discard and
native reload failure recovery. They do not yet cover all lifecycle, input-device,
large-text or visual acceptance criteria above. Rust regressions cover layout
validation/persistence and independent Shift sides, focus release and binding-file
round trips. New inspection regressions cover mapper/boot mapping, seven model
families, watchpoint isolation, bus/OAM/palette preservation and equivalent CPU,
memory, video and audio state after repeated frontend snapshots. Save-state SRAM tests remain a Phase 3 requirement.
