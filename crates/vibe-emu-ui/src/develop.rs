use super::*;
use develop_layout::Panel;
use egui_dock::{DockArea, DockState, TabViewer};

struct Viewer<'a> {
    app: &'a mut VibeEmuApp,
    snapshot: Option<&'a UiSnapshot>,
}

impl TabViewer for Viewer<'_> {
    type Tab = Panel;

    fn title(&mut self, tab: &mut Panel) -> egui::WidgetText {
        tab.title().into()
    }

    fn id(&mut self, tab: &mut Panel) -> egui::Id {
        egui::Id::new(("develop-panel", *tab))
    }

    fn scroll_bars(&self, tab: &Panel) -> [bool; 2] {
        match tab {
            Panel::Registers | Panel::Watchpoints | Panel::Video => [true, true],
            _ => [false, false],
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        let Some(snapshot) = self.snapshot else {
            ui.label("Open a ROM to inspect the machine.");
            return;
        };
        match tab {
            Panel::Disassembly => self.app.draw_disassembly_pane(ui, snapshot),
            Panel::Registers => self.app.draw_state_panes(ui, snapshot),
            Panel::Memory => self.app.draw_memory_viewer(ui, snapshot),
            Panel::Video => {
                self.app
                    .draw_vram_viewer_content(ui, &ui.ctx().clone(), Some(&snapshot.ppu))
            }
            Panel::Watchpoints => self.app.draw_watchpoints_content(ui),
            Panel::Game => {
                if let Some(texture) = &self.app.texture {
                    let (width, height) = self.app.frame_dimensions();
                    let factor = (ui.available_width() / width as f32)
                        .min(ui.available_height() / height as f32)
                        .max(0.01);
                    let response = ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(
                        width as f32 * factor,
                        height as f32 * factor,
                    )));
                    response.context_menu(|ui| self.app.draw_game_actions(ui));
                }
            }
        }
    }
}

impl VibeEmuApp {
    pub(super) fn draw_develop_workspace(&mut self, ui: &mut egui::Ui) {
        if self.current_rom_path.is_some() {
            self.process_debugger_actions();
            if let Ok(mut gb) = self.gb.try_lock() {
                self.debugger_snapshot = Some(UiSnapshot::from_gb(&mut gb, self.paused));
            }
        }
        let snapshot = self
            .current_rom_path
            .as_ref()
            .and(self.debugger_snapshot.clone());
        if let Some(snapshot) = &snapshot {
            self.draw_debugger_toolbar(ui, snapshot);
        }
        ui.separator();
        if ui.available_width() < 780.0 || ui.available_height() < 420.0 {
            // Compact navigation does not modify the saved desktop docking arrangement.
            let visible: Vec<_> = Panel::ALL
                .into_iter()
                .filter(|p| self.develop_layout.visible(*p))
                .collect();
            if !visible.contains(&self.develop_layout.compact_panel)
                && let Some(panel) = visible.first()
            {
                self.develop_layout.compact_panel = *panel;
            }
            ui.horizontal_wrapped(|ui| {
                for panel in &visible {
                    ui.selectable_value(
                        &mut self.develop_layout.compact_panel,
                        *panel,
                        panel.title(),
                    );
                }
            });
            if visible.is_empty() {
                ui.label(
                    "Reopen panels from View > Develop panels, or reset the workspace layout.",
                );
            } else {
                let mut panel = self.develop_layout.compact_panel;
                let mut viewer = Viewer {
                    app: self,
                    snapshot: snapshot.as_ref(),
                };
                egui::ScrollArea::new(viewer.scroll_bars(&panel))
                    .show(ui, |ui| viewer.ui(ui, &mut panel));
            }
        } else {
            let mut dock = std::mem::replace(&mut self.develop_layout.dock, DockState::new(vec![]));
            DockArea::new(&mut dock)
                .style(egui_dock::Style::from_egui(ui.style()))
                .show_inside(
                    ui,
                    &mut Viewer {
                        app: self,
                        snapshot: snapshot.as_ref(),
                    },
                );
            self.develop_layout.dock = dock;
        }
        if let Err(error) = self.develop_layout.save(false) {
            self.load_error = Some(format!("Could not save workspace layout: {error}"));
        }
    }
}
