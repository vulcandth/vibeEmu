use super::*;

impl VibeEmuApp {
    pub(super) fn apply_appearance(&self, ctx: &egui::Context) {
        use vibe_emu_frontend::Theme;
        ctx.set_zoom_factor(self.ui_config.preferences.ui_scale);
        ctx.set_theme(match self.ui_config.preferences.theme {
            Theme::System => egui::ThemePreference::System,
            Theme::Light => egui::ThemePreference::Light,
            Theme::Dark => egui::ThemePreference::Dark,
        });
    }

    pub(super) fn draw_options_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Search settings");
            ui.text_edit_singleline(&mut self.settings_search);
            if ui.button("Clear").clicked() {
                self.settings_search.clear();
            }
        });
        ui.separator();
        if !self.options_tab.matches(&self.settings_search)
            && let Some(category) = OptionsTab::ALL
                .into_iter()
                .find(|category| category.matches(&self.settings_search))
        {
            self.options_tab = category;
        }
        if !OptionsTab::ALL
            .into_iter()
            .any(|category| category.matches(&self.settings_search))
        {
            ui.label("No matching settings.");
            return;
        }
        let previous = self.ui_config.preferences.clone();
        if ui.available_width() < 600.0 {
            egui::ComboBox::from_id_salt("settings_category")
                .selected_text(self.options_tab.label())
                .show_ui(ui, |ui| {
                    for category in OptionsTab::ALL {
                        if category.matches(&self.settings_search) {
                            ui.selectable_value(&mut self.options_tab, category, category.label());
                        }
                    }
                });
            self.draw_settings_body(ui, ctx);
        } else {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(155.0);
                    for category in OptionsTab::ALL {
                        if category.matches(&self.settings_search) {
                            ui.selectable_value(&mut self.options_tab, category, category.label());
                        }
                    }
                });
                ui.separator();
                self.draw_settings_body(ui, ctx);
            });
        }
        if previous != self.ui_config.preferences {
            self.ui_config.preferences.normalize();
            self.audio_controls.set(
                self.ui_config.preferences.volume,
                self.ui_config.preferences.mono,
            );
            self.save_ui_config();
        }
    }
    fn draw_settings_body(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::ScrollArea::both()
            .id_salt("settings_content")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // The wide view lives in a horizontal sidebar layout; establish
                // a vertical layout explicitly for the scrollable category body.
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width());
                    ui.heading(self.options_tab.label());
                    ui.add_space(8.0);
                    self.draw_settings_category(ui, ctx);
                    ui.separator();
                    if matches!(
                        self.options_tab,
                        OptionsTab::General
                            | OptionsTab::Audio
                            | OptionsTab::Video
                            | OptionsTab::Controls
                    ) && ui.button("Restore category defaults").clicked()
                    {
                        self.restore_category_defaults();
                    }
                });
            });
    }

    fn draw_settings_category(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match self.options_tab {
            OptionsTab::Controls => self.draw_control_settings(ui, ctx),
            OptionsTab::System => self.draw_system_settings(ui),
            OptionsTab::Video => self.draw_video_settings(ui, ctx),
            OptionsTab::General => {
                use vibe_emu_frontend::Theme;
                egui::ComboBox::from_label("Appearance")
                    .selected_text(format!("{:?}", self.ui_config.preferences.theme))
                    .show_ui(ui, |ui| {
                        for theme in [Theme::System, Theme::Light, Theme::Dark] {
                            ui.selectable_value(
                                &mut self.ui_config.preferences.theme,
                                theme,
                                format!("{theme:?}"),
                            );
                        }
                    });
                ui.add(
                    egui::Slider::new(&mut self.ui_config.preferences.ui_scale, 0.75..=2.5)
                        .text("UI scale"),
                );
                ui.checkbox(
                    &mut self.ui_config.preferences.pause_on_focus_loss,
                    "Pause when the app loses focus",
                );
                ui.checkbox(
                    &mut self.ui_config.preferences.background_controllers,
                    "Allow controllers while in background",
                );
                ui.checkbox(
                    &mut self.ui_config.preferences.show_status,
                    "Show status bar",
                );
                ui.checkbox(
                    &mut self.ui_config.preferences.double_click_fullscreen,
                    "Double-click game to toggle fullscreen",
                );
                ui.label("Changes apply immediately. Workspace selection is remembered.");
            }
            OptionsTab::Audio => {
                let mut enabled = self.sound_enabled.load(Ordering::Relaxed);
                if ui.checkbox(&mut enabled, "Enable sound").changed() {
                    self.sound_enabled.store(enabled, Ordering::Relaxed);
                    self.persist_runtime_settings();
                }
                ui.add(
                    egui::Slider::new(&mut self.ui_config.preferences.volume, 0..=100)
                        .text("Volume %"),
                );
                ui.checkbox(&mut self.ui_config.preferences.mono, "Mono output");
                ui.label("Uses the system output device and sample rate. Output controls do not change the emulated sound hardware.");
            }
            OptionsTab::Capture => {
                ui.label("PNG screenshots");
                ui.label(format!(
                    "Destination: {}",
                    self.screenshot_output_dir().display()
                ));
                if ui.button("Choose screenshot folder…").clicked()
                    && let Some(path) = FileDialog::new().pick_folder()
                {
                    self.ui_config.screenshot_directory = Some(path);
                    self.save_ui_config();
                }
                if ui.button("Use default folder").clicked() {
                    self.ui_config.screenshot_directory = None;
                    self.save_ui_config();
                }
                self.action_button(ui, "Capture screenshot", Action::Screenshot);
            }
            OptionsTab::Peripherals => {
                self.draw_serial_peripheral_submenu(ui);
            }
            OptionsTab::Files => {
                ui.label("Open ROMs directly or drag them into the game window.");
                ui.label(format!("Configuration: {}", self.ui_config_path.display()));
                ui.label(format!(
                    "Keyboard bindings: {}",
                    self.keybinds_path.display()
                ));
                ui.label("Recent ROMs can be pinned or cleared from File → Recent ROMs.");
            }
            OptionsTab::Developer => {
                self.action_button(ui, "Open Develop workspace", Action::Develop);
                ui.checkbox(&mut self.show_debugger, "Detached debugger");
                ui.checkbox(&mut self.show_watchpoints, "Watchpoints");
                ui.checkbox(&mut self.show_vram_viewer, "VRAM viewer");
                if ui.button("Reload symbols").clicked() {
                    self.debugger_state.reload_symbols();
                }
                ui.label("Debugger tools are available in release builds. Hardware behavior remains accurate.");
            }
        }
    }

    fn restore_category_defaults(&mut self) {
        let defaults = vibe_emu_frontend::Preferences::default();
        match self.options_tab {
            OptionsTab::General => {
                let p = &mut self.ui_config.preferences;
                p.theme = defaults.theme;
                p.ui_scale = defaults.ui_scale;
                p.pause_on_focus_loss = defaults.pause_on_focus_loss;
                p.background_controllers = defaults.background_controllers;
                p.show_status = defaults.show_status;
                p.double_click_fullscreen = defaults.double_click_fullscreen;
            }
            OptionsTab::Audio => {
                self.ui_config.preferences.volume = 100;
                self.ui_config.preferences.mono = false;
                self.sound_enabled.store(true, Ordering::Relaxed);
            }
            OptionsTab::Video => {
                self.apply_video_filter_config(VideoFilterConfig::default());
                self.ui_config.show_sgb_border = true;
                self.ui_config.dmg_palette = None;
                if let Ok(mut gb) = self.gb.lock() {
                    gb.mmu.ppu.set_dmg_palette(if self.dmg_neutral {
                        [0xffffff, 0xaaaaaa, 0x555555, 0]
                    } else {
                        [0x9bbc0f, 0x8bac0f, 0x306230, 0x0f380f]
                    });
                }
            }
            OptionsTab::Controls => {
                self.keybinds = KeyBindings::defaults();
                self.ui_config.preferences.fast_forward_percent = defaults.fast_forward_percent;
                if let Err(e) = self.keybinds.save_to_file(&self.keybinds_path) {
                    self.load_error = Some(e.to_string());
                }
            }
            _ => {}
        }
        self.persist_runtime_settings();
    }

    fn begin_rebind(&mut self, target: RebindTarget) {
        self.rebind_shift_presses = self
            .shift
            .lock()
            .map(|state| state.presses)
            .unwrap_or_default();
        self.rebinding = Some(target);
        self.release_gameplay_input();
    }

    pub(super) fn finish_rebind(&mut self, key: keybinds::BindingKey) {
        if let Some(target) = self.rebinding.take() {
            self.keybinds.rebind(target, key);
            if let Err(error) = self.keybinds.save_to_file(&self.keybinds_path) {
                self.load_error = Some(format!("Could not save key binding: {error}"));
            }
        }
    }

    pub(super) fn draw_control_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Use this viewport's focus: settings may be a detached native window.
        if self.rebinding.is_some() && ctx.input(|i| i.focused) {
            let shift = self.shift.lock().map(|state| *state).unwrap_or_default();
            if let Some(side) = shift
                .newly_pressed(self.rebind_shift_presses)
                .iter()
                .position(|pressed| *pressed)
            {
                self.finish_rebind(if side == 0 {
                    keybinds::BindingKey::ShiftLeft
                } else {
                    keybinds::BindingKey::ShiftRight
                });
            }
        }
        ui.label("Keyboard controls player 1. In SGB multiplayer, gamepads occupy players 1–4 in connection order.");
        #[cfg(not(target_os = "android"))]
        if let Some(gamepad) = &self.gamepad {
            for (player, id) in gamepad.players.iter().enumerate() {
                ui.label(format!(
                    "Player {}: {}",
                    player + 1,
                    id.map(|id| gamepad.gilrs.gamepad(id).name().to_owned())
                        .unwrap_or_else(|| "no gamepad".into())
                ));
            }
        }
        if self.rebinding.is_some() {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::YELLOW, "Waiting for key...");
                if ui.button("Cancel").clicked() {
                    self.rebinding = None;
                }
            });
            ui.separator();

            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        ..
                    } = event
                        && self.rebinding.is_some()
                    {
                        self.finish_rebind((*key).into());
                        break;
                    }
                }
            });
        }

        ui.label(
            "Click Rebind, then press a key. Left Shift and Right Shift are separate bindings.",
        );
        ui.label("Escape opens the game menu unless an explicit Quit binding uses it. Restore category defaults to use the new menu binding.");
        ui.add(
            egui::Slider::new(
                &mut self.ui_config.preferences.fast_forward_percent,
                100..=1000,
            )
            .text("Fast-forward cap %"),
        );
        ui.add_space(4.0);

        egui::Grid::new("keybinds_grid")
            .num_columns(3)
            .spacing([20.0, 4.0])
            .show(ui, |ui| {
                let fmt_joy = |keybinds: &KeyBindings, mask: u8| -> String {
                    keybinds
                        .key_for_joypad_mask(mask)
                        .map(|k| format!("{k:?}"))
                        .unwrap_or_else(|| "<unbound>".to_string())
                };

                for (label, mask) in [
                    ("Up", 0x04u8),
                    ("Down", 0x08),
                    ("Left", 0x02),
                    ("Right", 0x01),
                ] {
                    ui.label(label);
                    ui.label(fmt_joy(&self.keybinds, mask));
                    if ui.button("Rebind").clicked() {
                        self.begin_rebind(RebindTarget::Joypad(mask));
                    }
                    ui.end_row();
                }

                ui.separator();
                ui.end_row();

                for (label, mask) in [
                    ("A", 0x10u8),
                    ("B", 0x20),
                    ("Select", 0x40),
                    ("Start", 0x80),
                ] {
                    ui.label(label);
                    ui.label(fmt_joy(&self.keybinds, mask));
                    if ui.button("Rebind").clicked() {
                        self.begin_rebind(RebindTarget::Joypad(mask));
                    }
                    ui.end_row();
                }

                ui.separator();
                ui.end_row();

                ui.label("Pause / resume");
                ui.label(format!("{:?}", self.keybinds.pause_key()));
                if ui.button("Rebind").clicked() {
                    self.begin_rebind(RebindTarget::Pause);
                }
                ui.end_row();

                ui.label("Quit");
                ui.label(if self.keybinds.quit_is_bound() {
                    format!("{:?}", self.keybinds.quit_key())
                } else {
                    "<unbound>".to_owned()
                });
                if ui.button("Rebind").clicked() {
                    self.begin_rebind(RebindTarget::Quit);
                }
                ui.end_row();

                ui.label("Fast Forward");
                ui.label(format!("{:?}", self.keybinds.fast_forward_key()));
                if ui.button("Rebind").clicked() {
                    self.begin_rebind(RebindTarget::FastForward);
                }
                ui.end_row();

                ui.label("Screenshot");
                ui.label(format!("{:?}", self.keybinds.screenshot_key()));
                if ui.button("Rebind").clicked() {
                    self.begin_rebind(RebindTarget::Screenshot);
                }
                ui.end_row();
            });
    }
    pub(super) fn draw_system_settings(&mut self, ui: &mut egui::Ui) {
        let before = (self.ui_config.dmg_revision, self.ui_config.cgb_revision);
        for (label, revision, labels) in [
            (
                "DMG revision",
                &mut self.ui_config.dmg_revision,
                &["0", "A", "B", "C"][..],
            ),
            (
                "CGB revision",
                &mut self.ui_config.cgb_revision,
                &["0", "A", "B", "C", "D", "E"][..],
            ),
        ] {
            egui::ComboBox::from_label(label)
                .selected_text(labels[usize::from(*revision).min(labels.len() - 1)])
                .show_ui(ui, |ui| {
                    for (i, name) in labels.iter().enumerate() {
                        ui.selectable_value(revision, i as u8, *name);
                    }
                });
        }
        if before != (self.ui_config.dmg_revision, self.ui_config.cgb_revision) {
            self.save_ui_config();
        }
        if self.load_config() != self.active_load_config {
            ui.label("Pending machine changes — apply on the next load.");
            if ui.button("Discard pending changes").clicked() {
                let active = self.active_load_config.clone();
                self.emulation_mode = active.emulation_mode;
                self.dmg_neutral = active.dmg_neutral;
                self.bootrom_override = active.bootrom_override;
                self.sgb_bootrom_override = active.sgb_bootrom_override;
                self.ui_config.dmg_revision = active.dmg_revision;
                self.ui_config.cgb_revision = active.cgb_revision;
                self.bootrom_paths = active.bootrom_paths.each_ref().map(|p| {
                    p.as_ref()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
                self.persist_bootrom_paths();
                self.persist_runtime_settings();
            }
        }
        ui.menu_button("Emulation mode", |ui| {
            self.draw_emulation_mode_submenu(ui);
        });
        ui.label("Boot ROMs (blank = skip boot). Changes apply on the next ROM load.");
        ui.label("Initial-border mode uses SGB for capture and CGB for gameplay.");
        if self.bootrom_override.is_some() || self.sgb_bootrom_override.is_some() {
            ui.label("Command-line boot ROM overrides are active.");
            if ui.button("Use saved boot ROM settings instead").clicked() {
                self.bootrom_override = None;
                self.sgb_bootrom_override = None;
            }
        }
        for (index, (label, model)) in ui_config::BOOT_MODELS.into_iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{label}:"));
                let mut changed = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.bootrom_paths[index])
                            .desired_width(230.0),
                    )
                    .changed();
                if ui.button("Browse...").clicked()
                    && let Some(path) = FileDialog::new().pick_file()
                {
                    self.bootrom_paths[index] = path.to_string_lossy().into_owned();
                    changed = true;
                }
                if ui.button("Clear").clicked() {
                    self.bootrom_paths[index].clear();
                    changed = true;
                }
                if changed {
                    self.persist_bootrom_paths();
                }
            });
            if let Some(path) = Self::optional_path_from_input(&self.bootrom_paths[index]) {
                let validation = std::fs::metadata(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|meta| {
                        let size = meta.len();
                        if (model.is_cgb() && matches!(size, 0x800 | 0x900))
                            || (!model.is_cgb() && size == 0x100)
                        {
                            Ok(())
                        } else {
                            Err(format!("Unexpected boot ROM size: {size} bytes"))
                        }
                    });
                if let Err(e) = validation {
                    ui.colored_label(egui::Color32::RED, e);
                }
            }
        }
        if ui
            .add_enabled(
                self.current_rom_path.is_some() && self.loading.is_none(),
                egui::Button::new("Apply and reload current ROM"),
            )
            .clicked()
        {
            self.pending_rom_load = self.current_rom_path.clone();
        }
        ui.label("SGB + GBC keeps both command sets active. Initial border runs normal CGB after capture (no SGB multiplayer or later border changes).");
        ui.label("SGB audio commands and uploaded SNES programs are not emulated.");
        if let Some(error) = &self.load_error {
            ui.colored_label(egui::Color32::RED, error);
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
    }
    pub(super) fn draw_video_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label("Monochrome palette");
        let mut palette = self
            .gb
            .lock()
            .map(|gb| gb.mmu.ppu.dmg_palette())
            .unwrap_or([0xffffff, 0xaaaaaa, 0x555555, 0]);
        let original = palette;
        ui.horizontal(|ui| {
            if ui.button("Greyscale").clicked() {
                palette = [0xffffff, 0xaaaaaa, 0x555555, 0];
            }
            if ui.button("LCD green").clicked() {
                palette = [0xe0f8d0, 0x88c070, 0x346856, 0x081820];
            }
            if ui.button("Interpolate").clicked() {
                for i in 1..3 {
                    palette[i] = [16, 8, 0]
                        .into_iter()
                        .map(|shift| {
                            let a = (palette[0] >> shift) & 255;
                            let b = (palette[3] >> shift) & 255;
                            ((a * (3 - i as u32) + b * i as u32) / 3) << shift
                        })
                        .sum();
                }
            }
        });
        ui.horizontal(|ui| {
            for color in &mut palette {
                let mut rgb = [(*color >> 16) as u8, (*color >> 8) as u8, *color as u8];
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    *color =
                        (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]);
                }
            }
        });
        if original != palette {
            self.ui_config.dmg_palette = Some(palette);
            if let Ok(mut gb) = self.gb.lock() {
                gb.mmu.ppu.set_dmg_palette(palette);
            }
            self.save_ui_config();
        }
        ui.small("Applies to monochrome rendering; game-supplied CGB/SGB colors are preserved.");
        ui.separator();
        if ui
            .checkbox(&mut self.ui_config.show_sgb_border, "Show SGB border")
            .changed()
        {
            self.save_ui_config();
            self.apply_window_scale(ctx);
        }
        ui.label("Display filter");

        let prev_filter_config = self.current_video_filter_config();
        let mut selected_preset = Self::video_filter_preset_for_config(prev_filter_config);

        egui::ComboBox::from_label("Preset")
            .selected_text(
                selected_preset
                    .map(Self::video_filter_preset_label)
                    .unwrap_or("Custom"),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut selected_preset,
                    Some(VideoFilterPreset::CurrentMethod),
                    Self::video_filter_preset_label(VideoFilterPreset::CurrentMethod),
                );
                ui.selectable_value(
                    &mut selected_preset,
                    Some(VideoFilterPreset::HorizontalBlur),
                    Self::video_filter_preset_label(VideoFilterPreset::HorizontalBlur),
                );
                ui.selectable_value(
                    &mut selected_preset,
                    Some(VideoFilterPreset::Bilinear),
                    Self::video_filter_preset_label(VideoFilterPreset::Bilinear),
                );
                ui.selectable_value(
                    &mut selected_preset,
                    Some(VideoFilterPreset::Scanlines),
                    Self::video_filter_preset_label(VideoFilterPreset::Scanlines),
                );
                ui.selectable_value(
                    &mut selected_preset,
                    Some(VideoFilterPreset::LcdGrid),
                    Self::video_filter_preset_label(VideoFilterPreset::LcdGrid),
                );
            });

        ui.small("\"30fps.net\" is the article source name, not the emulation frame rate.");

        if let Some(preset) = selected_preset {
            self.apply_video_filter_config(Self::video_filter_preset_config(preset));
        }

        egui::ComboBox::from_label("Horizontal sampling")
            .selected_text(Self::axis_filter_label(self.display_horizontal_filter))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.display_horizontal_filter,
                    AxisFilter::Nearest,
                    Self::axis_filter_label(AxisFilter::Nearest),
                );
                ui.selectable_value(
                    &mut self.display_horizontal_filter,
                    AxisFilter::Linear,
                    Self::axis_filter_label(AxisFilter::Linear),
                );
            });

        egui::ComboBox::from_label("Vertical sampling")
            .selected_text(Self::axis_filter_label(self.display_vertical_filter))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.display_vertical_filter,
                    AxisFilter::Nearest,
                    Self::axis_filter_label(AxisFilter::Nearest),
                );
                ui.selectable_value(
                    &mut self.display_vertical_filter,
                    AxisFilter::Linear,
                    Self::axis_filter_label(AxisFilter::Linear),
                );
            });

        egui::ComboBox::from_label("Screen effect")
            .selected_text(Self::display_effect_label(self.display_effect))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.display_effect,
                    DisplayEffect::None,
                    Self::display_effect_label(DisplayEffect::None),
                );
                ui.selectable_value(
                    &mut self.display_effect,
                    DisplayEffect::Scanlines,
                    Self::display_effect_label(DisplayEffect::Scanlines),
                );
                ui.selectable_value(
                    &mut self.display_effect,
                    DisplayEffect::LcdGrid,
                    Self::display_effect_label(DisplayEffect::LcdGrid),
                );
            });

        if self.current_video_filter_config() != prev_filter_config {
            self.persist_runtime_settings();
        }
    }
}
