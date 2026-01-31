// Watchpoints module - placeholder for egui port
use vibe_emu_core::watchpoints::{Watchpoint, WatchpointHit, WatchpointTrigger};

#[derive(Debug, Default, Clone)]
pub struct WatchpointsState {
    watchpoints: Vec<Watchpoint>,
    selected: Option<usize>,
    pending_sync: bool,
    status_line: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct WatchpointsUiActions {
    pub watchpoints_updated: bool,
    pub watchpoints: Vec<Watchpoint>,
}

impl WatchpointsState {
    pub fn take_actions(&mut self) -> WatchpointsUiActions {
        let updated = std::mem::take(&mut self.pending_sync);
        WatchpointsUiActions {
            watchpoints_updated: updated,
            watchpoints: if updated {
                self.watchpoints.clone()
            } else {
                Vec::new()
            },
        }
    }

    pub fn note_watchpoint_hit(&mut self, hit: &WatchpointHit) {
        let label = match hit.trigger {
            WatchpointTrigger::Read => "read",
            WatchpointTrigger::Write => "write",
            WatchpointTrigger::Execute => "execute",
            WatchpointTrigger::Jump => "jump",
        };
        self.status_line = Some(format!("Watchpoint hit: {} at {:04X}", label, hit.addr));
    }
}
