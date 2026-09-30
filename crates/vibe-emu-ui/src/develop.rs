use super::*;
use develop_layout::Panel;
use egui_dock::{DockArea, DockState, TabViewer};

struct Viewer<'a> {
    app: &'a mut VibeEmuApp,
    snapshot: Option<&'a UiSnapshot>,
    floating: Vec<Panel>,
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

    fn allowed_in_windows(&self, _tab: &mut Panel) -> bool {
        // egui_dock windows are confined to the root viewport. Use the native
        // Undock action instead; tab dragging still rearranges the main dock.
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        ui.horizontal(|ui| {
            let label = if self.floating.contains(tab) {
                "Dock back"
            } else {
                "Undock"
            };
            if ui
                .small_button(label)
                .on_hover_text(
                    "Open this panel in a separate desktop window, or return it to the main dock",
                )
                .clicked()
            {
                self.app.develop_layout.pending_float = Some(*tab);
            }
        });
        let Some(snapshot) = self.snapshot else {
            if *tab == Panel::Game {
                self.app.draw_empty_game(ui);
            } else {
                ui.label("Open a ROM to inspect the machine.");
            }
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
        // Toolbar actions may have stepped or edited the machine during this frame.
        let snapshot = self
            .current_rom_path
            .as_ref()
            .and(self.debugger_snapshot.clone());
        ui.separator();
        let floating = self.develop_layout.floating_panels();
        if ui.available_width() < 780.0 || ui.available_height() < 420.0 {
            // Compact navigation does not modify the saved desktop docking arrangement.
            let visible: Vec<_> = Panel::ALL
                .into_iter()
                .filter(|p| self.develop_layout.visible(*p) && !floating.contains(p))
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
                    floating: floating.clone(),
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
                        floating,
                    },
                );
            self.develop_layout.dock = dock;
        }
        self.apply_panel_window_changes();
    }

    fn apply_panel_window_changes(&mut self) {
        if let Some(panel) = self.develop_layout.pending_float.take() {
            self.develop_layout.toggle_floating(panel);
        }
        if let Err(error) = self.develop_layout.save(false) {
            self.load_error = Some(format!("Could not save workspace layout: {error}"));
        }
    }

    pub(super) fn draw_detached_panels(&mut self, ctx: &egui::Context) {
        let panels = self.develop_layout.floating_panels();
        if panels.is_empty() {
            return;
        }
        for mut panel in panels {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(("develop-native-panel", panel)),
                egui::ViewportBuilder::default()
                    .with_title(format!("{} - vibeEmu", panel.title()))
                    .with_inner_size([720.0, 520.0])
                    .with_min_inner_size([280.0, 200.0]),
                |ui, _| {
                    let ctx = ui.ctx().clone();
                    if ctx.input(|i| i.viewport().close_requested()) {
                        // Closing a tool returns it to the workspace rather than
                        // losing it. The root window and emulation stay alive.
                        self.develop_layout.pending_float = Some(panel);
                        return;
                    }
                    if ctx.input(|i| i.focused) && self.rebinding.is_none() {
                        self.handle_shortcuts(&ctx);
                    }
                    if self.current_rom_path.is_some() {
                        self.process_debugger_actions();
                        if let Ok(mut gb) = self.gb.try_lock() {
                            self.debugger_snapshot =
                                Some(UiSnapshot::from_gb(&mut gb, self.paused));
                        }
                    }
                    let snapshot = self
                        .current_rom_path
                        .as_ref()
                        .and(self.debugger_snapshot.clone());
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        let mut viewer = Viewer {
                            app: self,
                            snapshot: snapshot.as_ref(),
                            floating: vec![panel],
                        };
                        egui::ScrollArea::new(viewer.scroll_bars(&panel))
                            .show(ui, |ui| viewer.ui(ui, &mut panel));
                    });
                    if !self.paused {
                        ctx.request_repaint_after(Duration::from_millis(16));
                    }
                },
            );
            self.apply_panel_window_changes();
        }
    }
}
