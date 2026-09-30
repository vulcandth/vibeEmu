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
                crash_report::set_rom(None);
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
                submenu(ui, "Recent ROMs", |ui| {
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
                submenu(ui, "Speed", |ui| {
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
                submenu(ui, "Hardware mode", |ui| {
                    self.draw_emulation_mode_submenu(ui);
                });
            });
            ui.menu_button("View", |ui| {
                self.action_button(ui, "Play workspace", Action::Play);
                self.action_button(ui, "Develop workspace", Action::Develop);
                self.action_button(ui, "Fullscreen", Action::ToggleFullscreen);
                submenu(ui, "Window scale", |ui| {
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
                submenu(ui, "Develop panels", |ui| {
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
                submenu(ui, "Peripherals", |ui| {
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
        submenu(ui, "Peripherals", |ui| {
            self.draw_serial_peripheral_submenu(ui);
        });
        self.action_button(ui, "Settings…", Action::Settings);
        self.action_button(ui, "Play workspace", Action::Play);
        self.action_button(ui, "Develop workspace", Action::Develop);
    }
}

/// Retain native cascading menus and their hover/keyboard ownership. Bound the
/// child to the space beside its parent so egui's automatic placement can pick
/// left/right instead of falling back below and obscuring sibling actions.
pub(super) fn submenu(
    ui: &mut egui::Ui,
    title: &str,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    use egui::containers::menu;
    if !menu::is_in_menu(ui) {
        return ui.menu_button(title, content).response;
    }
    let screen = ui.ctx().content_rect();
    let parent = ui
        .ctx()
        .read_response(menu::find_menu_root(ui).id)
        .map_or(ui.max_rect(), |response| response.rect);
    let margin = egui::Frame::menu(ui.style()).total_margin().sum();
    // Native submenu anchors are inset by half the menu frame; their gap is
    // that inset plus two points. Include the child's frame in the budget.
    let side_room = (parent.left() - screen.left()).max(screen.right() - parent.right());
    // Leave a few points for pixel rounding and the scroll area's inner spacing.
    let width = (side_room - margin.x - 8.0).max(24.0);
    let y = ui.available_rect_before_wrap().top();
    let height = (screen.bottom() - y).max(y - screen.top()) - margin.y - 20.0;
    ui.menu_button(title, |ui| {
        ui.set_max_width(width);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        egui::ScrollArea::vertical()
            .id_salt(title)
            .max_height(height.max(40.0))
            .show(ui, content);
    })
    .response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascading_submenus_stay_beside_parent_at_both_window_edges() {
        for (width, menu_x) in [
            (320.0, 154.0),
            (360.0, 154.0),
            (514.0, 154.0),
            (1100.0, 900.0),
            (1100.0, 154.0),
        ] {
            let ctx = egui::Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let draw = |events| {
                let mut rects = [egui::Rect::NOTHING; 4];
                let _ = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 400.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::MenuBar::new().ui(ui, |ui| {
                            ui.add_space(menu_x);
                            rects[0] = ui
                                .menu_button("Tools", |ui| {
                                    let _ = ui.button("Capture screenshot");
                                    let _ = ui.button("Mute / unmute");
                                    rects[1] = submenu(ui, "Peripherals", |ui| {
                                        let _ = ui.radio(false, "None");
                                        let _ = ui.radio(false, "Mobile Adapter");
                                        let _ = ui.radio(false, "Link Cable (Network)");
                                        rects[2] = ui.min_rect();
                                    })
                                    .rect;
                                    rects[3] = ui.button("Detached debugger").rect;
                                })
                                .response
                                .rect;
                        });
                    },
                );
                rects
            };
            let click = |position| {
                for pressed in [true, false] {
                    draw(vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ]);
                }
            };
            draw(vec![]);
            let rects = draw(vec![]);
            click(rects[0].center());
            let rects = draw(vec![]);
            assert!(rects[1].is_positive(), "parent must open");
            click(rects[1].center());
            draw(vec![]);
            let rects = draw(vec![]);
            assert!(rects[2].is_positive(), "submenu must open");
            assert!(rects[3].is_positive(), "parent must stay open");
            assert!(
                !rects[2].intersects(rects[3]),
                "{width}: child overlaps sibling: {rects:?}"
            );
            assert!(
                rects[2].left() >= rects[1].right() || rects[2].right() <= rects[1].left(),
                "{width}: child must cascade beside parent: {rects:?}"
            );
            assert!(rects[2].left() >= 0.0 && rects[2].right() <= width);
        }
    }
}
