//! Cycle-accurate Game Boy / Game Boy Color emulation core.
//!
//! This crate contains the platform-agnostic emulator logic (CPU/MMU/PPU/APU/etc).
//! Frontends (desktop UI, mobile) live in separate crates and drive the core via
//! the [`gameboy`] facade.

#![allow(non_snake_case)]
#![allow(dead_code)]
#![warn(missing_docs)]

/// Diagnostic logging infrastructure for the emulator core.
pub mod diagnostics;

#[allow(unused_macros)]
macro_rules! core_trace {
	(target: $target:expr, $($arg:tt)*) => {{
		if crate::diagnostics::has_log_sink() {
			crate::diagnostics::emit(crate::diagnostics::Level::Trace, $target, format_args!($($arg)*));
		}
	}};
}

#[allow(unused_macros)]
macro_rules! core_info {
	(target: $target:expr, $($arg:tt)*) => {{
		if crate::diagnostics::has_log_sink() {
			crate::diagnostics::emit(crate::diagnostics::Level::Info, $target, format_args!($($arg)*));
		}
	}};
}

#[allow(unused_macros)]
macro_rules! core_warn {
	(target: $target:expr, $($arg:tt)*) => {{
		if crate::diagnostics::has_log_sink() {
			crate::diagnostics::emit(crate::diagnostics::Level::Warn, $target, format_args!($($arg)*));
		}
	}};
}

/// Audio Processing Unit (APU) emulation.
pub mod apu;

/// Lock-free-ish audio ring buffer used by the APU.
pub mod audio_queue;

/// Cartridge mappers (MBC) and ROM/RAM/RTC handling.
pub mod cartridge;

/// LR35902 CPU core.
pub mod cpu;

/// High-level facade that wires the CPU and MMU into a single machine.
pub mod gameboy;

/// Hardware revisions and revision-specific quirks.
pub mod hardware;

/// Joypad input register and edge-triggered interrupt behavior.
pub mod input;

/// Memory map and hardware plumbing.
pub mod mmu;

/// Optional debugger watchpoints (read/write/execute/jump).
pub mod watchpoints;

/// Pixel Processing Unit (PPU) emulation.
pub mod ppu;

/// Serial unit and link cable plumbing.
pub mod serial;

/// Divider/timer unit.
pub mod timer;
