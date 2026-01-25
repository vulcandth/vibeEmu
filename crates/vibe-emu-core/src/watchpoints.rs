use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchpointTrigger {
    Read,
    Write,
    Execute,
    Jump,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watchpoint {
    pub id: u32,
    pub enabled: bool,
    pub range: RangeInclusive<u16>,
    pub on_read: bool,
    pub on_write: bool,
    pub on_execute: bool,
    pub on_jump: bool,
    pub value_match: Option<u8>,
    pub message: Option<String>,
}

impl Watchpoint {
    pub fn matches_addr(&self, addr: u16) -> bool {
        self.range.contains(&addr)
    }

    pub fn matches_value(&self, value: Option<u8>) -> bool {
        match (self.value_match, value) {
            (None, _) => true,
            (Some(expected), Some(actual)) => expected == actual,
            (Some(_), None) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchpointHit {
    pub id: u32,
    pub trigger: WatchpointTrigger,
    pub addr: u16,
    pub value: Option<u8>,
    pub pc: Option<u16>,
}

#[derive(Debug, Default, Clone)]
pub struct WatchpointEngine {
    watchpoints: Vec<Watchpoint>,
    has_read: bool,
    has_write: bool,
    suspended: bool,
    pending_hit: Option<WatchpointHit>,
}

impl WatchpointEngine {
    pub fn set_watchpoints(&mut self, watchpoints: Vec<Watchpoint>) {
        self.watchpoints = watchpoints;
        self.recompute_fast_paths();
        self.pending_hit = None;
    }

    pub fn watchpoints(&self) -> &[Watchpoint] {
        &self.watchpoints
    }

    pub fn set_suspended(&mut self, value: bool) {
        self.suspended = value;
        if value {
            self.pending_hit = None;
        }
    }

    pub fn suspended(&self) -> bool {
        self.suspended
    }

    pub fn take_hit(&mut self) -> Option<WatchpointHit> {
        self.pending_hit.take()
    }

    pub fn clear_hit(&mut self) {
        self.pending_hit = None;
    }

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
