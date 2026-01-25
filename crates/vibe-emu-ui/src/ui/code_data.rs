use crate::ui::snapshot::UiSnapshot;
use std::collections::{HashMap, HashSet};

/// Classification state for disassembly alignment.
///
/// This is intentionally UI-facing and independent from the debugger UI so it can be reused
/// later by other tooling (e.g. importing mgbdis-style block annotations).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    /// No information yet; treat as data for rendering.
    Unknown,
    /// Start of an instruction.
    CodeStart { len: u8 },
    /// Bytes that are part of an instruction body (not an instruction boundary).
    CodeBody,
    /// Known data (non-code).
    Data,
    /// Known text (non-code).
    Text,
    /// Known image (non-code).
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    Code,
    Data,
    Text,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockRange {
    pub bank: u8,
    pub start: u16,
    pub end_inclusive: u16,
    pub kind: BlockKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExecutedInstruction {
    pub bank: u8,
    pub addr: u16,
    pub len: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrackerKey {
    active_rom_bank: u8,
    seed_revision: u64,
    forced_revision: u64,
}

/// Tracks which addresses should be treated as code vs data for disassembly rendering.
///
/// Current implementation focuses on ROM execution flow, because that's what both the debugger
/// disassembly and future ROM-oriented tools need. Non-ROM regions default to Data/Unknown.
#[derive(Debug, Clone)]
pub struct CodeDataTracker {
    key: Option<TrackerKey>,

    rom0: Vec<CellKind>,
    romx: HashMap<u8, Vec<CellKind>>,

    forced_blocks: Vec<BlockRange>,

    observed_entrypoints: HashSet<(u8, u16)>,
    seed_revision: u64,
    forced_revision: u64,
    revision: u64,
}

impl Default for CodeDataTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeDataTracker {
    pub fn new() -> Self {
        Self {
            key: None,
            rom0: vec![CellKind::Unknown; 0x4000],
            romx: HashMap::new(),
            forced_blocks: Vec::new(),
            observed_entrypoints: HashSet::new(),
            seed_revision: 0,
            forced_revision: 0,
            revision: 0,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn note_executed(&mut self, executed: impl IntoIterator<Item = ExecutedInstruction>) {
        let mut changed = false;
        for e in executed {
            if !is_rom_addr(e.addr) {
                continue;
            }

            let mut bank = e.bank;
            if e.addr < 0x4000 {
                bank = 0;
            }

            let len = e.len.max(1);

            if self.mark_code(bank, e.addr, len) {
                changed = true;
            }

            if self.observe_entrypoint(bank, e.addr) {
                changed = true;
            }
        }

        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn apply_forced_blocks(&mut self, blocks: impl IntoIterator<Item = BlockRange>) {
        self.forced_blocks.clear();
        self.forced_blocks.extend(blocks);
        let code_seeds: Vec<(u8, u16)> = self
            .forced_blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Code))
            .map(|b| (b.bank, b.start))
            .collect();
        for (bank, addr) in code_seeds {
            let _ = self.observe_entrypoint(bank, addr);
        }

        self.forced_revision = self.forced_revision.wrapping_add(1);
        self.key = None;
    }

    pub fn observe_entrypoint(&mut self, bank: u8, addr: u16) -> bool {
        let inserted = self.observed_entrypoints.insert((bank, addr));
        if !inserted {
            return false;
        }

        self.seed_revision = self.seed_revision.wrapping_add(1);
        self.key = None;
        true
    }

    pub fn kind_at(&self, addr: u16, active_rom_bank: u8) -> CellKind {
        if addr < 0x4000 {
            return self
                .rom0
                .get(addr as usize)
                .copied()
                .unwrap_or(CellKind::Unknown);
        }

        if addr < 0x8000 {
            let idx = (addr - 0x4000) as usize;
            return self
                .romx
                .get(&active_rom_bank)
                .and_then(|cells| cells.get(idx))
                .copied()
                .unwrap_or(CellKind::Unknown);
        }

        CellKind::Data
    }

    pub fn code_len_at(&self, addr: u16, active_rom_bank: u8) -> Option<u8> {
        match self.kind_at(addr, active_rom_bank) {
            CellKind::CodeStart { len } if len != 0 => Some(len),
            _ => None,
        }
    }

    pub fn ensure_up_to_date(&mut self, snapshot: &UiSnapshot) {
        let Some(mem) = snapshot.debugger.mem_image.as_deref().map(|m| m.as_slice()) else {
            self.key = None;
            self.rom0.fill(CellKind::Unknown);
            self.romx.clear();
            return;
        };

        // Always observe the current PC (and vectors) as a seed. If this introduces a new
        // entrypoint, we rebuild even when the mapper state hasn't changed.
        self.observe_common_entrypoints(snapshot);

        let active_bank = snapshot.debugger.active_rom_bank.min(0xFF) as u8;
        let key = TrackerKey {
            active_rom_bank: active_bank,
            seed_revision: self.seed_revision,
            forced_revision: self.forced_revision,
        };

        if self.key == Some(key) {
            return;
        }

        self.key = Some(key);

        let mut work: Vec<(u8, u16)> = Vec::new();
        let mut queued: HashSet<(u8, u16)> = HashSet::new();

        for &(bank, addr) in &self.observed_entrypoints {
            if (bank == 0 || bank == active_bank) && queued.insert((bank, addr)) {
                work.push((bank, addr));
            }
        }

        let mut popped = 0u32;

        while let Some((bank, start)) = work.pop() {
            popped = popped.wrapping_add(1);
            if !is_rom_addr(start) {
                continue;
            }

            // For ROMX addresses we can only analyze the *active* bank, since mem is a mapped view.
            if (0x4000..0x8000).contains(&start) && bank != active_bank {
                continue;
            }

            let mut pc = start;
            let mut steps = 0u32;
            while steps < 0x2000 {
                steps += 1;

                if !is_rom_addr(pc) {
                    break;
                }

                if is_forced_non_code(&self.forced_blocks, bank, pc) {
                    break;
                }

                let (existing_len, already_boundary) = match self.kind_at(pc, active_bank) {
                    CellKind::CodeStart { len } => (len.max(1), true),
                    CellKind::CodeBody => break,
                    _ => (0, false),
                };

                let opcode = mem.get(pc as usize).copied().unwrap_or(0x00);
                let len = if already_boundary {
                    existing_len
                } else {
                    sm83_instr_len(opcode)
                };

                if self.mark_code(bank, pc, len) {
                    // Marking new code may unlock more UI cache usage.
                }

                let flows = flow_edges(pc, opcode, mem, len, active_bank);
                if let Some(target) = flows.jump_target {
                    let target_bank = if target < 0x4000 { 0 } else { bank };
                    if queued.insert((target_bank, target)) {
                        work.push((target_bank, target));
                    }
                }
                if let Some(target) = flows.call_target {
                    let target_bank = if target < 0x4000 { 0 } else { bank };
                    if queued.insert((target_bank, target)) {
                        work.push((target_bank, target));
                    }
                }

                if flows.stop_linear {
                    break;
                }

                pc = pc.wrapping_add(len as u16);

                if is_forced_non_code(&self.forced_blocks, bank, pc) {
                    break;
                }
            }
        }

        // Apply forced blocks as the final authority.
        self.apply_forced_block_overlay(active_bank);

        self.revision = self.revision.wrapping_add(1);
    }

    fn observe_common_entrypoints(&mut self, snapshot: &UiSnapshot) {
        let active_bank = snapshot.debugger.active_rom_bank.min(0xFF) as u8;

        // Cartridge entrypoint (ROM0). After the boot ROM, execution continues at $0100.
        let _ = self.observe_entrypoint(0, 0x0100);

        // Current PC as an execution hint.
        let pc = snapshot.cpu.pc;
        let bank = if pc < 0x4000 { 0 } else { active_bank };
        let _ = self.observe_entrypoint(bank, pc);
    }

    fn apply_forced_block_overlay(&mut self, active_rom_bank: u8) {
        let forced_blocks = self.forced_blocks.clone();
        for b in forced_blocks {
            let end = b.end_inclusive.max(b.start);
            let mut addr = b.start;
            loop {
                let mapped_bank = if addr < 0x4000 { 0 } else { active_rom_bank };
                if mapped_bank == b.bank {
                    let cell = match b.kind {
                        BlockKind::Code => self.kind_at(addr, active_rom_bank),
                        BlockKind::Data => CellKind::Data,
                        BlockKind::Text => CellKind::Text,
                        BlockKind::Image => CellKind::Image,
                    };

                    // Forced non-code blocks should override whatever was inferred.
                    if !matches!(b.kind, BlockKind::Code) {
                        self.set_cell(mapped_bank, addr, cell);
                    }
                }

                if addr == end {
                    break;
                }
                addr = addr.wrapping_add(1);
            }
        }
    }

    fn mark_code(&mut self, bank: u8, addr: u16, len: u8) -> bool {
        let len = len.max(1);
        let mut changed = false;

        let current = self.kind_at(addr, bank);
        if !matches!(current, CellKind::CodeStart { .. }) {
            self.set_cell(bank, addr, CellKind::CodeStart { len });
            changed = true;
        }

        for i in 1..len {
            let a = addr.wrapping_add(i as u16);
            if a >= 0x8000 {
                break;
            }

            let cur = self.kind_at(a, bank);
            if matches!(
                cur,
                CellKind::Unknown | CellKind::Data | CellKind::Text | CellKind::Image
            ) {
                self.set_cell(bank, a, CellKind::CodeBody);
                changed = true;
            }
        }

        changed
    }

    fn set_cell(&mut self, bank: u8, addr: u16, kind: CellKind) {
        if addr < 0x4000 {
            if let Some(cell) = self.rom0.get_mut(addr as usize) {
                *cell = kind;
            }
            return;
        }

        if addr < 0x8000 {
            let idx = (addr - 0x4000) as usize;
            let bank_cells = self
                .romx
                .entry(bank)
                .or_insert_with(|| vec![CellKind::Unknown; 0x4000]);
            if let Some(cell) = bank_cells.get_mut(idx) {
                *cell = kind;
            }
        }
    }
}

fn is_rom_addr(addr: u16) -> bool {
    addr <= 0x7FFF
}

fn is_forced_non_code(blocks: &[BlockRange], bank: u8, addr: u16) -> bool {
    blocks.iter().any(|b| {
        b.bank == bank
            && addr >= b.start
            && addr <= b.end_inclusive
            && !matches!(b.kind, BlockKind::Code)
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct Flow {
    stop_linear: bool,
    jump_target: Option<u16>,
    call_target: Option<u16>,
}

fn read_u16_le(mem: &[u8], addr: u16) -> u16 {
    let lo = mem.get(addr as usize).copied().unwrap_or(0);
    let hi = mem.get(addr.wrapping_add(1) as usize).copied().unwrap_or(0);
    u16::from_le_bytes([lo, hi])
}

pub fn sm83_instr_len(opcode: u8) -> u8 {
    match opcode {
        0xCB => 2,

        // 3-byte (imm16)
        0x01 | 0x08 | 0x11 | 0x21 | 0x31 | 0xC2 | 0xC3 | 0xC4 | 0xCA | 0xCC | 0xCD | 0xD2
        | 0xD4 | 0xDA | 0xDC | 0xEA | 0xFA => 3,

        // 2-byte (imm8 / rel8)
        0x06 | 0x0E | 0x10 | 0x16 | 0x18 | 0x1E | 0x20 | 0x26 | 0x28 | 0x2E | 0x30 | 0x36
        | 0x38 | 0x3E | 0xC6 | 0xCE | 0xD6 | 0xDE | 0xE0 | 0xE6 | 0xE8 | 0xEE | 0xF0 | 0xF6
        | 0xF8 | 0xFE => 2,

        _ => 1,
    }
}

fn flow_edges(pc: u16, opcode: u8, mem: &[u8], len: u8, active_rom_bank: u8) -> Flow {
    let mut out = Flow::default();

    match opcode {
        // Unconditional JP a16
        0xC3 => {
            let target = read_u16_le(mem, pc.wrapping_add(1));
            if is_rom_addr(target) {
                let _ = active_rom_bank;
                out.jump_target = Some(target);
            }
            out.stop_linear = true;
        }
        // Conditional JP a16 (keep linear + follow target)
        0xC2 | 0xCA | 0xD2 | 0xDA => {
            let target = read_u16_le(mem, pc.wrapping_add(1));
            if is_rom_addr(target) {
                let _ = active_rom_bank;
                out.jump_target = Some(target);
            }
        }
        // JR r8 (unconditional)
        0x18 => {
            let off = mem.get(pc.wrapping_add(1) as usize).copied().unwrap_or(0) as i8;
            let target = pc.wrapping_add(len as u16).wrapping_add(off as i16 as u16);
            if is_rom_addr(target) {
                let _ = active_rom_bank;
                out.jump_target = Some(target);
            }
            out.stop_linear = true;
        }
        // JR cc,r8
        0x20 | 0x28 | 0x30 | 0x38 => {
            let off = mem.get(pc.wrapping_add(1) as usize).copied().unwrap_or(0) as i8;
            let target = pc.wrapping_add(len as u16).wrapping_add(off as i16 as u16);
            if is_rom_addr(target) {
                let _ = active_rom_bank;
                out.jump_target = Some(target);
            }
        }
        // CALL a16
        0xCD => {
            let target = read_u16_le(mem, pc.wrapping_add(1));
            if is_rom_addr(target) {
                let _ = active_rom_bank;
                out.call_target = Some(target);
            }
        }
        // CALL cc,a16
        0xC4 | 0xCC | 0xD4 | 0xDC => {
            let target = read_u16_le(mem, pc.wrapping_add(1));
            if is_rom_addr(target) {
                let _ = active_rom_bank;
                out.call_target = Some(target);
            }
        }
        // RST vectors
        0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
            let vec = (opcode & 0x38) as u16;
            let _ = active_rom_bank;
            out.call_target = Some(vec);
        }
        // RET / RETI
        0xC9 | 0xD9 => {
            out.stop_linear = true;
        }
        // JP (HL)
        0xE9 => {
            out.stop_linear = true;
        }
        _ => {
            let _ = mem;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_vectors_and_marks_simple_flow_as_code() {
        let mut snap = UiSnapshot::default();
        snap.debugger.paused = true;
        snap.debugger.active_rom_bank = 1;
        snap.cpu.pc = 0x0100;

        // Minimal ROM0 with a JP $0103 at the cartridge entrypoint.
        let mut mem = Box::new([0u8; 0x10000]);
        mem[0x0100] = 0xC3;
        mem[0x0101] = 0x03;
        mem[0x0102] = 0x01;
        mem[0x0103] = 0x00;
        snap.debugger.mem_image = Some(mem);

        let mut tracker = CodeDataTracker::new();
        tracker.ensure_up_to_date(&snap);

        let active_bank = snap.debugger.active_rom_bank.min(0xFF) as u8;

        assert!(matches!(
            tracker.kind_at(0x0100, active_bank),
            CellKind::CodeStart { .. }
        ));
        assert!(matches!(
            tracker.kind_at(0x0101, active_bank),
            CellKind::CodeBody
        ));
        assert!(matches!(
            tracker.kind_at(0x0103, active_bank),
            CellKind::CodeStart { .. }
        ));
    }
}
