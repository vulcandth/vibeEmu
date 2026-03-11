use std::ops::RangeInclusive;

/// Reason a watchpoint was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchpointTrigger {
    /// The watched address was read.
    Read,
    /// The watched address was written.
    Write,
    /// The CPU executed an instruction at the watched address.
    Execute,
    /// The CPU jumped to the watched address.
    Jump,
}

/// A single configurable watchpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watchpoint {
    /// Unique identifier assigned by the debugger.
    pub id: u32,
    /// Whether this watchpoint is currently active.
    pub enabled: bool,
    /// Address range that triggers this watchpoint.
    pub range: RangeInclusive<u16>,
    /// Trigger on memory reads.
    pub on_read: bool,
    /// Trigger on memory writes.
    pub on_write: bool,
    /// Trigger on instruction execution.
    pub on_execute: bool,
    /// Trigger on jump/branch targets.
    pub on_jump: bool,
    /// Optional value filter; `None` matches any value.
    pub value_match: Option<u8>,
    /// Optional human-readable label shown when the watchpoint fires.
    pub message: Option<String>,
}

impl Watchpoint {
    /// Returns `true` if `addr` falls within this watchpoint's range.
    pub fn matches_addr(&self, addr: u16) -> bool {
        self.range.contains(&addr)
    }

    /// Returns `true` if `value` satisfies the optional value filter.
    pub fn matches_value(&self, value: Option<u8>) -> bool {
        match (self.value_match, value) {
            (None, _) => true,
            (Some(expected), Some(actual)) => expected == actual,
            (Some(_), None) => false,
        }
    }
}

/// Details of a watchpoint that fired during emulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchpointHit {
    /// ID of the watchpoint that fired.
    pub id: u32,
    /// The kind of access that triggered this hit.
    pub trigger: WatchpointTrigger,
    /// The memory address that caused the hit.
    pub addr: u16,
    /// The value read or written, if applicable.
    pub value: Option<u8>,
    /// The CPU program counter at the time of the hit, if known.
    pub pc: Option<u16>,
}

/// Engine that evaluates a set of watchpoints against memory accesses.
///
/// # Examples
///
/// ```
/// use vibe_emu_core::watchpoints::{Watchpoint, WatchpointEngine, WatchpointTrigger};
///
/// let mut engine = WatchpointEngine::default();
/// engine.set_watchpoints(vec![Watchpoint {
///     id: 1,
///     enabled: true,
///     range: 0xC000..=0xC000,
///     on_read: true,
///     on_write: false,
///     on_execute: false,
///     on_jump: false,
///     value_match: None,
///     message: None,
/// }]);
///
/// engine.note_read(Some(0x0100), 0xC000, 0x42);
/// if let Some(hit) = engine.take_hit() {
///     assert_eq!(hit.trigger, WatchpointTrigger::Read);
///     assert_eq!(hit.addr, 0xC000);
/// }
/// ```
#[derive(Debug, Default, Clone)]
pub struct WatchpointEngine {
    watchpoints: Vec<Watchpoint>,
    has_read: bool,
    has_write: bool,
    suspended: bool,
    pending_hit: Option<WatchpointHit>,
}

impl WatchpointEngine {
    /// Replace the active watchpoint list.
    pub fn set_watchpoints(&mut self, watchpoints: Vec<Watchpoint>) {
        self.watchpoints = watchpoints;
        self.recompute_fast_paths();
        self.pending_hit = None;
    }

    /// Returns a slice of the currently registered watchpoints.
    pub fn watchpoints(&self) -> &[Watchpoint] {
        &self.watchpoints
    }

    /// Suspend or resume watchpoint evaluation.
    ///
    /// While suspended, no hits are recorded. Suspending also clears any pending hit.
    pub fn set_suspended(&mut self, value: bool) {
        self.suspended = value;
        if value {
            self.pending_hit = None;
        }
    }

    /// Returns `true` if watchpoint evaluation is currently suspended.
    pub fn suspended(&self) -> bool {
        self.suspended
    }

    /// Take the pending hit, leaving `None` in its place.
    pub fn take_hit(&mut self) -> Option<WatchpointHit> {
        self.pending_hit.take()
    }

    /// Discard any pending hit without returning it.
    pub fn clear_hit(&mut self) {
        self.pending_hit = None;
    }

    /// Notify the engine of a memory read at `addr` with `value`.
    pub fn note_read(&mut self, pc: Option<u16>, addr: u16, value: u8) {
        if self.suspended || !self.has_read || self.pending_hit.is_some() {
            return;
        }

        for wp in &self.watchpoints {
            if !wp.enabled || !wp.on_read || !wp.matches_addr(addr) {
                continue;
            }
            if !wp.matches_value(Some(value)) {
                continue;
            }
            self.pending_hit = Some(WatchpointHit {
                id: wp.id,
                trigger: WatchpointTrigger::Read,
                addr,
                value: Some(value),
                pc,
            });
            return;
        }
    }

    /// Notify the engine of a memory write of `value` to `addr`.
    pub fn note_write(&mut self, pc: Option<u16>, addr: u16, value: u8) {
        if self.suspended || !self.has_write || self.pending_hit.is_some() {
            return;
        }

        for wp in &self.watchpoints {
            if !wp.enabled || !wp.on_write || !wp.matches_addr(addr) {
                continue;
            }
            if !wp.matches_value(Some(value)) {
                continue;
            }
            self.pending_hit = Some(WatchpointHit {
                id: wp.id,
                trigger: WatchpointTrigger::Write,
                addr,
                value: Some(value),
                pc,
            });
            return;
        }
    }

    fn recompute_fast_paths(&mut self) {
        self.has_read = self.watchpoints.iter().any(|wp| wp.enabled && wp.on_read);
        self.has_write = self.watchpoints.iter().any(|wp| wp.enabled && wp.on_write);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wp(id: u32, range: RangeInclusive<u16>) -> Watchpoint {
        Watchpoint {
            id,
            enabled: true,
            range,
            on_read: false,
            on_write: false,
            on_execute: false,
            on_jump: false,
            value_match: None,
            message: None,
        }
    }

    #[test]
    fn read_watchpoint_hits_and_records_details() {
        let mut engine = WatchpointEngine::default();
        let mut w = wp(1, 0xC000..=0xC000);
        w.on_read = true;
        engine.set_watchpoints(vec![w]);

        engine.note_read(Some(0x0100), 0xC000, 0x12);
        assert_eq!(
            engine.take_hit(),
            Some(WatchpointHit {
                id: 1,
                trigger: WatchpointTrigger::Read,
                addr: 0xC000,
                value: Some(0x12),
                pc: Some(0x0100),
            })
        );
    }

    #[test]
    fn value_match_filters_hits() {
        let mut engine = WatchpointEngine::default();
        let mut w = wp(1, 0xC000..=0xC000);
        w.on_write = true;
        w.value_match = Some(0xAA);
        engine.set_watchpoints(vec![w]);

        engine.note_write(Some(0x0100), 0xC000, 0x12);
        assert_eq!(engine.take_hit(), None);

        engine.note_write(Some(0x0100), 0xC000, 0xAA);
        assert!(engine.take_hit().is_some());
    }

    #[test]
    fn suspended_disables_hits() {
        let mut engine = WatchpointEngine::default();
        let mut w = wp(1, 0xC000..=0xC000);
        w.on_read = true;
        engine.set_watchpoints(vec![w]);

        engine.set_suspended(true);
        engine.note_read(Some(0x0100), 0xC000, 0x12);
        assert_eq!(engine.take_hit(), None);

        engine.set_suspended(false);
        engine.note_read(Some(0x0100), 0xC000, 0x12);
        assert!(engine.take_hit().is_some());
    }
}
