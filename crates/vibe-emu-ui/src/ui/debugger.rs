// Debugger module - placeholder for egui port
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use vibe_emu_core::watchpoints::WatchpointHit;

use crate::ui::code_data::ExecutedInstruction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BreakpointSpec {
    pub bank: u8,
    pub addr: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebuggerPauseReason {
    User,
    Breakpoint,
    Watchpoint,
    Step,
    DebuggerFocus,
}

#[derive(Debug, Default, Clone)]
pub struct DebuggerState {
    breakpoints: BTreeMap<BreakpointSpec, bool>,
    sym_path: Option<PathBuf>,
    status_line: Option<String>,
    pause_reason: Option<DebuggerPauseReason>,
    pending_breakpoints_sync: bool,
}

#[derive(Debug, Default, Clone)]
pub struct DebuggerUiActions {
    pub breakpoints_updated: bool,
    pub breakpoints: Vec<BreakpointSpec>,
}

impl DebuggerState {
    pub fn take_actions(&mut self) -> DebuggerUiActions {
        let updated = std::mem::take(&mut self.pending_breakpoints_sync);
        DebuggerUiActions {
            breakpoints_updated: updated,
            breakpoints: if updated {
                self.breakpoints.keys().copied().collect()
            } else {
                Vec::new()
            },
        }
    }

    pub fn load_symbols_for_rom_path(&mut self, _rom_path: Option<&Path>) {
        // TODO: implement symbol loading
    }

    pub fn note_executed_instructions(&mut self, _instructions: &[ExecutedInstruction]) {
        // TODO: implement instruction tracking
    }

    pub fn ack_debug_cmd(&mut self, _cmd_id: u64) {
        // TODO: implement debug command acknowledgment
    }

    pub fn note_breakpoint_hit(&mut self, _bank: u8, _addr: u16) {
        self.pause_reason = Some(DebuggerPauseReason::Breakpoint);
    }

    pub fn note_watchpoint_hit(&mut self, _hit: &WatchpointHit) {
        self.pause_reason = Some(DebuggerPauseReason::Watchpoint);
    }

    pub fn set_pause_reason(&mut self, reason: DebuggerPauseReason) {
        self.pause_reason = Some(reason);
    }
}
