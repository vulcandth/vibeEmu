use crate::ui::{
    code_data::{CellKind, CodeDataTracker, ExecutedInstruction},
    snapshot::UiSnapshot,
};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};
use vibe_emu_core::watchpoints::{WatchpointHit, WatchpointTrigger};

const NO_BANK: u8 = 0xFF;

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

#[derive(Debug, Clone)]
struct CachedDisassembly {
    key: DisasmCacheKey,
    addr_to_row: Vec<u32>,
    rows: Vec<DisasmRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisasmCacheKey {
    active_rom_bank: u16,
    cgb_mode: bool,
    vram_bank: u8,
    wram_bank: u8,
    sram_bank: u8,
    sram_enabled: bool,
    sym_revision: u64,
    analysis_revision: u64,
}

#[derive(Debug, Clone)]
struct DisasmRow {
    addr: u16,
    bp_bank: u8,
    display_bank: u8,
    label: Option<String>,
    bytes: String,
    text: String,
    len: u16,
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
pub struct DebuggerState {
    breakpoints: BTreeMap<BreakpointSpec, bool>,
    add_breakpoint_hex: String,
    goto_disasm: String,
    cursor: Option<BreakpointSpec>,
    sym: Option<RgbdsSymbols>,
    sym_path: Option<PathBuf>,
    status_line: Option<String>,
    pending_scroll_to_pc: bool,
    pending_scroll_to_addr: Option<u16>,
    last_paused: bool,
    last_pc: Option<u16>,
    pause_reason: Option<DebuggerPauseReason>,
    cached_disassembly: Option<CachedDisassembly>,
    code_data: CodeDataTracker,
    sym_revision: u64,
    pending_focus_main: bool,
    pending_breakpoints_sync: bool,
    next_debug_cmd_id: u64,
    pending_step_cmd_id: Option<u64>,
    waiting_debug_cmd_id: Option<u64>,
    pending_run_to: Option<DebuggerRunToRequest>,
    pending_step_over: bool,
    pending_run_to_cursor: bool,
    pending_continue: bool,
    pending_continue_no_break: bool,
    pending_continue_ignore_once: Option<BreakpointSpec>,
    pending_pause: bool,
    pending_reload_symbols: bool,
    pending_run_to_cursor_no_break: bool,
    pending_step_out: bool,
    pending_jump_to_cursor: bool,
    pending_call_cursor: bool,
    pending_jump_sp: bool,
    pending_toggle_animate: bool,
    pending_jump_to_addr: Option<u16>,
    pending_call_addr: Option<u16>,
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

impl DebuggerState {
    pub fn note_executed_instructions(&mut self, events: &[ExecutedInstruction]) {
        self.code_data.note_executed(events.iter().copied());
    }

    pub fn set_pause_reason(&mut self, reason: DebuggerPauseReason) {
        self.pause_reason = Some(reason);
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

    pub fn ack_debug_cmd(&mut self, cmd_id: u64) {
        if self.waiting_debug_cmd_id == Some(cmd_id) {
            self.waiting_debug_cmd_id = None;
        }
    }

    pub fn request_pause(&mut self) {
        self.pending_pause = true;
    }

    pub fn request_step(&mut self) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }

        let cmd_id = self.next_debug_cmd_id;
        self.next_debug_cmd_id = self.next_debug_cmd_id.wrapping_add(1);
        self.pending_step_cmd_id = Some(cmd_id);
        self.waiting_debug_cmd_id = Some(cmd_id);
        self.pause_reason = Some(DebuggerPauseReason::Step);
    }

    pub fn request_step_over(&mut self) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }
        self.pending_step_over = true;
    }

    pub fn request_run_to_cursor(&mut self) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }
        self.pending_run_to_cursor = true;
        self.pending_run_to_cursor_no_break = false;
    }

    pub fn request_run_to_cursor_no_break(&mut self) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }
        self.pending_run_to_cursor = false;
        self.pending_run_to_cursor_no_break = true;
    }

    fn request_continue(&mut self) {
        self.pending_continue = true;
        self.pending_continue_no_break = false;
        self.pending_continue_ignore_once = None;
        self.pause_reason = None;
    }

    fn request_continue_no_break(&mut self) {
        self.pending_continue = false;
        self.pending_continue_no_break = true;
        self.pending_continue_ignore_once = None;
        self.pause_reason = None;
    }

    fn request_continue_ignore_once(&mut self, bp: BreakpointSpec) {
        self.pending_continue = false;
        self.pending_continue_no_break = false;
        self.pending_continue_ignore_once = Some(bp);
        self.pause_reason = None;
    }

    pub fn request_continue_and_focus_main(&mut self) {
        self.pending_continue = true;
        self.pending_continue_no_break = false;
        self.pending_continue_ignore_once = None;
        self.pending_focus_main = true;
        self.pause_reason = None;
    }

    pub fn request_continue_no_break_and_focus_main(&mut self) {
        self.pending_continue = false;
        self.pending_continue_no_break = true;
        self.pending_continue_ignore_once = None;
        self.pending_focus_main = true;
        self.pause_reason = None;
    }

    pub fn request_run_not_this_break_and_focus_main(&mut self) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }

        let Some(DebuggerPauseReason::Breakpoint { bank, addr }) = self.pause_reason else {
            self.status_line = Some("Not paused at a breakpoint".to_string());
            return;
        };

        self.request_continue_ignore_once(BreakpointSpec { bank, addr });
        self.pending_focus_main = true;
    }

    pub fn request_step_out(&mut self) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }
        self.pending_step_out = true;
    }

    pub fn request_jump_to_cursor(&mut self) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }
        self.pending_jump_to_cursor = true;
    }

    pub fn request_call_cursor(&mut self) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }
        self.pending_call_cursor = true;
    }

    pub fn request_jump_sp(&mut self) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }
        self.pending_jump_sp = true;
    }

    pub fn request_toggle_animate(&mut self) {
        self.pending_toggle_animate = true;
    }

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
        self.pending_breakpoints_sync = false;
    }

    pub fn load_symbols_for_rom_path(&mut self, rom_path: Option<&Path>) {
        let Some(rom_path) = rom_path else {
            self.sym = None;
            self.sym_path = None;
            return;
        };

        let sym_path = rom_path.with_extension("sym");
        self.sym_path = Some(sym_path.clone());
        match fs::read_to_string(&sym_path) {
            Ok(text) => match RgbdsSymbols::parse(&text) {
                Ok(sym) => {
                    self.status_line = Some(format!(
                        "Loaded symbols: {}",
                        sym_path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    self.sym = Some(sym);
                    self.sym_revision = self.sym_revision.wrapping_add(1);
                    self.cached_disassembly = None;
                }
                Err(e) => {
                    self.sym = None;
                    self.status_line = Some(format!("Symbol parse failed: {e}"));
                    self.sym_revision = self.sym_revision.wrapping_add(1);
                    self.cached_disassembly = None;
                }
            },
            Err(_) => {
                self.sym = None;
                self.status_line = Some("No .sym file found".to_string());
                self.sym_revision = self.sym_revision.wrapping_add(1);
                self.cached_disassembly = None;
            }
        }
    }

    pub fn take_actions(&mut self) -> DebuggerUiActions {
        let out = DebuggerUiActions {
            request_pause: self.pending_pause,
            request_continue: self.pending_continue,
            request_continue_no_break: self.pending_continue_no_break,
            request_continue_ignore_once: self.pending_continue_ignore_once.take(),
            request_step: self.pending_step_cmd_id.take(),
            request_run_to: self.pending_run_to.take(),
            request_jump_to_cursor: self.pending_jump_to_addr.take(),
            request_call_cursor: self.pending_call_addr.take(),
            request_jump_sp: self.pending_jump_sp,
            request_focus_main: self.pending_focus_main,
            request_toggle_animate: self.pending_toggle_animate,
            breakpoints_updated: self.pending_breakpoints_sync,
            breakpoints: self.breakpoints().collect(),
        };

        self.pending_pause = false;
        self.pending_continue = false;
        self.pending_continue_no_break = false;
        self.pending_jump_sp = false;
        self.pending_toggle_animate = false;
        self.pending_focus_main = false;
        self.pending_breakpoints_sync = false;

        if self.pending_reload_symbols {
            self.pending_reload_symbols = false;
            if let Some(path) = self.sym_path.clone() {
                self.reload_symbols_from_path(&path);
            }
        }

        out
    }

    fn reload_symbols_from_path(&mut self, sym_path: &Path) {
        match fs::read_to_string(sym_path) {
            Ok(text) => match RgbdsSymbols::parse(&text) {
                Ok(sym) => {
                    self.sym = Some(sym);
                    self.sym_revision = self.sym_revision.wrapping_add(1);
                    self.cached_disassembly = None;
                    self.status_line = Some(format!(
                        "Reloaded symbols: {}",
                        sym_path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                Err(e) => {
                    self.sym = None;
                    self.sym_revision = self.sym_revision.wrapping_add(1);
                    self.cached_disassembly = None;
                    self.status_line = Some(format!("Symbol parse failed: {e}"));
                }
            },
            Err(e) => {
                self.sym = None;
                self.sym_revision = self.sym_revision.wrapping_add(1);
                self.cached_disassembly = None;
                self.status_line = Some(format!("Failed to read .sym: {e}"));
            }
        }
    }

    pub fn status_line(&self) -> Option<&str> {
        self.status_line.as_deref()
    }

    pub fn pause_reason(&self) -> Option<DebuggerPauseReason> {
        self.pause_reason
    }

    pub fn add_breakpoint(&mut self, bp: BreakpointSpec) {
        let should_sync = self.breakpoints.get(&bp).copied() != Some(true);
        self.breakpoints.insert(bp, true);
        if should_sync {
            self.pending_breakpoints_sync = true;
        }
    }

    pub fn remove_breakpoint(&mut self, bp: &BreakpointSpec) {
        if self.breakpoints.remove(bp).is_some() {
            self.pending_breakpoints_sync = true;
        }
    }

    pub fn toggle_breakpoint(&mut self, bp: BreakpointSpec) {
        if let Some(slot) = self.breakpoints.get_mut(&bp) {
            *slot = !*slot;
            self.pending_breakpoints_sync = true;
        } else {
            self.breakpoints.insert(bp, true);
            self.pending_breakpoints_sync = true;
        }
    }

    pub fn clear_breakpoints(&mut self) {
        if !self.breakpoints.is_empty() {
            self.breakpoints.clear();
            self.pending_breakpoints_sync = true;
        }
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

    pub fn lookup_symbol(&self, name: &str) -> Option<(u8, u16)> {
        self.sym.as_ref()?.lookup_name(name)
    }

    pub fn first_label_for(&self, bank: u8, addr: u16) -> Option<&str> {
        self.sym.as_ref()?.first_label_for(bank, addr)
    }

    pub fn parse_breakpoint_input(
        &self,
        input: &str,
        snapshot: &UiSnapshot,
    ) -> Option<BreakpointSpec> {
        parse_breakpoint_input(input, snapshot, self.sym.as_ref())
    }

    pub fn goto_address(&mut self, input: &str, snapshot: &UiSnapshot) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }

        let mut found: Option<(Option<u8>, u16)> = None;

        if let Some(sym) = self.sym.as_ref()
            && let Some((bank, addr)) = sym.lookup_name(trimmed)
        {
            found = Some((Some(bank), addr));
        }

        if found.is_none() {
            let t = trimmed.trim_start_matches("0x").trim_start_matches('$');
            if let Ok(addr) = u16::from_str_radix(t, 16) {
                found = Some((None, addr));
            }
        }

        let Some((bank, addr)) = found else {
            self.status_line = Some(format!("Unknown symbol/address: {trimmed}"));
            return;
        };

        if let Some(bank) = bank {
            if (0x4000..=0x7FFF).contains(&addr)
                && bank != snapshot.debugger.active_rom_bank.min(0xFF) as u8
            {
                self.status_line = Some(format!(
                    "Symbol is in ROM bank {bank:02X}, active bank is {:02X} (view may not match)",
                    snapshot.debugger.active_rom_bank.min(0xFF) as u8
                ));
            } else {
                self.status_line = None;
            }
        } else {
            self.status_line = None;
        }

        self.pending_scroll_to_addr = Some(addr);
    }

    pub fn reload_symbols(&mut self) {
        if let Some(sym_path) = self.sym_path.clone() {
            self.reload_symbols_from_path(&sym_path);
        }
    }

    pub fn invalidate_disasm_cache(&mut self) {
        self.cached_disassembly = None;
    }

    pub fn set_status(&mut self, status: String) {
        self.status_line = Some(status);
    }

    pub fn handle_step_over_request(
        &mut self,
        paused: bool,
        pc: u16,
        memory: impl Fn(u16) -> u8,
        snapshot: &UiSnapshot,
    ) {
        if !self.pending_step_over || !paused {
            return;
        }
        self.pending_step_over = false;

        let opcode = memory(pc);

        let should_step_over = matches!(
            opcode,
            0xC4 | 0xCC
                | 0xCD
                | 0xD4
                | 0xDC
                | 0xC7
                | 0xCF
                | 0xD7
                | 0xDF
                | 0xE7
                | 0xEF
                | 0xF7
                | 0xFF
        );

        if should_step_over {
            let next_pc = pc.wrapping_add(3);
            let bank = bp_bank_for_addr(next_pc, snapshot);
            self.pending_run_to = Some(DebuggerRunToRequest {
                target: BreakpointSpec {
                    bank,
                    addr: next_pc,
                },
                ignore_breakpoints: false,
            });
        } else {
            self.request_step();
        }
    }

    pub fn handle_run_to_cursor_request(&mut self, paused: bool) {
        if !paused {
            return;
        }

        if self.pending_run_to_cursor || self.pending_run_to_cursor_no_break {
            let ignore_bp = self.pending_run_to_cursor_no_break;
            self.pending_run_to_cursor = false;
            self.pending_run_to_cursor_no_break = false;

            if let Some(cursor) = self.cursor {
                self.pending_run_to = Some(DebuggerRunToRequest {
                    target: cursor,
                    ignore_breakpoints: ignore_bp,
                });
            } else {
                self.status_line = Some("No cursor set".to_string());
            }
        }
    }

    pub fn handle_step_out_request(
        &mut self,
        paused: bool,
        sp: u16,
        memory: impl Fn(u16) -> u8,
        snapshot: &UiSnapshot,
    ) {
        if !self.pending_step_out || !paused {
            return;
        }
        self.pending_step_out = false;

        let lo = memory(sp);
        let hi = memory(sp.wrapping_add(1));
        let return_addr = u16::from_le_bytes([lo, hi]);

        let bank = bp_bank_for_addr(return_addr, snapshot);
        self.pending_run_to = Some(DebuggerRunToRequest {
            target: BreakpointSpec {
                bank,
                addr: return_addr,
            },
            ignore_breakpoints: false,
        });
    }

    pub fn handle_jump_to_cursor_request(&mut self, paused: bool) {
        if !self.pending_jump_to_cursor || !paused {
            return;
        }
        self.pending_jump_to_cursor = false;

        if let Some(cursor) = self.cursor {
            self.pending_jump_to_addr = Some(cursor.addr);
        } else {
            self.status_line = Some("No cursor set".to_string());
        }
    }

    pub fn handle_call_cursor_request(&mut self, paused: bool) {
        if !self.pending_call_cursor || !paused {
            return;
        }
        self.pending_call_cursor = false;

        if let Some(cursor) = self.cursor {
            self.pending_call_addr = Some(cursor.addr);
        } else {
            self.status_line = Some("No cursor set".to_string());
        }
    }

    pub fn code_data(&self) -> &CodeDataTracker {
        &self.code_data
    }

    pub fn symbols(&self) -> Option<&RgbdsSymbols> {
        self.sym.as_ref()
    }
}

#[allow(dead_code)]
fn display_bank_for_addr(addr: u16, snapshot: &UiSnapshot) -> u8 {
    match addr {
        0x0000..=0x3FFF => 0,
        0x4000..=0x7FFF => snapshot.debugger.active_rom_bank.min(0xFF) as u8,
        0x8000..=0x9FFF => snapshot.debugger.vram_bank,
        0xA000..=0xBFFF => snapshot.debugger.sram_bank,
        0xC000..=0xCFFF => 0,
        0xD000..=0xDFFF => snapshot.debugger.wram_bank,
        0xE000..=0xEFFF => 0,
        0xF000..=0xFDFF => snapshot.debugger.wram_bank,
        _ => 0,
    }
}

fn bp_bank_for_addr(addr: u16, snapshot: &UiSnapshot) -> u8 {
    match addr {
        0x0000..=0x3FFF => 0,
        0x4000..=0x7FFF => snapshot.debugger.active_rom_bank.min(0xFF) as u8,
        _ => NO_BANK,
    }
}

fn parse_breakpoint_input(
    input: &str,
    snapshot: &UiSnapshot,
    sym: Option<&RgbdsSymbols>,
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

    if let Some(sym) = sym
        && let Some((bank, addr)) = sym.lookup_name(trimmed)
    {
        return Some(BreakpointSpec { bank, addr });
    }

    let t = trimmed.trim_start_matches('$').trim_start_matches("0x");
    let addr = u16::from_str_radix(t, 16).ok()?;
    let bank = bp_bank_for_addr(addr, snapshot);
    Some(BreakpointSpec { bank, addr })
}

#[derive(Debug, Default, Clone)]
pub struct RgbdsSymbols {
    by_bank_addr: HashMap<(u8, u16), Vec<String>>,
    by_name: HashMap<String, (u8, u16)>,
}

impl RgbdsSymbols {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut out = Self::default();

        for (line_no, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }

            let Some((lhs, name)) = line.split_once(' ') else {
                continue;
            };
            let Some((bank_s, addr_s)) = lhs.split_once(':') else {
                continue;
            };

            let bank = u8::from_str_radix(bank_s, 16)
                .map_err(|e| format!("line {}: invalid bank '{bank_s}': {e}", line_no + 1))?;
            let addr = u16::from_str_radix(addr_s, 16)
                .map_err(|e| format!("line {}: invalid addr '{addr_s}': {e}", line_no + 1))?;

            let name = name.trim();
            if name.is_empty() {
                continue;
            }

            out.by_bank_addr
                .entry((bank, addr))
                .or_default()
                .push(name.to_string());
            out.by_name.insert(name.to_string(), (bank, addr));
        }

        Ok(out)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sym_file() {
        let sym = RgbdsSymbols::parse("00:0100 Start\n03:4000 Main\n").unwrap();
        assert_eq!(sym.lookup_name("Start"), Some((0x00, 0x0100)));
        assert_eq!(sym.lookup_name("Main"), Some((0x03, 0x4000)));
        assert_eq!(sym.first_label_for(0x00, 0x0100), Some("Start"));
    }

    #[test]
    fn parse_breakpoint_with_bank() {
        let snap = UiSnapshot::default();
        let bp = parse_breakpoint_input("03:4000", &snap, None).unwrap();
        assert_eq!(bp.bank, 0x03);
        assert_eq!(bp.addr, 0x4000);
    }

    #[test]
    fn parse_breakpoint_without_bank() {
        let mut snap = UiSnapshot::default();
        snap.debugger.active_rom_bank = 5;
        let bp = parse_breakpoint_input("$4100", &snap, None).unwrap();
        assert_eq!(bp.bank, 0x05);
        assert_eq!(bp.addr, 0x4100);
    }

    #[test]
    fn breakpoint_input_accepts_symbol_name() {
        let sym = RgbdsSymbols::parse("03:4000 Start\n").unwrap();
        let mut snap = UiSnapshot::default();
        snap.debugger.active_rom_bank = 3;

        let bp = parse_breakpoint_input("Start", &snap, Some(&sym)).unwrap();
        assert_eq!(bp.bank, 0x03);
        assert_eq!(bp.addr, 0x4000);
    }
}
