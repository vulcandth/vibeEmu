use imgui::Ui;
use vibe_emu_core::watchpoints::{Watchpoint, WatchpointHit, WatchpointTrigger};

#[derive(Debug, Default, Clone)]
pub struct WatchpointsState {
    watchpoints: Vec<Watchpoint>,
    selected: Option<usize>,

    edit_enabled: bool,
    edit_range: String,
    edit_value_match: String,
    edit_value_match_enabled: bool,
    edit_on_read: bool,
    edit_on_write: bool,
    edit_on_execute: bool,
    edit_on_jump: bool,
    edit_message: String,

    next_id: u32,
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

        let value = hit
            .value
            .map(|v| format!(" = ${v:02X}"))
            .unwrap_or_default();

        let pc = hit
            .pc
            .map(|pc| format!(" (pc=${pc:04X})"))
            .unwrap_or_default();

        let msg = self
            .watchpoints
            .iter()
            .find(|wp| wp.id == hit.id)
            .and_then(|wp| wp.message.as_ref())
            .map(|m| format!(" \u{2013} {m}"))
            .unwrap_or_default();

        self.status_line = Some(format!(
            "Watchpoint hit: {label} @ ${:04X}{value}{pc}{msg}",
            hit.addr
        ));
    }

    pub fn ui(&mut self, ui: &Ui) {
        let display = ui.io().display_size;
        let flags = imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_COLLAPSE;

        ui.window("Watchpoints")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size(display, imgui::Condition::Always)
            .flags(flags)
            .build(|| {
                if let Some(line) = &self.status_line {
                    ui.text(line);
                }

                ui.separator();

                ui.columns(2, "watchpoints_cols", true);

                ui.text("List");
                ui.separator();

                let list_height = ui.content_region_avail()[1].max(100.0);
                ui.child_window("watchpoints_list")
                    .size([0.0, list_height])
                    .build(|| {
                        let mut pending_select: Option<usize> = None;
                        for idx in 0..self.watchpoints.len() {
                            let wp = &self.watchpoints[idx];
                            let enabled = if wp.enabled { "" } else { "(disabled) " };
                            let range =
                                format!("${:04X}-${:04X}", wp.range.start(), wp.range.end());
                            let mut triggers = String::new();
                            if wp.on_read {
                                triggers.push('R');
                            }
                            if wp.on_write {
                                triggers.push('W');
                            }
                            if wp.on_execute {
                                triggers.push('X');
                            }
                            if wp.on_jump {
                                triggers.push('J');
                            }
                            if triggers.is_empty() {
                                triggers.push('-');
                            }

                            let label = format!("#{:03} {enabled}[{triggers}] {range}", wp.id);
                            let selected = self.selected == Some(idx);
                            if ui.selectable_config(label).selected(selected).build() {
                                pending_select = Some(idx);
                            }
                        }

                        if let Some(idx) = pending_select {
                            self.selected = Some(idx);
                            self.load_editor_from_selected();
                        }
                    });

                ui.next_column();

                ui.text("Editor");
                ui.separator();

                ui.checkbox("Enabled", &mut self.edit_enabled);

                ui.text("Address or range (hex):");
                ui.set_next_item_width(-1.0);
                ui.input_text("##range", &mut self.edit_range).build();

                ui.checkbox("Value match", &mut self.edit_value_match_enabled);
                if self.edit_value_match_enabled {
                    ui.same_line();
                    ui.set_next_item_width(100.0);
                    ui.input_text("##value", &mut self.edit_value_match).build();
                }

                ui.separator();

                ui.text("Triggers:");
                ui.checkbox("On read", &mut self.edit_on_read);
                ui.checkbox("On write", &mut self.edit_on_write);
                ui.checkbox("On execute", &mut self.edit_on_execute);
                ui.checkbox("On jump", &mut self.edit_on_jump);

                ui.separator();

                ui.text("Debug message (optional):");
                ui.set_next_item_width(-1.0);
                ui.input_text_multiline("##msg", &mut self.edit_message, [0.0, 60.0])
                    .build();

                ui.separator();

                if ui.button("Add") {
                    self.add_from_editor();
                }
                ui.same_line();
                if ui.button("Replace") {
                    self.replace_selected_from_editor();
                }
                ui.same_line();
                if ui.button("Delete") {
                    self.delete_selected();
                }

                ui.separator();

                if ui.button("Disable all") {
                    for wp in &mut self.watchpoints {
                        wp.enabled = false;
                    }
                    self.pending_sync = true;
                }
                ui.same_line();
                if ui.button("Enable all") {
                    for wp in &mut self.watchpoints {
                        wp.enabled = true;
                    }
                    self.pending_sync = true;
                }

                ui.columns(1, "", false);
            });
    }

    fn load_editor_from_selected(&mut self) {
        let Some(idx) = self.selected else {
            return;
        };
        let Some(wp) = self.watchpoints.get(idx) else {
            return;
        };

        self.edit_enabled = wp.enabled;
        self.edit_range = if wp.range.start() == wp.range.end() {
            format!("${:04X}", wp.range.start())
        } else {
            format!("${:04X}-${:04X}", wp.range.start(), wp.range.end())
        };

        self.edit_value_match_enabled = wp.value_match.is_some();
        self.edit_value_match = wp
            .value_match
            .map(|v| format!("${v:02X}"))
            .unwrap_or_default();

        self.edit_on_read = wp.on_read;
        self.edit_on_write = wp.on_write;
        self.edit_on_execute = wp.on_execute;
        self.edit_on_jump = wp.on_jump;
        self.edit_message = wp.message.clone().unwrap_or_default();
    }

    fn add_from_editor(&mut self) {
        let parsed = self.parse_editor();
        let Some(mut wp) = parsed else {
            return;
        };

        wp.id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        self.watchpoints.push(wp);
        self.selected = Some(self.watchpoints.len().saturating_sub(1));
        self.pending_sync = true;
    }

    fn replace_selected_from_editor(&mut self) {
        let Some(idx) = self.selected else {
            self.status_line = Some("No watchpoint selected".to_string());
            return;
        };

        let Some(mut wp) = self.parse_editor() else {
            return;
        };

        let Some(existing) = self.watchpoints.get(idx) else {
            self.status_line = Some("Selected watchpoint out of range".to_string());
            return;
        };

        wp.id = existing.id;
        self.watchpoints[idx] = wp;
        self.pending_sync = true;
    }

    fn delete_selected(&mut self) {
        let Some(idx) = self.selected else {
            self.status_line = Some("No watchpoint selected".to_string());
            return;
        };

        if idx >= self.watchpoints.len() {
            self.status_line = Some("Selected watchpoint out of range".to_string());
            return;
        }

        self.watchpoints.remove(idx);
        if self.watchpoints.is_empty() {
            self.selected = None;
        } else {
            self.selected = Some(idx.min(self.watchpoints.len() - 1));
            self.load_editor_from_selected();
        }
        self.pending_sync = true;
    }

    fn parse_editor(&mut self) -> Option<Watchpoint> {
        let (start, end) = match parse_range(&self.edit_range) {
            Ok(v) => v,
            Err(e) => {
                self.status_line = Some(e);
                return None;
            }
        };

        let value_match = if self.edit_value_match_enabled {
            match parse_u8_hex(&self.edit_value_match) {
                Ok(v) => Some(v),
                Err(e) => {
                    self.status_line = Some(e);
                    return None;
                }
            }
        } else {
            None
        };

        if !(self.edit_on_read || self.edit_on_write || self.edit_on_execute || self.edit_on_jump) {
            self.status_line = Some("No triggers selected".to_string());
            return None;
        }

        let message = if self.edit_message.trim().is_empty() {
            None
        } else {
            Some(self.edit_message.trim().to_string())
        };

        Some(Watchpoint {
            id: 0,
            enabled: self.edit_enabled,
            range: start..=end,
            on_read: self.edit_on_read,
            on_write: self.edit_on_write,
            on_execute: self.edit_on_execute,
            on_jump: self.edit_on_jump,
            value_match,
            message,
        })
    }
}

fn parse_u16_hex(s: &str) -> Result<u16, String> {
    let s = s.trim();
    let s = s.strip_prefix('$').unwrap_or(s);
    let s = s.strip_prefix("0x").unwrap_or(s);
    u16::from_str_radix(s, 16).map_err(|_| format!("Invalid hex u16: '{s}'"))
}

fn parse_u8_hex(s: &str) -> Result<u8, String> {
    let s = s.trim();
    let s = s.strip_prefix('$').unwrap_or(s);
    let s = s.strip_prefix("0x").unwrap_or(s);
    u8::from_str_radix(s, 16).map_err(|_| format!("Invalid hex u8: '{s}'"))
}

fn parse_range(s: &str) -> Result<(u16, u16), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Range is empty".to_string());
    }

    if let Some((a, b)) = s.split_once('-') {
        let start = parse_u16_hex(a)?;
        let end = parse_u16_hex(b)?;
        if start > end {
            return Err("Range start must be <= end".to_string());
        }
        Ok((start, end))
    } else {
        let addr = parse_u16_hex(s)?;
        Ok((addr, addr))
    }
}
