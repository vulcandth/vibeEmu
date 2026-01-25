use crate::ui::{
    code_data::{CellKind, CodeDataTracker, ExecutedInstruction},
    snapshot::UiSnapshot,
};
use imgui::ListClipper;
use imgui::Ui;
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
            self.load_symbols_for_rom_path(self.sym_path.as_deref().and_then(|p| {
                // sym_path stores the .sym, but we want rom path; best-effort strip.
                // If this fails, we just reload from sym_path directly.
                let _ = p;
                None
            }));
        }

        out
    }

    pub fn ui(&mut self, ui: &Ui, snapshot: &UiSnapshot) {
        let paused = snapshot.debugger.paused;
        if paused && !self.last_paused {
            self.pending_scroll_to_pc = true;
        }
        if !paused && self.last_paused {
            self.pause_reason = None;
        }
        self.last_paused = paused;

        if paused {
            let pc = snapshot.cpu.pc;
            if self.last_pc != Some(pc) {
                self.pending_scroll_to_pc = true;
            }
            self.last_pc = Some(pc);
        }

        if paused {
            if self.pending_step_over {
                self.pending_step_over = false;
                self.handle_step_over_request(snapshot);
            }

            if self.pending_run_to_cursor {
                self.pending_run_to_cursor = false;
                self.handle_run_to_cursor_request(snapshot);
            }

            if self.pending_run_to_cursor_no_break {
                self.pending_run_to_cursor_no_break = false;
                self.handle_run_to_cursor_no_break_request(snapshot);
            }

            if self.pending_step_out {
                self.pending_step_out = false;
                self.handle_step_out_request(snapshot);
            }

            if self.pending_jump_to_cursor {
                self.pending_jump_to_cursor = false;
                self.handle_jump_to_cursor_request(snapshot);
            }

            if self.pending_call_cursor {
                self.pending_call_cursor = false;
                self.handle_call_cursor_request(snapshot);
            }
        } else {
            self.pending_step_over = false;
            self.pending_run_to_cursor = false;
            self.pending_run_to_cursor_no_break = false;
            self.pending_step_out = false;
            self.pending_jump_to_cursor = false;
            self.pending_call_cursor = false;
            self.pending_jump_sp = false;
        }

        self.update_disasm_cache(snapshot);

        let display = ui.io().display_size;
        let flags = imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_COLLAPSE;

        ui.window("Debugger")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size(display, imgui::Condition::Always)
            .flags(flags)
            .build(|| {
                self.draw_toolbar(ui, snapshot);
                ui.separator();

                if let Some(_table) = ui.begin_table_with_flags(
                    "debugger_layout",
                    2,
                    imgui::TableFlags::SIZING_STRETCH_PROP
                        | imgui::TableFlags::RESIZABLE
                        | imgui::TableFlags::BORDERS_INNER_V,
                ) {
                    ui.table_setup_column("Disasm");
                    ui.table_setup_column("State");

                    ui.table_next_row();

                    ui.table_next_column();
                    self.draw_disassembly(ui, snapshot);

                    ui.table_next_column();
                    self.draw_state_panes(ui, snapshot);
                }

                if let Some(status) = self.status_line.as_deref() {
                    ui.separator();
                    ui.text_disabled(status);
                }
            });
    }

    fn draw_toolbar(&mut self, ui: &Ui, snapshot: &UiSnapshot) {
        let paused = snapshot.debugger.paused;
        let run_label = if paused { "Run" } else { "Pause" };

        if ui.button(run_label) {
            if paused {
                self.pending_continue = true;
                self.pending_focus_main = true;
                self.pause_reason = None;
            } else {
                self.pending_pause = true;
                self.pause_reason = Some(DebuggerPauseReason::Manual);
            }
        }
        ui.same_line();
        if ui.button("Step") {
            self.request_step();
        }

        if paused {
            ui.same_line();
            if ui.button("Step Out") {
                self.request_step_out();
            }

            ui.same_line();
            if ui.button("Jump") {
                self.request_jump_to_cursor();
            }

            ui.same_line();
            if ui.button("Call") {
                self.request_call_cursor();
            }

            ui.same_line();
            if ui.button("Jump SP") {
                self.request_jump_sp();
            }
        }
        if self.waiting_debug_cmd_id.is_some() {
            ui.same_line();
            ui.text_disabled("(waiting)");
        }
        ui.same_line();

        if paused && let Some(reason) = self.pause_reason {
            ui.text_disabled(match reason {
                DebuggerPauseReason::Manual => "Paused (manual)".to_string(),
                DebuggerPauseReason::Step => "Paused (step)".to_string(),
                DebuggerPauseReason::DebuggerFocus => "Paused (debugger focus)".to_string(),
                DebuggerPauseReason::Breakpoint { bank, addr } => {
                    format!("Paused (breakpoint {:02X}:{:04X})", bank, addr)
                }
                DebuggerPauseReason::Watchpoint {
                    trigger,
                    addr,
                    value,
                    pc,
                } => {
                    let label = match trigger {
                        WatchpointTrigger::Read => "read",
                        WatchpointTrigger::Write => "write",
                        WatchpointTrigger::Execute => "execute",
                        WatchpointTrigger::Jump => "jump",
                    };
                    let value = value.map(|v| format!("=${v:02X} ")).unwrap_or_default();
                    let pc = pc.map(|pc| format!("pc={pc:04X} ")).unwrap_or_default();
                    format!("Paused (watchpoint {label} {pc}{value}@ {addr:04X})")
                }
            });
            ui.same_line();
        }

        ui.text("BP");
        ui.same_line();
        ui.set_next_item_width(130.0);
        let submitted = imgui::InputText::new(ui, "##add_bp", &mut self.add_breakpoint_hex)
            .enter_returns_true(true)
            .build();
        ui.same_line();
        if (ui.small_button("Add") || submitted)
            && let Some(bp) =
                parse_breakpoint_input(&self.add_breakpoint_hex, snapshot, self.sym.as_ref())
        {
            let should_sync = self.breakpoints.get(&bp).copied() != Some(true);
            self.breakpoints.insert(bp, true);
            if should_sync {
                self.pending_breakpoints_sync = true;
            }
        }
        ui.same_line();
        if ui.small_button("Clear") && !self.breakpoints.is_empty() {
            self.breakpoints.clear();
            self.pending_breakpoints_sync = true;
        }

        ui.text("Go");
        ui.same_line();
        ui.set_next_item_width(180.0);
        let goto_submitted = imgui::InputText::new(ui, "##goto_disasm", &mut self.goto_disasm)
            .enter_returns_true(true)
            .build();
        ui.same_line();
        let goto_clicked = ui.small_button("Go##goto_btn");
        if goto_submitted || goto_clicked {
            self.handle_goto_request(snapshot);
        }

        ui.same_line();
        #[allow(clippy::collapsible_if)]
        if ui.small_button("Reload .sym") {
            if let Some(sym_path) = self.sym_path.clone() {
                match fs::read_to_string(&sym_path) {
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
        }
    }

    fn handle_goto_request(&mut self, snapshot: &UiSnapshot) {
        let trimmed = self.goto_disasm.trim();
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
            let t = trimmed.trim_start_matches("0x");
            if let Ok(addr) = u16::from_str_radix(t, 16) {
                found = Some((None, addr));
            }
        }

        let Some((bank, addr)) = found else {
            self.status_line = Some(format!("Unknown symbol/address: {trimmed}"));
            return;
        };

        if let Some(bank) = bank
            && (0x4000..=0x7FFF).contains(&addr)
            && bank != snapshot.debugger.active_rom_bank.min(0xFF) as u8
        {
            self.status_line = Some(format!(
                "Symbol is in ROM bank {bank:02X}, active bank is {:02X} (view may not match)",
                snapshot.debugger.active_rom_bank.min(0xFF) as u8
            ));
        } else {
            self.status_line = None;
        }

        self.pending_scroll_to_addr = Some(addr);
    }

    fn draw_state_panes(&mut self, ui: &Ui, snapshot: &UiSnapshot) {
        let cpu = &snapshot.cpu;

        ui.text("CPU");
        if let Some(_table) = ui.begin_table_with_flags(
            "regs",
            2,
            imgui::TableFlags::SIZING_FIXED_FIT | imgui::TableFlags::BORDERS_INNER_H,
        ) {
            reg_row(ui, "AF", ((cpu.a as u16) << 8) | cpu.f as u16);
            reg_row(ui, "BC", ((cpu.b as u16) << 8) | cpu.c as u16);
            reg_row(ui, "DE", ((cpu.d as u16) << 8) | cpu.e as u16);
            reg_row(ui, "HL", ((cpu.h as u16) << 8) | cpu.l as u16);
            reg_row(ui, "SP", cpu.sp);
            reg_row(ui, "PC", cpu.pc);

            ui.table_next_row();
            ui.table_next_column();
            ui.text("Flags");
            ui.table_next_column();
            let f = cpu.f;
            let z = if (f & 0x80) != 0 { 'Z' } else { '-' };
            let n = if (f & 0x40) != 0 { 'N' } else { '-' };
            let h = if (f & 0x20) != 0 { 'H' } else { '-' };
            let c = if (f & 0x10) != 0 { 'C' } else { '-' };
            ui.text(format!("{z}{n}{h}{c}"));

            ui.table_next_row();
            ui.table_next_column();
            ui.text("IME");
            ui.table_next_column();
            ui.text(format!("{}", cpu.ime));

            ui.table_next_row();
            ui.table_next_column();
            ui.text("Cycles");
            ui.table_next_column();
            ui.text(format!("{}", cpu.cycles));
        }

        ui.separator();
        ui.text("I/O");
        if let Some(_table) = ui.begin_table_with_flags(
            "io_regs",
            2,
            imgui::TableFlags::SIZING_FIXED_FIT | imgui::TableFlags::BORDERS_INNER_H,
        ) {
            reg_row_u8(ui, "LCDC", snapshot.ppu.lcdc);
            reg_row_u8(ui, "STAT", snapshot.ppu.stat);
            reg_row_u8(ui, "LY", snapshot.ppu.ly);
            reg_row_u8(ui, "SCX", snapshot.ppu.scx);
            reg_row_u8(ui, "SCY", snapshot.ppu.scy);
            reg_row_u8(ui, "IF", snapshot.debugger.if_reg);
            reg_row_u8(ui, "IE", snapshot.debugger.ie_reg);
        }

        ui.separator();
        ui.text("Breakpoints");
        ui.child_window("bp_list").size([0.0, 140.0]).build(|| {
            let mut to_remove: Option<BreakpointSpec> = None;
            let entries: Vec<(BreakpointSpec, bool)> =
                self.breakpoints.iter().map(|(&bp, &en)| (bp, en)).collect();
            for (bp, mut enabled) in entries {
                let sym_label = self
                    .sym
                    .as_ref()
                    .and_then(|s| s.first_label_for(bp.bank, bp.addr));
                ui.checkbox(
                    format!("##bp_enabled_{:02X}_{:04X}", bp.bank, bp.addr),
                    &mut enabled,
                );
                ui.same_line();
                if let Some(sym_label) = sym_label {
                    ui.text(format!("{:02X}:{:04X}  {sym_label}", bp.bank, bp.addr));
                } else {
                    ui.text(format!("{:02X}:{:04X}", bp.bank, bp.addr));
                }
                ui.same_line();
                let btn = format!("Remove##bp_{:02X}_{:04X}", bp.bank, bp.addr);
                if ui.small_button(btn) {
                    to_remove = Some(bp);
                }

                if let Some(slot) = self.breakpoints.get_mut(&bp)
                    && *slot != enabled
                {
                    *slot = enabled;
                    self.pending_breakpoints_sync = true;
                }
            }
            if let Some(bp) = to_remove {
                self.breakpoints.remove(&bp);
                self.pending_breakpoints_sync = true;
            }
        });

        ui.separator();
        ui.text("Stack");
        ui.child_window("stack").size([0.0, 0.0]).build(|| {
            let base = snapshot.debugger.stack_base;
            let bytes = &snapshot.debugger.stack_bytes;

            for (i, chunk) in bytes.chunks_exact(2).take(16).enumerate() {
                let addr = base.wrapping_add((i as u16) * 2);
                let val = (chunk[1] as u16) << 8 | (chunk[0] as u16);
                ui.text(format!("{addr:04X}: {val:04X}"));
            }
        });
    }

    fn draw_disassembly(&mut self, ui: &Ui, snapshot: &UiSnapshot) {
        ui.text("Disassembly");
        ui.separator();
        let pc = snapshot.cpu.pc;

        ui.child_window("disasm").size([0.0, 0.0]).build(|| {
            if let Some(_table) = ui.begin_table_with_flags(
                "disasm_table",
                4,
                imgui::TableFlags::SIZING_FIXED_FIT
                    | imgui::TableFlags::ROW_BG
                    | imgui::TableFlags::SCROLL_Y,
            ) {
                ui.table_setup_column("BP");
                ui.table_setup_column("Addr");
                ui.table_setup_column("Bytes");
                ui.table_setup_column("Instr");
                ui.table_headers_row();

                let Some(cache) = self.cached_disassembly.as_ref() else {
                    return;
                };

                let scroll_target = self
                    .pending_scroll_to_addr
                    .take()
                    .or_else(|| self.pending_scroll_to_pc.then_some(pc));
                if let Some(addr) = scroll_target {
                    if let Some(&row_idx) = cache.addr_to_row.get(addr as usize)
                        && row_idx != u32::MAX
                    {
                        let line_h = ui.text_line_height_with_spacing();
                        ui.set_scroll_y(row_idx as f32 * line_h);
                    }
                    self.pending_scroll_to_pc = false;
                }

                let item_h = ui.text_line_height_with_spacing();
                let mut clipper = ListClipper::new(cache.rows.len() as i32)
                    .items_height(item_h)
                    .begin(ui);
                while clipper.step() {
                    for idx in clipper.display_start()..clipper.display_end() {
                        let row = &cache.rows[idx as usize];
                        let is_pc = row.addr == pc;
                        let bp_key = BreakpointSpec {
                            bank: row.bp_bank,
                            addr: row.addr,
                        };
                        let is_cursor = self.cursor == Some(bp_key);
                        let bp_enabled = self.breakpoints.get(&bp_key).copied();
                        let has_bp = bp_enabled.is_some();

                        ui.table_next_row();
                        ui.table_next_column();
                        let marker = match bp_enabled {
                            Some(true) => "●",
                            Some(false) => "○",
                            None => " ",
                        };
                        let btn_id =
                            format!("{marker}##bp_toggle_{:02X}_{:04X}", row.bp_bank, row.addr);
                        if ui.small_button(btn_id) {
                            if has_bp {
                                if let Some(slot) = self.breakpoints.get_mut(&bp_key) {
                                    *slot = !*slot;
                                }
                            } else {
                                self.breakpoints.insert(bp_key, true);
                            }
                            self.pending_breakpoints_sync = true;
                        }

                        ui.table_next_column();
                        let addr_text = if row.display_bank == NO_BANK {
                            format!("--:{:04X}", row.addr)
                        } else {
                            format!("{:02X}:{:04X}", row.display_bank, row.addr)
                        };
                        if is_pc {
                            ui.text_colored([1.0, 1.0, 0.2, 1.0], &addr_text);
                        } else if is_cursor {
                            ui.text_colored([0.6, 0.8, 1.0, 1.0], &addr_text);
                        } else {
                            ui.text(&addr_text);
                        }
                        if ui.is_item_clicked() {
                            self.cursor = Some(bp_key);
                        }

                        ui.table_next_column();
                        ui.text(&row.bytes);
                        if ui.is_item_clicked() {
                            self.cursor = Some(bp_key);
                        }

                        ui.table_next_column();
                        if let Some(label) = row.label.as_deref() {
                            ui.text_colored([0.6, 0.8, 1.0, 1.0], format!("{label}:"));
                            ui.same_line();
                        }

                        if is_pc {
                            ui.text_colored([1.0, 1.0, 0.2, 1.0], &row.text);
                        } else {
                            ui.text(&row.text);
                        }
                        if ui.is_item_clicked() {
                            self.cursor = Some(bp_key);
                        }
                    }
                }
            }
        });
    }

    fn update_disasm_cache(&mut self, snapshot: &UiSnapshot) {
        let pc = snapshot.cpu.pc;
        if !snapshot.debugger.paused {
            self.cached_disassembly = None;
            return;
        }

        let Some(mem) = snapshot.debugger.mem_image.as_deref().map(|m| m.as_slice()) else {
            self.cached_disassembly = None;
            return;
        };

        self.code_data.ensure_up_to_date(snapshot);
        let active_bank = snapshot.debugger.active_rom_bank.min(0xFF) as u8;

        let key = DisasmCacheKey {
            active_rom_bank: snapshot.debugger.active_rom_bank,
            cgb_mode: snapshot.debugger.cgb_mode,
            vram_bank: snapshot.debugger.vram_bank,
            wram_bank: snapshot.debugger.wram_bank,
            sram_bank: snapshot.debugger.sram_bank,
            sram_enabled: snapshot.debugger.sram_enabled,
            sym_revision: self.sym_revision,
            analysis_revision: self.code_data.revision(),
        };

        if let Some(cache) = self.cached_disassembly.as_ref()
            && cache.key == key
            && cache
                .addr_to_row
                .get(pc as usize)
                .copied()
                .unwrap_or(u32::MAX)
                != u32::MAX
        {
            return;
        }
        let mut rows = Vec::with_capacity(0x10000 / 2);
        let mut addr_to_row = vec![u32::MAX; 0x10000];

        let mut a: u32 = 0;
        while a < 0x10000 {
            let addr = a as u16;

            let bp_bank = breakpoint_bank_for_addr(addr, snapshot);
            let display_bank = display_bank_for_addr(addr, snapshot);

            let label = self
                .sym
                .as_ref()
                .and_then(|sym| {
                    if (0x0000..=0x7FFF).contains(&addr) {
                        sym.first_label_for(bp_bank, addr)
                    } else {
                        None
                    }
                })
                .map(|s| s.to_string());

            let (mut text, len) = match self.code_data.kind_at(addr, active_bank) {
                CellKind::CodeStart { len } => {
                    let (text, decoded_len) = decode_sm83_at(mem, 0, addr);
                    let effective_len = if decoded_len == 0 {
                        u16::from(len.max(1))
                    } else {
                        decoded_len
                    };
                    (text, effective_len)
                }
                _ => {
                    let b = mem.get(addr as usize).copied().unwrap_or(0);
                    (format!("DB ${:02X}", b), 1)
                }
            };

            let bytes = format_disasm_bytes(mem, addr, len);

            if matches!(
                self.code_data.kind_at(addr, active_bank),
                CellKind::CodeStart { .. }
            ) {
                let instr_bank = effective_bank_for_addr(addr, snapshot.debugger.active_rom_bank);
                text = substitute_immediate_labels(&text, snapshot, self.sym.as_ref(), instr_bank);
            }

            let row_index = rows.len() as u32;
            addr_to_row[addr as usize] = row_index;

            rows.push(DisasmRow {
                addr,
                bp_bank,
                display_bank,
                label,
                bytes,
                text,
                len,
            });

            a = a.saturating_add(len as u32);
        }

        self.cached_disassembly = Some(CachedDisassembly {
            key,
            addr_to_row,
            rows,
        });
    }

    fn handle_step_over_request(&mut self, snapshot: &UiSnapshot) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }

        let Some(mem) = snapshot.debugger.mem_image.as_deref().map(|m| m.as_slice()) else {
            self.request_step();
            return;
        };

        let pc = snapshot.cpu.pc;
        let opcode = mem.get(pc as usize).copied().unwrap_or(0x00);
        let is_call_like = matches!(opcode, 0xC4 | 0xCC | 0xCD | 0xD4 | 0xDC);
        let is_rst = matches!(
            opcode,
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF
        );

        if !(is_call_like || is_rst) {
            self.request_step();
            return;
        }

        let (_text, mut len) = decode_sm83_at(mem, 0, pc);
        if len == 0 {
            len = 1;
        }

        let ret = pc.wrapping_add(len);
        let bank = breakpoint_bank_for_addr(ret, snapshot);
        self.pending_run_to = Some(DebuggerRunToRequest {
            target: BreakpointSpec { bank, addr: ret },
            ignore_breakpoints: false,
        });
        self.pause_reason = None;
    }

    fn handle_run_to_cursor_request(&mut self, snapshot: &UiSnapshot) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }

        let Some(target) = self.cursor else {
            self.status_line = Some("No cursor selected in disassembly".to_string());
            return;
        };

        let pc = snapshot.cpu.pc;
        let pc_bank = breakpoint_bank_for_addr(pc, snapshot);
        let mut note = String::new();
        if target.addr == pc && target.bank == pc_bank {
            self.status_line = Some(format!(
                "Cursor is already at PC {:02X}:{:04X}",
                pc_bank, pc
            ));
            return;
        }

        if target.bank == pc_bank && target.addr < pc {
            note = " (cursor is behind current PC; will stop on next execution)".to_string();
        }

        self.status_line = Some(format!(
            "Run to {:02X}:{:04X} (from PC {:02X}:{:04X}){note}",
            target.bank, target.addr, pc_bank, pc
        ));

        self.pending_run_to = Some(DebuggerRunToRequest {
            target,
            ignore_breakpoints: false,
        });
        self.pause_reason = None;
    }

    fn handle_run_to_cursor_no_break_request(&mut self, snapshot: &UiSnapshot) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }

        let Some(target) = self.cursor else {
            self.status_line = Some("No cursor selected in disassembly".to_string());
            return;
        };

        let pc = snapshot.cpu.pc;
        let pc_bank = breakpoint_bank_for_addr(pc, snapshot);
        let mut note = String::new();
        if target.addr == pc && target.bank == pc_bank {
            self.status_line = Some(format!(
                "Cursor is already at PC {:02X}:{:04X}",
                pc_bank, pc
            ));
            return;
        }

        if target.bank == pc_bank && target.addr < pc {
            note = " (cursor is behind current PC; will stop on next execution)".to_string();
        }

        self.status_line = Some(format!(
            "Run to {:02X}:{:04X} (no break) (from PC {:02X}:{:04X}){note}",
            target.bank, target.addr, pc_bank, pc
        ));

        self.pending_run_to = Some(DebuggerRunToRequest {
            target,
            ignore_breakpoints: true,
        });
        self.pause_reason = None;
    }

    fn handle_step_out_request(&mut self, snapshot: &UiSnapshot) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }

        let Some(mem) = snapshot.debugger.mem_image.as_deref().map(|m| m.as_slice()) else {
            self.status_line = Some("No memory image available for step-out".to_string());
            return;
        };

        let sp = snapshot.cpu.sp;
        let lo = mem.get(sp as usize).copied().unwrap_or(0);
        let hi = mem.get(sp.wrapping_add(1) as usize).copied().unwrap_or(0);
        let ret = u16::from_le_bytes([lo, hi]);
        let bank = breakpoint_bank_for_addr(ret, snapshot);

        self.status_line = Some(format!("Step out to {:02X}:{:04X}", bank, ret));
        self.pending_run_to = Some(DebuggerRunToRequest {
            target: BreakpointSpec { bank, addr: ret },
            ignore_breakpoints: false,
        });
        self.pause_reason = None;
    }

    fn handle_jump_to_cursor_request(&mut self, _snapshot: &UiSnapshot) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }

        let Some(target) = self.cursor else {
            self.status_line = Some("No cursor selected in disassembly".to_string());
            return;
        };

        self.status_line = Some(format!(
            "Jump to cursor {:02X}:{:04X}",
            target.bank, target.addr
        ));
        self.pending_jump_to_addr = Some(target.addr);
    }

    fn handle_call_cursor_request(&mut self, _snapshot: &UiSnapshot) {
        if self.waiting_debug_cmd_id.is_some() {
            return;
        }

        let Some(target) = self.cursor else {
            self.status_line = Some("No cursor selected in disassembly".to_string());
            return;
        };

        self.status_line = Some(format!(
            "Call cursor {:02X}:{:04X} (pushes current PC)",
            target.bank, target.addr
        ));
        self.pending_call_addr = Some(target.addr);
    }
}

fn reg_row(ui: &Ui, label: &str, value: u16) {
    ui.table_next_row();
    ui.table_next_column();
    ui.text(label);
    ui.table_next_column();
    ui.text(format!("{value:04X}"));
}

fn reg_row_u8(ui: &Ui, label: &str, value: u8) {
    ui.table_next_row();
    ui.table_next_column();
    ui.text(label);
    ui.table_next_column();
    ui.text(format!("{value:02X}"));
}

fn format_disasm_bytes(mem: &[u8], addr: u16, len: u16) -> String {
    let mut out = String::new();
    let max_len = len.min(3) as usize;
    for i in 0..3 {
        if i != 0 {
            out.push(' ');
        }

        if i < max_len {
            let idx = addr as usize + i;
            if let Some(b) = mem.get(idx) {
                use std::fmt::Write;
                let _ = write!(&mut out, "{b:02X}");
            } else {
                out.push_str("??");
            }
        } else {
            out.push_str("  ");
        }
    }
    out
}

fn parse_u16_hex_exact(s: &str) -> Option<u16> {
    if s.len() != 4 {
        return None;
    }
    u16::from_str_radix(s, 16).ok()
}

fn parse_u8_hex_exact(s: &str) -> Option<u8> {
    if s.len() != 2 {
        return None;
    }
    u8::from_str_radix(s, 16).ok()
}

fn parse_breakpoint_spec(input: &str, snapshot: &UiSnapshot) -> Option<BreakpointSpec> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((bank_s, addr_s)) = trimmed.split_once(':') {
        let bank = parse_u8_hex_exact(bank_s.trim())?;
        let addr = parse_u16_hex_exact(addr_s.trim())?;
        let bank = normalize_bank_for_addr(addr, bank);
        return Some(BreakpointSpec { bank, addr });
    }

    let addr = {
        let t = trimmed.trim_start_matches("0x");
        u16::from_str_radix(t, 16).ok()?
    };
    let bank = breakpoint_bank_for_addr(addr, snapshot);
    Some(BreakpointSpec { bank, addr })
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

    if trimmed.contains(':') {
        return parse_breakpoint_spec(trimmed, snapshot);
    }

    let hex_candidate = trimmed.trim_start_matches("0x");
    if u16::from_str_radix(hex_candidate, 16).is_ok() {
        return parse_breakpoint_spec(trimmed, snapshot);
    }

    let (bank, addr) = sym?.lookup_name(trimmed)?;
    Some(BreakpointSpec { bank, addr })
}

fn normalize_bank_for_addr(addr: u16, bank: u8) -> u8 {
    match addr {
        0x0000..=0x3FFF => 0,
        0x4000..=0x7FFF => bank,
        0x8000..=0x9FFF => bank,
        0xA000..=0xBFFF => bank,
        0xC000..=0xCFFF => 0,
        0xD000..=0xDFFF => bank,
        0xE000..=0xEFFF => 0,
        0xF000..=0xFDFF => bank,
        _ => 0,
    }
}

fn breakpoint_bank_for_addr(addr: u16, snapshot: &UiSnapshot) -> u8 {
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

fn effective_bank_for_addr(addr: u16, active_rom_bank: u16) -> Option<u8> {
    if addr < 0x4000 {
        return Some(0);
    }
    if (0x4000..=0x7FFF).contains(&addr) {
        return Some(active_rom_bank.min(0xFF) as u8);
    }
    None
}

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

fn substitute_immediate_labels(
    text: &str,
    snapshot: &UiSnapshot,
    sym: Option<&RgbdsSymbols>,
    current_bank: Option<u8>,
) -> String {
    let Some(sym) = sym else {
        return text.to_string();
    };

    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        if i + 5 > bytes.len() {
            out.push('$');
            i += 1;
            continue;
        }

        let hex = &text[i + 1..i + 5];
        let Ok(addr) = u16::from_str_radix(hex, 16) else {
            out.push('$');
            i += 1;
            continue;
        };

        if let Some(label) = resolve_label_for_immediate(sym, snapshot, current_bank, addr) {
            out.push_str(label);
        } else {
            out.push('$');
            out.push_str(hex);
        }

        i += 5;
    }

    out
}

fn resolve_label_for_immediate<'a>(
    sym: &'a RgbdsSymbols,
    snapshot: &UiSnapshot,
    current_bank: Option<u8>,
    addr: u16,
) -> Option<&'a str> {
    let active_rom_bank = snapshot.debugger.active_rom_bank.min(0xFF) as u8;

    let mut banks = [0u8; 2];
    banks[0] = 0;
    banks[1] = current_bank.unwrap_or(active_rom_bank);

    if (0x4000..=0x7FFF).contains(&addr) {
        banks.swap(0, 1);
    }

    for bank in banks {
        if let Some(label) = sym.first_label_for(bank, addr) {
            return Some(label);
        }
    }

    None
}

fn decode_sm83_at(mem: &[u8], base: u16, addr: u16) -> (String, u16) {
    let idx = addr.wrapping_sub(base) as usize;
    let b0 = *mem.get(idx).unwrap_or(&0x00);

    if b0 == 0xCB {
        let b1 = *mem.get(idx + 1).unwrap_or(&0x00);
        let (s, len) = decode_cb(b1);
        return (s, len);
    }

    decode_base(mem, base, addr, b0)
}

fn decode_base(mem: &[u8], base: u16, addr: u16, op: u8) -> (String, u16) {
    let x = op >> 6;
    let y = (op >> 3) & 0x07;
    let z = op & 0x07;
    let p = y >> 1;
    let q = y & 0x01;

    let imm8 = |offset: usize| -> u8 {
        *mem.get(addr.wrapping_sub(base) as usize + offset)
            .unwrap_or(&0)
    };
    let imm16 = |offset: usize| -> u16 {
        let lo = imm8(offset) as u16;
        let hi = imm8(offset + 1) as u16;
        (hi << 8) | lo
    };

    let r = |idx: u8| -> &'static str {
        match idx {
            0 => "B",
            1 => "C",
            2 => "D",
            3 => "E",
            4 => "H",
            5 => "L",
            6 => "(HL)",
            7 => "A",
            _ => "?",
        }
    };

    let rp = |idx: u8| -> &'static str {
        match idx {
            0 => "BC",
            1 => "DE",
            2 => "HL",
            3 => "SP",
            _ => "?",
        }
    };

    let rp2 = |idx: u8| -> &'static str {
        match idx {
            0 => "BC",
            1 => "DE",
            2 => "HL",
            3 => "AF",
            _ => "?",
        }
    };

    let alu = |idx: u8| -> &'static str {
        match idx {
            0 => "ADD",
            1 => "ADC",
            2 => "SUB",
            3 => "SBC",
            4 => "AND",
            5 => "XOR",
            6 => "OR",
            7 => "CP",
            _ => "?",
        }
    };

    match x {
        0 => match z {
            0 => match y {
                0 => ("NOP".to_string(), 1),
                1 => (format!("LD (${:04X}),SP", imm16(1)), 3),
                2 => ("STOP".to_string(), 2),
                3 => {
                    let e = imm8(1) as i8;
                    let dest = addr.wrapping_add(2).wrapping_add(e as u16);
                    (format!("JR ${:04X}", dest), 2)
                }
                4 => rel("JR NZ", addr, imm8(1)),
                5 => rel("JR Z", addr, imm8(1)),
                6 => rel("JR NC", addr, imm8(1)),
                7 => rel("JR C", addr, imm8(1)),
                _ => unreachable!(),
            },
            1 => {
                let rp_name = rp(p);
                if q == 0 {
                    (format!("LD {rp_name},${:04X}", imm16(1)), 3)
                } else {
                    (format!("ADD HL,{rp_name}"), 1)
                }
            }
            2 => {
                let s = match (q, p) {
                    (0, 0) => "LD (BC),A".to_string(),
                    (0, 1) => "LD (DE),A".to_string(),
                    (0, 2) => "LD (HL+),A".to_string(),
                    (0, 3) => "LD (HL-),A".to_string(),
                    (1, 0) => "LD A,(BC)".to_string(),
                    (1, 1) => "LD A,(DE)".to_string(),
                    (1, 2) => "LD A,(HL+)".to_string(),
                    (1, 3) => "LD A,(HL-)".to_string(),
                    _ => "DB".to_string(),
                };
                (s, 1)
            }
            3 => {
                let rp_name = rp(p);
                if q == 0 {
                    (format!("INC {rp_name}"), 1)
                } else {
                    (format!("DEC {rp_name}"), 1)
                }
            }
            4 => (format!("INC {}", r(y)), 1),
            5 => (format!("DEC {}", r(y)), 1),
            6 => (format!("LD {},${:02X}", r(y), imm8(1)), 2),
            7 => match y {
                0 => ("RLCA".to_string(), 1),
                1 => ("RRCA".to_string(), 1),
                2 => ("RLA".to_string(), 1),
                3 => ("RRA".to_string(), 1),
                4 => ("DAA".to_string(), 1),
                5 => ("CPL".to_string(), 1),
                6 => ("SCF".to_string(), 1),
                7 => ("CCF".to_string(), 1),
                _ => ("DB".to_string(), 1),
            },
            _ => (format!("DB ${op:02X}"), 1),
        },
        1 => {
            if op == 0x76 {
                return ("HALT".to_string(), 1);
            }
            (format!("LD {},{}", r(y), r(z)), 1)
        }
        2 => (format!("{} {}", alu(y), r(z)), 1),
        3 => match z {
            0 => match y {
                0 => ("RET NZ".to_string(), 1),
                1 => ("RET Z".to_string(), 1),
                2 => ("RET NC".to_string(), 1),
                3 => ("RET C".to_string(), 1),
                4 => (format!("LDH ($FF{:02X}),A", imm8(1)), 2),
                5 => {
                    let e = imm8(1) as i8;
                    (format!("ADD SP,{e}"), 2)
                }
                6 => (format!("LDH A,($FF{:02X})", imm8(1)), 2),
                7 => {
                    let e = imm8(1) as i8;
                    (format!("LD HL,SP+{e}"), 2)
                }
                _ => (format!("DB ${op:02X}"), 1),
            },
            1 => {
                if q == 0 {
                    (format!("POP {}", rp2(p)), 1)
                } else {
                    match p {
                        0 => ("RET".to_string(), 1),
                        1 => ("RETI".to_string(), 1),
                        2 => ("JP (HL)".to_string(), 1),
                        3 => ("LD SP,HL".to_string(), 1),
                        _ => (format!("DB ${op:02X}"), 1),
                    }
                }
            }
            2 => match y {
                0 => (format!("JP NZ,${:04X}", imm16(1)), 3),
                1 => (format!("JP Z,${:04X}", imm16(1)), 3),
                2 => (format!("JP NC,${:04X}", imm16(1)), 3),
                3 => (format!("JP C,${:04X}", imm16(1)), 3),
                4 => ("LDH (C),A".to_string(), 1),
                5 => (format!("LD (${:04X}),A", imm16(1)), 3),
                6 => ("LDH A,(C)".to_string(), 1),
                7 => (format!("LD A,(${:04X})", imm16(1)), 3),
                _ => (format!("DB ${op:02X}"), 1),
            },
            3 => match y {
                0 => (format!("JP ${:04X}", imm16(1)), 3),
                1 => ("PREFIX CB".to_string(), 1),
                6 => ("DI".to_string(), 1),
                7 => ("EI".to_string(), 1),
                _ => (format!("DB ${op:02X}"), 1),
            },
            4 => match y {
                0 => (format!("CALL NZ,${:04X}", imm16(1)), 3),
                1 => (format!("CALL Z,${:04X}", imm16(1)), 3),
                2 => (format!("CALL NC,${:04X}", imm16(1)), 3),
                3 => (format!("CALL C,${:04X}", imm16(1)), 3),
                _ => (format!("DB ${op:02X}"), 1),
            },
            5 => {
                if q == 0 {
                    (format!("PUSH {}", rp2(p)), 1)
                } else if p == 0 {
                    (format!("CALL ${:04X}", imm16(1)), 3)
                } else {
                    (format!("DB ${op:02X}"), 1)
                }
            }
            6 => (format!("{} ${:02X}", alu(y), imm8(1)), 2),
            7 => (format!("RST ${:02X}", y * 8), 1),
            _ => (format!("DB ${op:02X}"), 1),
        },
        _ => (format!("DB ${op:02X}"), 1),
    }
}

fn rel(mn: &str, addr: u16, imm: u8) -> (String, u16) {
    let e = imm as i8;
    let dest = addr.wrapping_add(2).wrapping_add(e as u16);
    (format!("{mn},${:04X}", dest), 2)
}

fn decode_cb(op: u8) -> (String, u16) {
    let x = op >> 6;
    let y = (op >> 3) & 0x07;
    let z = op & 0x07;

    let r = |idx: u8| -> &'static str {
        match idx {
            0 => "B",
            1 => "C",
            2 => "D",
            3 => "E",
            4 => "H",
            5 => "L",
            6 => "(HL)",
            7 => "A",
            _ => "?",
        }
    };

    let rot = |idx: u8| -> &'static str {
        match idx {
            0 => "RLC",
            1 => "RRC",
            2 => "RL",
            3 => "RR",
            4 => "SLA",
            5 => "SRA",
            6 => "SWAP",
            7 => "SRL",
            _ => "?",
        }
    };

    let s = match x {
        0 => format!("{} {}", rot(y), r(z)),
        1 => format!("BIT {y},{}", r(z)),
        2 => format!("RES {y},{}", r(z)),
        3 => format!("SET {y},{}", r(z)),
        _ => format!("DB $CB{:02X}", op),
    };

    (s, 2)
}

#[derive(Debug, Default, Clone)]
struct RgbdsSymbols {
    by_bank_addr: HashMap<(u8, u16), Vec<String>>,
    by_name: HashMap<String, (u8, u16)>,
}

impl RgbdsSymbols {
    fn parse(text: &str) -> Result<Self, String> {
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

    fn first_label_for(&self, bank: u8, addr: u16) -> Option<&str> {
        self.by_bank_addr
            .get(&(bank, addr))
            .and_then(|v| v.first())
            .map(|s| s.as_str())
    }

    fn lookup_name(&self, name: &str) -> Option<(u8, u16)> {
        self.by_name.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_sym_lines() {
        let text = r#"
; comment
00:0000 EntryPoint
01:4000 Foo
01:4000 Foo.Alias
"#;

        let sym = RgbdsSymbols::parse(text).unwrap();
        assert_eq!(sym.first_label_for(0x00, 0x0000), Some("EntryPoint"));
        assert_eq!(sym.first_label_for(0x01, 0x4000), Some("Foo"));
    }

    #[test]
    fn parse_breakpoint_formats() {
        let mut snap = UiSnapshot::default();
        snap.debugger.active_rom_bank = 3;
        snap.debugger.vram_bank = 1;
        snap.debugger.wram_bank = 2;
        snap.debugger.sram_bank = 7;

        let bp = parse_breakpoint_spec("00:018D", &snap).unwrap();
        assert_eq!(bp.bank, 0);
        assert_eq!(bp.addr, 0x018D);

        let bp = parse_breakpoint_spec("03:4000", &snap).unwrap();
        assert_eq!(bp.bank, 0x03);
        assert_eq!(bp.addr, 0x4000);

        // Plain address in ROMX uses the current bank.
        let bp = parse_breakpoint_spec("4000", &snap).unwrap();
        assert_eq!(bp.bank, 0x03);
        assert_eq!(bp.addr, 0x4000);

        // Plain address in ROM0 always becomes bank 00.
        let bp = parse_breakpoint_spec("0123", &snap).unwrap();
        assert_eq!(bp.bank, 0x00);
        assert_eq!(bp.addr, 0x0123);

        // Plain address in banked regions uses the current bank.
        let bp = parse_breakpoint_spec("8000", &snap).unwrap();
        assert_eq!(bp.bank, 0x01);
        assert_eq!(bp.addr, 0x8000);

        let bp = parse_breakpoint_spec("D000", &snap).unwrap();
        assert_eq!(bp.bank, 0x02);
        assert_eq!(bp.addr, 0xD000);

        let bp = parse_breakpoint_spec("A000", &snap).unwrap();
        assert_eq!(bp.bank, 0x07);
        assert_eq!(bp.addr, 0xA000);

        assert!(parse_breakpoint_spec("", &snap).is_none());
        assert!(parse_breakpoint_spec("GG:1234", &snap).is_none());
        assert!(parse_breakpoint_spec("01:12", &snap).is_none());
    }

    #[test]
    fn substitutes_jump_targets_with_labels() {
        let sym = RgbdsSymbols::parse("00:018D ClearText\n").unwrap();

        let mut snap = UiSnapshot::default();
        snap.debugger.active_rom_bank = 1;

        let out = substitute_immediate_labels("JP $018D", &snap, Some(&sym), Some(0));
        assert_eq!(out, "JP ClearText");

        let out = substitute_immediate_labels("JP NZ,$018D", &snap, Some(&sym), Some(0));
        assert_eq!(out, "JP NZ,ClearText");

        let out = substitute_immediate_labels("LD HL,$018D", &snap, Some(&sym), Some(0));
        assert_eq!(out, "LD HL,ClearText");

        let out = substitute_immediate_labels("JR $018D", &snap, Some(&sym), Some(0));
        assert_eq!(out, "JR ClearText");
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

    #[test]
    fn run_to_cursor_warns_when_cursor_behind_pc() {
        let mut state = DebuggerState::default();
        let mut snap = UiSnapshot::default();
        snap.cpu.pc = 0x0105;
        snap.debugger.active_rom_bank = 1;

        state.cursor = Some(BreakpointSpec {
            bank: 0x00,
            addr: 0x0103,
        });
        state.handle_run_to_cursor_request(&snap);

        assert!(
            state
                .status_line
                .as_deref()
                .unwrap_or_default()
                .contains("behind current PC")
        );

        let actions = state.take_actions();
        assert_eq!(
            actions.request_run_to,
            Some(DebuggerRunToRequest {
                target: BreakpointSpec {
                    bank: 0x00,
                    addr: 0x0103
                },
                ignore_breakpoints: false,
            })
        );
    }
}
