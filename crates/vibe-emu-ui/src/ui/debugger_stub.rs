use crate::ui::{
    code_data::{CodeDataTracker, ExecutedInstruction},
    snapshot::UiSnapshot,
};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};
use vibe_emu_core::watchpoints::{WatchpointHit, WatchpointTrigger};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BreakpointSpec {
    pub bank: u8,
    pub addr: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebuggerRunToRequest {
    pub target: BreakpointSpec,
    pub ignore_breakpoints: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebuggerPauseReason {
    Manual,
    Step,
    DebuggerFocus,
    Breakpoint {
        bank: u8,
        addr: u16,
    },
    Watchpoint {
        trigger: WatchpointTrigger,
        addr: u16,
        value: Option<u8>,
        pc: Option<u16>,
    },
}

#[derive(Debug, Default, Clone)]
pub struct DebuggerUiActions {
    pub request_pause: bool,
    pub request_continue: bool,
    pub request_continue_no_break: bool,
    pub request_continue_ignore_once: Option<BreakpointSpec>,
    pub request_step: Option<u64>,
    pub request_run_to: Option<DebuggerRunToRequest>,
    pub request_jump_to_cursor: Option<u16>,
    pub request_call_cursor: Option<u16>,
    pub request_jump_sp: bool,
    pub request_focus_main: bool,
    pub request_toggle_animate: bool,
    pub breakpoints_updated: bool,
    pub breakpoints: Vec<BreakpointSpec>,
}

#[derive(Debug, Default, Clone)]
pub struct DebuggerState {
    breakpoints: BTreeMap<BreakpointSpec, bool>,
    cursor: Option<BreakpointSpec>,
    pause_reason: Option<DebuggerPauseReason>,
    pending_scroll_to_pc: bool,
    pending_scroll_to_addr: Option<u16>,
}

impl DebuggerState {
    pub fn note_executed_instructions(&mut self, _events: &[ExecutedInstruction]) {}

    pub fn set_pause_reason(&mut self, reason: DebuggerPauseReason) {
        self.pause_reason = Some(reason);
    }

    pub fn request_scroll_to_pc(&mut self) {
        self.pending_scroll_to_pc = true;
        self.pending_scroll_to_addr = None;
    }

    pub fn take_pending_scroll(&mut self) -> Option<u16> {
        if self.pending_scroll_to_pc {
            self.pending_scroll_to_pc = false;
            return Some(u16::MAX);
        }
        self.pending_scroll_to_addr.take()
    }

    pub fn note_breakpoint_hit(&mut self, bank: u8, addr: u16) {
        self.pause_reason = Some(DebuggerPauseReason::Breakpoint { bank, addr });
        self.pending_scroll_to_addr = Some(addr);
        self.pending_scroll_to_pc = false;
    }

    pub fn note_watchpoint_hit(&mut self, hit: &WatchpointHit) {
        self.pause_reason = Some(DebuggerPauseReason::Watchpoint {
            trigger: hit.trigger,
            addr: hit.addr,
            value: hit.value,
            pc: hit.pc,
        });
        self.pending_scroll_to_addr = Some(hit.addr);
        self.pending_scroll_to_pc = false;
    }

    pub fn ack_debug_cmd(&mut self, _cmd_id: u64) {}

    pub fn request_pause(&mut self) {}

    pub fn request_step(&mut self) {
        self.pause_reason = Some(DebuggerPauseReason::Step);
        self.pending_scroll_to_pc = true;
    }

    pub fn request_step_over(&mut self) {}

    pub fn request_run_to_cursor(&mut self) {}

    pub fn request_run_to_cursor_no_break(&mut self) {}

    pub fn request_continue_and_focus_main(&mut self) {
        self.pause_reason = None;
    }

    pub fn request_continue_no_break_and_focus_main(&mut self) {
        self.pause_reason = None;
    }

    pub fn request_run_not_this_break_and_focus_main(&mut self) {
        self.pause_reason = None;
    }

    pub fn request_step_out(&mut self) {}

    pub fn request_jump_to_cursor(&mut self) {}

    pub fn request_call_cursor(&mut self) {}

    pub fn request_jump_sp(&mut self) {}

    pub fn request_toggle_animate(&mut self) {}

    pub fn breakpoints(&self) -> impl Iterator<Item = BreakpointSpec> + '_ {
        self.breakpoints
            .iter()
            .filter_map(|(&bp, &enabled)| enabled.then_some(bp))
    }

    pub fn set_breakpoints_from_emu(
        &mut self,
        breakpoints: impl IntoIterator<Item = BreakpointSpec>,
    ) {
        self.breakpoints = breakpoints.into_iter().map(|bp| (bp, true)).collect();
    }

    pub fn load_symbols_for_rom_path(&mut self, _rom_path: Option<&Path>) {}

    pub fn take_actions(&mut self) -> DebuggerUiActions {
        DebuggerUiActions {
            breakpoints: self.breakpoints().collect(),
            ..DebuggerUiActions::default()
        }
    }

    pub fn status_line(&self) -> Option<&str> {
        None
    }

    pub fn pause_reason(&self) -> Option<DebuggerPauseReason> {
        self.pause_reason
    }

    pub fn add_breakpoint(&mut self, bp: BreakpointSpec) {
        self.breakpoints.insert(bp, true);
    }

    pub fn remove_breakpoint(&mut self, bp: &BreakpointSpec) {
        self.breakpoints.remove(bp);
    }

    pub fn toggle_breakpoint(&mut self, bp: BreakpointSpec) {
        if let Some(slot) = self.breakpoints.get_mut(&bp) {
            *slot = !*slot;
        } else {
            self.breakpoints.insert(bp, true);
        }
    }

    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    pub fn has_breakpoint(&self, bp: &BreakpointSpec) -> Option<bool> {
        self.breakpoints.get(bp).copied()
    }

    pub fn all_breakpoints(&self) -> impl Iterator<Item = (&BreakpointSpec, &bool)> {
        self.breakpoints.iter()
    }

    pub fn set_cursor(&mut self, bp: BreakpointSpec) {
        self.cursor = Some(bp);
    }

    pub fn cursor(&self) -> Option<BreakpointSpec> {
        self.cursor
    }

    pub fn lookup_symbol(&self, _name: &str) -> Option<(u8, u16)> {
        None
    }

    pub fn first_label_for(&self, _bank: u8, _addr: u16) -> Option<&str> {
        None
    }

    pub fn parse_breakpoint_input(
        &self,
        input: &str,
        snapshot: &UiSnapshot,
    ) -> Option<BreakpointSpec> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        if let Some((bank_str, addr_str)) = trimmed.split_once(':') {
            let bank_str = bank_str.trim_start_matches('$').trim_start_matches("0x");
            let addr_str = addr_str.trim_start_matches('$').trim_start_matches("0x");
            let bank = u8::from_str_radix(bank_str, 16).ok()?;
            let addr = u16::from_str_radix(addr_str, 16).ok()?;
            return Some(BreakpointSpec { bank, addr });
        }

        let t = trimmed.trim_start_matches('$').trim_start_matches("0x");
        let addr = u16::from_str_radix(t, 16).ok()?;
        let bank = if addr < 0x4000 {
            0
        } else if (0x4000..=0x7FFF).contains(&addr) {
            snapshot.debugger.active_rom_bank.min(0xFF) as u8
        } else {
            0xFF
        };

        Some(BreakpointSpec { bank, addr })
    }

    pub fn goto_address(&mut self, input: &str, _snapshot: &UiSnapshot) {
        let trimmed = input
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches('$');
        if let Ok(addr) = u16::from_str_radix(trimmed, 16) {
            self.pending_scroll_to_addr = Some(addr);
            self.pending_scroll_to_pc = false;
        }
    }

    pub fn reload_symbols(&mut self) {}

    pub fn invalidate_disasm_cache(&mut self) {}

    pub fn set_status(&mut self, _status: String) {}

    pub fn handle_step_over_request(
        &mut self,
        _paused: bool,
        _pc: u16,
        _memory: impl FnMut(u16) -> u8,
        _snapshot: &UiSnapshot,
    ) {
    }

    pub fn handle_run_to_cursor_request(&mut self, _paused: bool) {}

    pub fn handle_step_out_request(
        &mut self,
        _paused: bool,
        _sp: u16,
        _memory: impl FnMut(u16) -> u8,
        _snapshot: &UiSnapshot,
    ) {
    }

    pub fn handle_jump_to_cursor_request(&mut self, _paused: bool) {}

    pub fn handle_call_cursor_request(&mut self, _paused: bool) {}

    pub fn code_data(&self) -> &CodeDataTracker {
        static EMPTY: std::sync::LazyLock<CodeDataTracker> =
            std::sync::LazyLock::new(CodeDataTracker::default);
        &EMPTY
    }

    pub fn symbols(&self) -> Option<&RgbdsSymbols> {
        None
    }
}

#[derive(Debug, Default, Clone)]
pub struct RgbdsSymbols {
    by_bank_addr: HashMap<(u8, u16), Vec<String>>,
    by_name: HashMap<String, (u8, u16)>,
}

impl RgbdsSymbols {
    pub fn parse(_text: &str) -> Result<Self, String> {
        Ok(Self::default())
    }

    pub fn first_label_for(&self, bank: u8, addr: u16) -> Option<&str> {
        self.by_bank_addr
            .get(&(bank, addr))
            .and_then(|v| v.first())
            .map(|s| s.as_str())
    }

    pub fn lookup_name(&self, name: &str) -> Option<(u8, u16)> {
        self.by_name.get(name).copied()
    }

    pub fn labels_for(&self, bank: u8, addr: u16) -> Option<&[String]> {
        self.by_bank_addr.get(&(bank, addr)).map(|v| v.as_slice())
    }

    pub fn nearest_label_for(&self, _bank: u8, _addr: u16) -> Option<(&str, u16)> {
        None
    }
}
