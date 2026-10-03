use super::*;
use vibe_emu_frontend::{Action, Workspace};

impl VibeEmuApp {
    pub(super) fn draw_empty_game(&mut self, ui: &mut egui::Ui) {
        if self.logo_texture.is_none()
            && let Some(icon) = load_window_icon()
        {
            self.logo_texture = Some(ui.ctx().load_texture(
                "vibeEmu logo",
                egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                ),
                egui::TextureOptions::LINEAR,
            ));
        }
        let available = ui.available_rect_before_wrap();
        let side = available
            .width()
            .min((available.height() - 48.0).max(24.0))
            .min(280.0);
        let rect = egui::Rect::from_center_size(
            available.center() - egui::vec2(0.0, 18.0),
            egui::Vec2::splat(side),
        );
        if let Some(logo) = &self.logo_texture {
            ui.put(rect, egui::Image::new(logo).fit_to_exact_size(rect.size()));
        }
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(available.left(), rect.bottom() + 8.0),
            available.max,
        );
        ui.put(
            text_rect,
            egui::Label::new(format!(
                "Open a ROM ({}) or drop it here",
                self.shortcut_text(ui.ctx(), Action::OpenRom)
            ))
            .wrap(),
        );
    }

    pub(super) fn action_button(&mut self, ui: &mut egui::Ui, label: &str, action: Action) {
        let shortcut = self.shortcut_text(ui.ctx(), action);
        let mut button = egui::Button::new(label);
        if egui::containers::menu::is_in_menu(ui) {
            button = menu_command_button(ui, label, &shortcut);
        }
        if ui
            .add_enabled(self.action_enabled(action), button)
            .on_hover_text(shortcut)
            .clicked()
        {
            self.dispatch_action(ui.ctx(), action);
            ui.close();
        }
    }

    pub(super) fn dispatch_action(&mut self, ctx: &egui::Context, action: Action) {
        if !self.action_enabled(action) {
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
                self.mem_edit = None;
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
                self.mem_edit = None;
                self.paused = !self.paused;
                if self.paused {
                    self.debugger_state.request_pause();
                } else {
                    self.debugger_state.request_continue_and_focus_main();
                }
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
            Action::SaveStates => {
                self.states.open = true;
                self.start_state_operation(save_states::Request::List);
            }
            Action::QuickSave => self.start_state_operation(save_states::Request::Save(
                vibe_emu_core::state_store::Slot::Quick,
            )),
            Action::QuickLoad => self.start_state_operation(save_states::Request::Load(
                vibe_emu_core::state_store::Slot::Quick,
            )),
            Action::UndoLoad => self.start_state_operation(save_states::Request::Load(
                vibe_emu_core::state_store::Slot::Recovery,
            )),
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
                    ctx.send_viewport_cmd_to(
                        egui::ViewportId::ROOT,
                        egui::ViewportCommand::InnerSize(egui::vec2(1100.0, 760.0)),
                    );
                    self.debugger_state.request_scroll_to_pc();
                } else {
                    self.apply_window_scale(ctx);
                }
            }
            Action::Quit => {
                let _ = self.emu_tx.send(EmuCommand::Shutdown);
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
            }
            Action::RunNoBreak => {
                self.debugger_state
                    .request_continue_no_break_and_focus_main();
                self.paused = false;
                let _ = self.emu_tx.send(EmuCommand::SetPaused(false));
            }
            Action::StepInto => self.do_single_step(),
            Action::StepOver => self.debugger_state.request_step_over(),
            Action::StepOut => self.debugger_state.request_step_out(),
            Action::RunToCursor => self.debugger_state.request_run_to_cursor(),
            Action::RunToCursorNoBreak => self.debugger_state.request_run_to_cursor_no_break(),
            Action::JumpToCursor => self.debugger_state.request_jump_to_cursor(),
            Action::CallCursor => self.debugger_state.request_call_cursor(),
            Action::JumpStack => self.debugger_state.request_jump_sp(),
            Action::ToggleBreakpoint => {
                if let Some(cursor) = self.debugger_state.cursor() {
                    self.debugger_state.toggle_breakpoint(cursor);
                }
            }
            Action::ReloadSymbols => self.debugger_state.reload_symbols(),
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
        let show_debug = self.ui_config.preferences.workspace == Workspace::Develop
            || self.show_debugger
            || !self.develop_layout.floating_panels().is_empty();
        main_menu_bar(ui, show_debug, |ui, menu| match menu {
            MainMenu::File => {
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
            }
            MainMenu::Emulation => {
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
                    if ui
                        .button(format!(
                            "Custom speed… ({}%)",
                            self.ui_config.preferences.speed_percent
                        ))
                        .clicked()
                    {
                        self.options_tab = OptionsTab::General;
                        self.show_options = true;
                        ui.close();
                    }
                });
                submenu(ui, "Hardware mode", |ui| {
                    self.draw_emulation_mode_submenu(ui);
                });
            }
            MainMenu::View => {
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
            }
            MainMenu::States => {
                self.action_button(ui, "Save states…", Action::SaveStates);
                self.action_button(ui, "Quick save", Action::QuickSave);
                self.action_button(ui, "Quick load", Action::QuickLoad);
                self.action_button(ui, "Undo load", Action::UndoLoad);
            }
            MainMenu::Tools => {
                self.action_button(ui, "Capture screenshot", Action::Screenshot);
                self.action_button(ui, "Mute / unmute", Action::ToggleMute);
                submenu(ui, "Peripherals", |ui| {
                    self.draw_serial_peripheral_submenu(ui);
                });
                ui.separator();
                ui.checkbox(&mut self.show_debugger, "Detached debugger");
                ui.checkbox(&mut self.show_watchpoints, "Watchpoints");
                ui.checkbox(&mut self.show_vram_viewer, "VRAM viewer");
            }
            MainMenu::Debug => {
                for &(action, label) in shortcuts::DEBUG_COMMANDS {
                    self.action_button(ui, label, action);
                }
            }
            MainMenu::Settings => {
                self.action_button(ui, "Settings…", Action::Settings);
            }
            MainMenu::Help => {
                if ui.button("Controls").clicked() {
                    self.options_tab = OptionsTab::Controls;
                    self.show_options = true;
                }
                if ui.button("About and licenses").clicked() {
                    self.show_about = true;
                }
            }
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
        self.action_button(ui, "Save states…", Action::SaveStates);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainMenu {
    File,
    Emulation,
    View,
    States,
    Tools,
    Debug,
    Settings,
    Help,
}

impl MainMenu {
    const ALL: [Self; 8] = [
        Self::File,
        Self::Emulation,
        Self::View,
        Self::States,
        Self::Tools,
        Self::Debug,
        Self::Settings,
        Self::Help,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Emulation => "Emulation",
            Self::View => "View",
            Self::States => "States",
            Self::Tools => "Tools",
            Self::Debug => "Debug",
            Self::Settings => "Settings",
            Self::Help => "Help",
        }
    }
}

fn main_menu_bar(
    ui: &mut egui::Ui,
    show_debug: bool,
    mut content: impl FnMut(&mut egui::Ui, MainMenu),
) {
    egui::MenuBar::new().ui(ui, |ui| {
        // Keep every top-level menu discoverable at the default 2x size and
        // enlarged text settings. Inherit the native menu ownership/config so
        // wrapped rows still support hover switching and keyboard navigation.
        ui.horizontal_wrapped(|ui| {
            // Compact gaps keep normal Play menus on one row at 320pt,
            // preserving room for the 2x game image. Larger text or Debug
            // can wrap without making any menu unreachable.
            ui.spacing_mut().item_spacing.x = 2.0;
            for menu in MainMenu::ALL {
                if menu != MainMenu::Debug || show_debug {
                    ui.menu_button(menu.title(), |ui| content(ui, menu));
                }
            }
        });
    });
}

fn menu_command_button<'a>(ui: &egui::Ui, label: &'a str, shortcut: &'a str) -> egui::Button<'a> {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let text_width = ui.fonts_mut(|fonts| {
        [label, shortcut]
            .into_iter()
            .map(|text| {
                fonts
                    .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            })
            .sum::<f32>()
    });
    // Reserve an explicit gap as well as both text columns. A one-item menu
    // otherwise sizes itself too narrowly for a long shortcut such as Ctrl+Comma.
    let width = text_width + 24.0 + 2.0 * ui.spacing().button_padding.x;
    egui::Button::new(label)
        .shortcut_text(shortcut)
        .min_size(egui::vec2(
            width.min(ui.ctx().content_rect().width() - 16.0),
            0.0,
        ))
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
        // Cap the native menu width without expanding its justified contents
        // to fill all remaining window space.
        ui.set_max_width(width.min(ui.max_rect().width()));
        // Allow wrapped rows to grow after a live resize; otherwise the area
        // can keep the previous frame's shorter height as its scroll viewport.
        ui.set_max_height(height.max(40.0));
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
    fn all_main_menus_remain_visible_and_open_after_resize() {
        for show_debug in [false, true] {
            for text_scale in [1.0, 1.5, 2.0] {
                let ctx = egui::Context::default();
                ctx.global_style_mut(|style| {
                    style.animation_time = 0.0;
                    for font in style.text_styles.values_mut() {
                        font.size *= text_scale;
                    }
                });
                let draw = |width, events| {
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 400.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            egui::Panel::top("menus").show_inside(ui, |ui| {
                                main_menu_bar(ui, show_debug, |ui, menu| {
                                    let _ = ui.button(format!("{} commands", menu.title()));
                                });
                            });
                        },
                    )
                };
                // Resize the same live context down and back up, with Debug
                // both present and absent, rather than testing fresh layouts.
                for width in [640.0, 480.0, 320.0, 360.0, 1100.0, 320.0] {
                    draw(
                        width,
                        vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Default::default(),
                        }],
                    );
                    draw(width, vec![]);
                    let output = draw(width, vec![]);
                    let mut triggers = Vec::new();
                    let mut first_row = None;
                    for menu in MainMenu::ALL {
                        if menu == MainMenu::Debug && !show_debug {
                            continue;
                        }
                        let text = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::epaint::Shape::Text(text)
                                    if text.galley.text() == menu.title() =>
                                {
                                    Some((text.visual_bounding_rect(), shape.clip_rect, text.pos.y))
                                }
                                _ => None,
                            })
                            .unwrap_or_else(|| panic!("{menu:?} missing at {width}/{text_scale}"));
                        assert!(
                            text.0.left() >= 0.0 && text.0.right() <= width,
                            "{menu:?} off screen: {text:?}"
                        );
                        assert!(text.1.contains_rect(text.0), "{menu:?} clipped: {text:?}");
                        if !show_debug && text_scale == 1.0 {
                            let top = *first_row.get_or_insert(text.2);
                            assert!(
                                (text.2 - top).abs() < 1.0,
                                "normal Play menu must preserve the 2x game area"
                            );
                        }
                        if matches!(menu, MainMenu::States | MainMenu::Help) {
                            triggers.push((menu, text.0.center()));
                        }
                    }
                    for (menu, pos) in triggers {
                        for pressed in [true, false] {
                            draw(
                                width,
                                vec![
                                    egui::Event::PointerMoved(pos),
                                    egui::Event::PointerButton {
                                        pos,
                                        button: egui::PointerButton::Primary,
                                        pressed,
                                        modifiers: Default::default(),
                                    },
                                ],
                            );
                        }
                        draw(width, vec![]);
                        let output = draw(width, vec![]);
                        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                            egui::epaint::Shape::Text(text) if text.galley.text() == format!("{} commands", menu.title())
                                && shape.clip_rect.contains_rect(text.visual_bounding_rect())
                                && text.visual_bounding_rect().right() <= width)), "{menu:?} popup inaccessible at {width}/{text_scale}");
                        draw(
                            width,
                            vec![egui::Event::Key {
                                key: egui::Key::Escape,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: Default::default(),
                            }],
                        );
                        draw(width, vec![]);
                    }
                }
            }
        }
    }

    #[test]
    fn settings_menu_keeps_label_and_shortcut_visible_and_separated() {
        for width in [320.0, 1100.0] {
            let ctx = egui::Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let draw = |events| {
                let mut trigger = egui::Rect::NOTHING;
                let output = ctx.run_ui(
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
                            ui.add_space(200.0);
                            trigger = ui
                                .menu_button("Settings", |ui| {
                                    ui.add(menu_command_button(ui, "Settings…", "Ctrl+Comma"));
                                })
                                .response
                                .rect;
                        });
                    },
                );
                (trigger, output)
            };
            draw(vec![]);
            let (trigger, _) = draw(vec![]);
            for pressed in [true, false] {
                draw(vec![
                    egui::Event::PointerMoved(trigger.center()),
                    egui::Event::PointerButton {
                        pos: trigger.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ]);
            }
            draw(vec![]);
            let (_, output) = draw(vec![]);
            let mut label = None;
            let mut shortcut = None;
            for clipped in output.shapes {
                if let egui::epaint::Shape::Text(text) = clipped.shape {
                    let bounds = text.visual_bounding_rect();
                    match text.galley.text() {
                        "Settings…" => label = Some((bounds, clipped.clip_rect)),
                        "Ctrl+Comma" => shortcut = Some((bounds, clipped.clip_rect)),
                        _ => {}
                    }
                }
            }
            let (label, label_clip) = label.expect("menu label rendered");
            let (shortcut, shortcut_clip) = shortcut.expect("shortcut rendered");
            assert!(
                label.right() + 10.0 <= shortcut.left(),
                "columns need a readable gap"
            );
            assert!(label_clip.contains_rect(label));
            assert!(shortcut_clip.contains_rect(shortcut));
            assert!(shortcut.right() <= width && label.left() >= 0.0);
        }
    }

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
            let viewport_width = std::cell::Cell::new(width);
            let draw = |events| {
                let mut rects = [egui::Rect::NOTHING; 6];
                let _ = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(viewport_width.get(), 400.0),
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
                                        rects[5] = ui.radio(false, "Link Cable (Network)").rect;
                                        rects[4] = ui.clip_rect();
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
            assert!(
                rects[2].width() < 250.0,
                "{width}: short submenu must remain compact: {rects:?}"
            );
            assert!(rects[4].contains_rect(rects[5]), "last row must be visible");
            if width == 1100.0 && menu_x == 154.0 {
                viewport_width.set(360.0);
                for _ in 0..3 {
                    draw(vec![]);
                }
                let resized = draw(vec![]);
                assert!(
                    resized[4].contains_rect(resized[5]),
                    "resizing must not clip wrapped rows: {resized:?}"
                );
            }
        }
    }
}
