# SGB hybrid modes and frontend settings

## Research and behavior

[BGB's manual](https://bgb.bircd.org/manual.html#menu) distinguishes two modes:

| Mode | Startup and gameplay | Expected use |
| --- | --- | --- |
| SGB + GBC | CGB hardware and boot registers, with a live SGB command host | Games that issue SGB commands while running in CGB mode |
| GBC + initial SGB border | First run SGB startup until a border is uploaded, then start CGB gameplay with that border | Games that skip SGB initialization after detecting CGB hardware |

These are different fictional combinations, not physical hardware revisions.
The simultaneous mode deliberately does not force a game's CGB path to issue
SGB commands. A game that branches around those commands needs initial-border
mode instead. Native CGB colors are preserved in both modes. Monochrome games
in simultaneous mode can still use SGB colorization and multiplayer.

[SameBoy's `GB_borrow_sgb_border`](https://github.com/LIJI32/SameBoy/blob/master/Core/gb.c)
uses a temporary SGB machine, a separate SGB boot ROM, and a 600-frame timeout.
vibeEmu follows that isolation approach and timeout. Its existing post-boot
initialization also permits capture without a boot ROM. The main CGB machine
is untouched during capture: only the border graphics, tile map, palettes and
backdrop are imported. The donor has a private cartridge/RAM copy, no save
paths, no audio output and no connected devices. A dot budget also bounds
malformed ROMs that never produce a frame. Timeout or no eligible dual-mode
header leaves normal CGB output; a CPU fault terminates capture early.

The border is captured after PCT_TRN has consumed its LCD transfer, not when
the command packet arrives. CGB gameplay cannot subsequently change that
border, send SGB palette/mask commands or detect SGB multiplayer. Reset retains
the captured border; loading another cartridge starts a fresh capture. The
simultaneous mode retains a live command host across reset instead.

## Desktop UI audit

| Area | Behavior and coverage |
| --- | --- |
| Model selection | All seven hardware models, both named hybrid modes, and automatic preferences for either GBC or SGB. Legacy saved `auto` retains Game Boy/GBC-only selection. |
| Boot ROM paths | Separate browse/edit/clear fields for DMG, MGB, SGB, SGB2, CGB, AGB0 and AGB. Existing DMG/CGB saved settings remain compatible. |
| Boot ROM loading | One loader shared by CLI and interactive loads; 256-byte monochrome dumps and 2048/2304-byte color dumps validated. Compact color dumps receive the omitted header hole before mapping. Invalid files produce an error instead of silently skipping boot. |
| Hybrid boot ROMs | Initial-border mode uses the SGB slot for capture and CGB slot for gameplay. `--sgb-bootrom` and `--bootrom` override these independently. The UI identifies active overrides and can clear them. |
| Applying settings | Model/boot changes take effect on ROM load. An explicit apply/reload action saves cartridge RAM before reopening the ROM. Reset uses current settings. |
| Loading | Interactive loading/capture runs on a worker, with a visible loading status and recoverable error dialog. Boot settings remain accessible after a saved-path startup error. |
| Border display | Show/hide setting applies to both display and PNG captures. Cropping retains the composed viewport's SGB palettes, masks and border overlay pixels. Window sizing follows 160x144 or 256x224. |
| Scaling | Existing 1x–6x choices plus working fullscreen integer and fit-to-screen options. Saved fullscreen choices no longer collapse to 2x. Filters use the selected visible dimensions. |
| Controllers | Keyboard controls player 1; connected gamepads have stable SGB player slots shown in Keybinds. Input routing follows the running hardware, not an unapplied model preference. Initial-border gameplay uses ordinary CGB input. |
| Reset/reload peripherals | Attached serial endpoints survive reset and model reload, without copying old hardware clock modes or active transfers. User DMG palette selection survives reset. |
| Debugging | Raw LCD framebuffer and PPU debugger remain separate from composed border output. |

This review covers settings needed by the supported models and hybrid modes;
it is not an attempt to reproduce every BGB debugger or visual filter option.
Game Boy audio works in all modes. The high-level SGB host does not implement
SNES sound commands, uploaded SNES programs, OBJ_TRN, BIOS menus or animations.
No SNES BIOS setting is offered because there is no SNES CPU/audio backend.
The SGB **boot ROM** setting is for the Game Boy CPU inside the SGB adapter.

## Android UI audit

Android exposes the same seven models, two hybrid modes and automatic
preferences. Existing DMG/CGB preference IDs and imported boot filenames remain
compatible. Each model has its own import, enable and clear controls; imports
validate size before replacing an existing image. The SGB and CGB boot slots
are used independently for initial-border capture and gameplay. Model/boot
changes apply on load, with an explicit reload button.

Loading and border capture run on the emulation worker with a progress dialog
and recoverable errors. Returning from a picker does not resume gameplay while
Options is open. The JNI framebuffer, portrait/landscape/TV layouts and scaling
support both 160x144 and 256x224. Hiding the border crops the composed viewport.
Frame pacing follows the running model's clock, including the faster SGB clock.

Four connected controllers have stable player slots shown in Input settings.
Disconnecting releases that player's buttons without moving other players.
Touch and keyboard control player 1; ordinary GB/CGB and initial-border modes
combine connected controllers into player 1. Analog directions and D-pad
buttons retain independent state. Back opens the gameplay menu on phones and
TV; controllers navigate settings while gameplay input is paused. Reset
preserves the running model, boot ROM
and captured border; settings changes do not silently rebuild the machine.

## Verification

`tests/sgb.rs` includes a source-generated dual-mode ROM that uploads a border
only on its SGB path, mutates donor RAM, and chooses a different CGB code path.
Tests cover capture with and without a donor boot ROM, complete border pixels,
CGB colors, native JOYP behavior, resets, timeout, bad headers and CPU faults.
The simultaneous mode is tested independently for colors, masks, controllers
and live border transfers. SameSuite's two SGB multiplayer ROMs run on SGB,
SGB2 and simultaneous CGB/SGB. UI tests cover every model's boot selection,
independent overrides, compact dumps, missing files, automatic preferences,
CLI aliases and backward-compatible settings serialization.

Android native tests cover every model and boot slot, compact boot mapping,
initial-border capture with separate boot ROMs, reset, frame dimensions and
cropping, independent input and fault termination. JVM tests cover persisted
IDs, boot sizes, packed frame dimensions, controller disconnects and mixed
analog/button directions. Android builds package arm64-v8a, armeabi-v7a and
x86_64 native libraries.

Validation: workspace formatting, Clippy with warnings denied, debug and
release test suites, plus Android `assembleDebug`, `testDebugUnitTest` and
`lintDebug`. The full Gambatte run retains the existing baseline of 1193
passes, 199 failures and 2037 ignored tests, with no new failures. Frontend
validation here is through builds, tests and code review; no physical Android
device or interactive visual check was performed.
