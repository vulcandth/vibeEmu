use super::*;
use vibe_emu_core::state_store::{Slot, StateStore};

pub enum Request {
    List,
    Save(Slot),
    Load(Slot),
    Import(PathBuf),
    Export(PathBuf),
}
pub struct Reply {
    pub message: String,
    pub restored: bool,
    pub rows: Vec<(Slot, String, bool)>,
}
impl Reply {
    pub fn error(message: String) -> Self {
        Self {
            message,
            restored: false,
            rows: Vec::new(),
        }
    }
}
#[derive(Default)]
pub struct StateUi {
    pub open: bool,
    pub pending: Option<mpsc::Receiver<Reply>>,
    message: String,
    notify_until: Option<Instant>,
    rows: Vec<(Slot, String, bool)>,
}

pub fn execute(gb: &mut GameBoy, root: &std::path::Path, request: Request) -> Reply {
    let store = match StateStore::new(root, gb) {
        Ok(s) => s,
        Err(e) => return Reply::error(e.to_string()),
    };
    let restores = matches!(request, Request::Load(_) | Request::Import(_));
    let result = match request {
        Request::List => Ok(()),
        Request::Save(slot) => store.save(gb, slot),
        Request::Load(slot) => store.load(gb, slot),
        Request::Import(path) => store.import(gb, &path),
        Request::Export(path) => store.export(gb, &path),
    };
    let restored = restores && result.is_ok();
    let message = match result {
        Ok(()) if restored => {
            "State loaded. Cartridge SRAM restored; pre-load state is in Recovery.".into()
        }
        Ok(()) => "Ready. Saves include all cartridge SRAM.".into(),
        Err(e) => format!("State operation failed: {e}"),
    };
    let rows = Slot::ALL
        .into_iter()
        .map(|slot| {
            let (label, available) = match store.metadata(slot) {
                Ok(Some(m)) => (
                    format!(
                        "{:?} · cycle {} · Unix {}",
                        m.model, m.cycles, m.created_unix
                    ),
                    true,
                ),
                Ok(None) => ("Empty".into(), false),
                Err(e) => (format!("Unavailable: {e}"), false),
            };
            (slot, label, available)
        })
        .collect();
    Reply {
        message,
        restored,
        rows,
    }
}

impl VibeEmuApp {
    pub(super) fn start_state_operation(&mut self, request: Request) {
        if self.states.pending.is_some()
            || self.loading.is_some()
            || self.current_rom_path.is_none()
        {
            return;
        }
        self.release_gameplay_input();
        let (tx, rx) = mpsc::channel();
        let root = self.ui_config_path.with_file_name("states");
        self.states.message = "Working…".into();
        self.states.notify_until = Some(Instant::now() + Duration::from_secs(5));
        if self
            .emu_tx
            .send(EmuCommand::State(request, root, tx))
            .is_ok()
        {
            self.states.pending = Some(rx);
        } else {
            self.states.message = "Emulator worker is unavailable".into();
        }
    }

    pub(super) fn poll_state_operation(&mut self) {
        let Some(rx) = &self.states.pending else {
            return;
        };
        let reply = match rx.try_recv() {
            Ok(reply) => reply,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Reply::error("State worker stopped unexpectedly".into())
            }
        };
        self.states.pending = None;
        self.states.message = reply.message;
        self.states.notify_until = Some(Instant::now() + Duration::from_secs(5));
        self.states.rows = reply.rows;
        if reply.restored {
            self.mem_edit = None;
            self.debugger_snapshot = None;
            self.cached_ppu_snapshot = None;
            self.debugger_state.request_scroll_to_pc();
            while self.frame_rx.try_recv().is_ok() {}
            if let Ok(mut gb) = self.gb.lock() {
                // Host input must be reconciled after restoring historical input.
                gb.mmu.input.set_state(self.joypad_state);
                for (i, state) in self.sgb_joypad_states.iter().enumerate() {
                    gb.mmu.input.set_player_state(i + 1, *state);
                }
                self.framebuffer = gb.mmu.ppu.display_framebuffer().to_vec();
                self._audio_stream = audio::start_stream(
                    &mut gb.mmu.apu,
                    true,
                    self.sound_enabled.clone(),
                    self.audio_controls.clone(),
                );
            }
        }
    }

    pub(super) fn draw_save_states(&mut self, ctx: &egui::Context) {
        let enabled = self.loading.is_none() && self.current_rom_path.is_some();
        if let Some(request) = self.states.draw(ctx, enabled) {
            self.start_state_operation(request);
        }
    }
}

impl StateUi {
    fn draw(&mut self, ctx: &egui::Context, enabled: bool) -> Option<Request> {
        if !self.open {
            if self.pending.is_some()
                || self
                    .notify_until
                    .is_some_and(|until| Instant::now() < until)
            {
                egui::Area::new(egui::Id::new("state-notification"))
                    .anchor(egui::Align2::RIGHT_TOP, [-12.0, 64.0])
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_max_width(300.0);
                            ui.label(&self.message);
                        });
                    });
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            return None;
        }
        let mut open = true;
        let mut request = None;
        let busy = self.pending.is_some();
        egui::Window::new("Save states")
            .default_width(520.0)
            .max_width((ctx.content_rect().width() - 24.0).max(200.0)).open(&mut open).resizable(true).show(ctx, |ui| {
            ui.label("Loading replaces cartridge SRAM. Undo load restores the pre-load machine and SRAM.");
            ui.label("Disconnect link cable / Mobile Adapter before saving or loading.");
            ui.label(&self.message);
            ui.add_enabled_ui(!busy && enabled, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Refresh").clicked() { request = Some(Request::List); }
                    if ui.button("Import and load…").clicked()
                        && let Some(path) = FileDialog::new().add_filter("vibeEmu state", &["vstate"]).pick_file() { request = Some(Request::Import(path)); }
                    if ui.button("Export current state…").clicked()
                        && let Some(path) = FileDialog::new().add_filter("vibeEmu state", &["vstate"]).set_file_name("game.vstate").save_file() { request = Some(Request::Export(path)); }
                });
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (slot, metadata, available) in &self.rows {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(slot.label());
                                if *slot != Slot::Recovery && ui.button("Save / overwrite").clicked() { request = Some(Request::Save(*slot)); }
                                if ui.add_enabled(*available, egui::Button::new("Load")).clicked() { request = Some(Request::Load(*slot)); }
                            });
                            ui.small(metadata);
                        });
                    }
                });
            });
        });
        self.open = open;
        if busy {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_browser_renders_busy_errors_and_reopening_in_small_windows() {
        let ctx = egui::Context::default();
        let mut state = StateUi {
            open: true,
            rows: Slot::ALL
                .into_iter()
                .map(|s| (s, "Empty".into(), false))
                .collect(),
            ..Default::default()
        };
        for size in [egui::vec2(320.0, 300.0), egui::vec2(1100.0, 760.0)] {
            for busy in [false, true, false] {
                let (_tx, rx) = mpsc::channel();
                state.pending = busy.then_some(rx);
                state.message =
                    "A failed import leaves the current machine and Recovery unchanged.".into();
                state.open = true;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| {
                        assert!(state.draw(ui.ctx(), true).is_none());
                    },
                );
                assert!(!output.shapes.is_empty());
                state.open = false;
                let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                    assert!(state.draw(ui.ctx(), true).is_none());
                });
            }
        }
    }

    #[test]
    fn repeated_load_undo_and_failed_import_preserve_machine_and_recovery() {
        let root = std::env::temp_dir().join(format!("vibe-state-ui-{}", std::process::id()));
        let mut gb = GameBoy::new(Model::default());
        gb.load_cart(Cartridge::from_bytes(vec![0; 0x8000]));
        let list = execute(&mut gb, &root, Request::List);
        assert_eq!(list.rows.len(), 12);
        gb.cpu.a = 17;
        assert!(!execute(&mut gb, &root, Request::Save(Slot::Quick)).restored);
        gb.cpu.a = 42;
        assert!(execute(&mut gb, &root, Request::Load(Slot::Quick)).restored);
        assert_eq!(gb.cpu.a, 17);
        assert!(execute(&mut gb, &root, Request::Load(Slot::Recovery)).restored);
        assert_eq!(gb.cpu.a, 42);
        assert!(execute(&mut gb, &root, Request::Load(Slot::Recovery)).restored);
        assert_eq!(gb.cpu.a, 17);
        let failed = execute(&mut gb, &root, Request::Import(root.join("missing")));
        assert!(!failed.restored);
        assert!(failed.message.contains("failed"));
        assert_eq!(gb.cpu.a, 17);
        std::fs::remove_dir_all(root).unwrap();
    }
}
