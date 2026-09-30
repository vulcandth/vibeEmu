use super::*;
use vibe_emu_frontend::{Action, Workspace};

impl VibeEmuApp {
    pub(super) fn action_button(&mut self, ui: &mut egui::Ui, label: &str, action: Action) {
        if ui
            .add_enabled(
                action.available(self.current_rom_path.is_some(), self.loading.is_some()),
                egui::Button::new(label),
            )
            .clicked()
        {
            self.dispatch_action(ui.ctx(), action);
            ui.close();
        }
    }

    pub(super) fn dispatch_action(&mut self, ctx: &egui::Context, action: Action) {
        if !action.available(self.current_rom_path.is_some(), self.loading.is_some()) {
            return;
        }
        match action {
            Action::OpenRom => {
                if let Some(path) = FileDialog::new()
                    .add_filter("Game Boy ROMs", &["gb", "gbc"])
                    .pick_file()
                {
                    self.pending_rom_load = Some(path);
                }
            }
            Action::ReloadRom => self.pending_rom_load = self.current_rom_path.clone(),
            Action::CloseRom => {
                if let Ok(mut gb) = self.gb.lock() {
                    gb.mmu.save_cart_ram();
                }
                self.paused = true;
                let _ = self.emu_tx.send(EmuCommand::SetPaused(true));
                self.release_gameplay_input();
                self.disconnect_serial_peripheral();
                self.current_rom_path = None;
                self.texture = None;
                self.framebuffer.clear();
                self.debugger_snapshot = None;
                self.show_game_menu = false;
            }
            Action::TogglePause => {
                self.paused = !self.paused;
                self.focus_paused = false;
                let _ = self.emu_tx.send(EmuCommand::SetPaused(self.paused));
                if let Some(tx) = &self.link_cmd_tx {
                    let _ = tx.send(if self.paused {
                        LinkCommand::NotifyPause
                    } else {
                        LinkCommand::NotifyResume
                    });
                }
            }
            Action::Reset => {
                let _ = self.emu_tx.send(EmuCommand::Reset);
            }
            Action::Screenshot => self.capture_screenshot(),
            Action::ToggleMute => {
                self.sound_enabled.fetch_xor(true, Ordering::Relaxed);
                self.persist_runtime_settings();
            }
            Action::ToggleFullscreen => {
                self.selected_window_scale = if self.selected_window_size().is_fullscreen() {
                    1
                } else {
                    6
                };
                self.apply_window_scale(ctx);
                self.persist_runtime_settings();
            }
            Action::Settings => {
                self.show_options = true;
                self.show_game_menu = false;
            }
            Action::ToggleMenu => {
                if self.show_options {
                    self.show_options = false;
                } else {
                    self.show_game_menu = !self.show_game_menu;
                }
                self.release_gameplay_input();
            }
            Action::Play | Action::Develop => {
                self.ui_config.preferences.workspace = if action == Action::Play {
                    Workspace::Play
                } else {
                    Workspace::Develop
                };
                self.save_ui_config();
                if action == Action::Develop {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1100.0, 760.0,
                    )));
                    self.debugger_state.request_scroll_to_pc();
                } else {
                    self.apply_window_scale(ctx);
                }
            }
            Action::Quit => {
                let _ = self.emu_tx.send(EmuCommand::Shutdown);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    pub(super) fn release_gameplay_input(&mut self) {
        if self.joypad_state != 0xff {
            self.joypad_state = 0xff;
            let _ = self.emu_tx.send(EmuCommand::UpdateInput {
                state: 0xff,
                at: Instant::now(),
            });
        }
        if self.sgb_joypad_states != [0xff; 3] {
            self.sgb_joypad_states = [0xff; 3];
            let _ = self.emu_tx.send(EmuCommand::UpdateSgbInput([0xff; 3]));
        }
        if self.fast_forward {
            self.fast_forward = false;
            self.apply_speed();
        }
    }

    pub(super) fn apply_speed(&self) {
        let prefs = &self.ui_config.preferences;
        let percent = if self.fast_forward {
            prefs.fast_forward_percent
        } else {
            prefs.speed_percent
        };
        let _ = self.emu_tx.send(EmuCommand::SetSpeed(Speed {
            factor: f32::from(percent) / 100.0,
            fast: false,
        }));
    }

    pub(super) fn draw_menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                self.action_button(ui, "Open ROM…", Action::OpenRom);
                ui.menu_button("Recent ROMs", |ui| {
                    for path in self
                        .ui_config
                        .pinned_roms
                        .iter()
                        .chain(
                            self.ui_config
                                .recent_roms
                                .iter()
                                .filter(|p| !self.ui_config.pinned_roms.contains(p)),
                        )
                        .cloned()
                        .collect::<Vec<_>>()
                    {
                        let name = path.file_name().unwrap_or_default().to_string_lossy();
                        if ui
                            .button(name)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            self.pending_rom_load = Some(path);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("Clear unpinned history").clicked() {
                        self.ui_config.recent_roms.clear();
                        self.save_ui_config();
                    }
                    if let Some(path) = self.current_rom_path.clone() {
                        let pinned = self.ui_config.pinned_roms.contains(&path);
                        if ui
                            .button(if pinned {
                                "Unpin current ROM"
                            } else {
                                "Pin current ROM"
                            })
                            .clicked()
                        {
                            if pinned {
                                self.ui_config.pinned_roms.retain(|p| p != &path);
                            } else {
                                self.ui_config.pinned_roms.push(path);
                            }
                            self.save_ui_config();
                        }
                    }
                });
                self.action_button(ui, "Reload ROM", Action::ReloadRom);
                self.action_button(ui, "Close ROM", Action::CloseRom);
                ui.separator();
                self.action_button(ui, "Exit", Action::Quit);
            });
            ui.menu_button("Emulation", |ui| {
                self.action_button(
                    ui,
                    if self.paused { "Resume" } else { "Pause" },
                    Action::TogglePause,
                );
                self.action_button(ui, "Reset", Action::Reset);
                ui.menu_button("Speed", |ui| {
                    for percent in [25, 50, 100, 150, 200, 400] {
                        if ui
                            .selectable_value(
                                &mut self.ui_config.preferences.speed_percent,
                                percent,
                                format!("{percent}%"),
                            )
                            .changed()
                        {
                            self.apply_speed();
                            self.save_ui_config();
                        }
                    }
                });
                ui.menu_button("Hardware mode", |ui| {
                    self.draw_emulation_mode_submenu(ui);
                });
            });
            ui.menu_button("View", |ui| {
                self.action_button(ui, "Play workspace", Action::Play);
                self.action_button(ui, "Develop workspace", Action::Develop);
                self.action_button(ui, "Fullscreen", Action::ToggleFullscreen);
                ui.menu_button("Window scale", |ui| {
                    for n in 1..=6 {
                        if ui.button(format!("{n}×")).clicked() {
                            self.selected_window_scale = n - 1;
                            self.apply_window_scale(ui.ctx());
                            self.persist_runtime_settings();
                        }
                    }
                    ui.separator();
                    for (label, index) in [("Fullscreen (integer)", 6), ("Fullscreen (fit)", 7)] {
                        if ui.button(label).clicked() {
                            self.selected_window_scale = index;
                            self.apply_window_scale(ui.ctx());
                            self.persist_runtime_settings();
                        }
                    }
                });
                if ui
                    .checkbox(&mut self.ui_config.show_sgb_border, "Show SGB border")
                    .changed()
                {
                    self.save_ui_config();
                    self.apply_window_scale(ui.ctx());
                }
                if ui
                    .checkbox(&mut self.ui_config.preferences.show_status, "Status bar")
                    .changed()
                {
                    self.save_ui_config();
                }
                ui.menu_button("Develop panels", |ui| {
                    for panel in develop_layout::Panel::ALL {
                        let mut visible = self.develop_layout.visible(panel);
                        if ui.checkbox(&mut visible, panel.title()).changed() {
                            self.develop_layout.set_visible(panel, visible);
                        }
                    }
                });
                if ui.button("Reset workspace layout").clicked() {
                    self.develop_layout.reset();
                    self.show_debugger = false;
                    self.show_watchpoints = false;
                    self.show_vram_viewer = false;
                }
            });
            ui.menu_button("Tools", |ui| {
                self.action_button(ui, "Capture screenshot", Action::Screenshot);
                self.action_button(ui, "Mute / unmute", Action::ToggleMute);
                ui.menu_button("Peripherals", |ui| {
                    self.draw_serial_peripheral_submenu(ui);
                });
                ui.separator();
                ui.checkbox(&mut self.show_debugger, "Detached debugger");
                ui.checkbox(&mut self.show_watchpoints, "Watchpoints");
                ui.checkbox(&mut self.show_vram_viewer, "VRAM viewer");
            });
            ui.menu_button("Settings", |ui| {
                self.action_button(ui, "Settings…", Action::Settings);
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Controls").clicked() {
                    self.options_tab = OptionsTab::Controls;
                    self.show_options = true;
                }
                if ui.button("About and licenses").clicked() {
                    self.show_about = true;
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            for (label, workspace, action) in [
                ("Play", Workspace::Play, Action::Play),
                ("Develop", Workspace::Develop, Action::Develop),
            ] {
                if ui
                    .selectable_label(self.ui_config.preferences.workspace == workspace, label)
                    .clicked()
                {
                    self.dispatch_action(ui.ctx(), action);
                }
            }
            ui.separator();
            self.action_button(
                ui,
                if self.paused { "Resume" } else { "Pause" },
                Action::TogglePause,
            );
            self.action_button(ui, "Reset", Action::Reset);
            ui.small(format!("{}%", self.ui_config.preferences.speed_percent));
        });
    }

    pub(super) fn draw_game_actions(&mut self, ui: &mut egui::Ui) {
        self.action_button(
            ui,
            if self.paused { "Resume" } else { "Pause" },
            Action::TogglePause,
        );
        self.action_button(ui, "Reset", Action::Reset);
        self.action_button(ui, "Screenshot", Action::Screenshot);
        self.action_button(ui, "Mute / unmute", Action::ToggleMute);
        self.action_button(ui, "Fullscreen", Action::ToggleFullscreen);
        ui.menu_button("Peripherals", |ui| {
            self.draw_serial_peripheral_submenu(ui);
        });
        self.action_button(ui, "Settings…", Action::Settings);
        self.action_button(ui, "Play workspace", Action::Play);
        self.action_button(ui, "Develop workspace", Action::Develop);
    }
}
