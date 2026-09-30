use super::*;

impl VibeEmuApp {
    pub(super) fn draw_memory_viewer(&mut self, ui: &mut egui::Ui, snapshot: &UiSnapshot) {
        let Some(mem) = snapshot.debugger.mem_image.as_ref() else {
            ui.label("Memory not available (run paused to capture)");
            return;
        };
        self.debugger_state.prepare_code_colors(snapshot);
        if !self.paused {
            self.mem_edit = None;
        }

        // Top bar with go-to address
        ui.horizontal(|ui| {
            ui.label("Go:");
            let goto_resp = ui.add(
                egui::TextEdit::singleline(&mut self.mem_viewer_goto)
                    .desired_width(80.0)
                    .font(egui::TextStyle::Monospace),
            );
            let submitted = goto_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Go").clicked() || submitted {
                if let Some(addr) =
                    self.parse_mem_viewer_address(&self.mem_viewer_goto.clone(), snapshot)
                {
                    self.mem_viewer_addr = addr & 0xFFF0; // Align to 16-byte row
                    self.mem_viewer_cursor = addr;
                    self.mem_viewer_scroll_to = Some((addr as usize) / 16);
                }
                self.mem_viewer_goto.clear();
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.monospace(format!(
                "Selected ${:04X}: ${:02X}",
                self.mem_viewer_cursor, mem[self.mem_viewer_cursor as usize]
            ));
            if ui
                .add_enabled(self.paused, egui::Button::new("Edit byte…"))
                .clicked()
            {
                self.mem_edit = Some((
                    self.mem_viewer_cursor,
                    format!("{:02X}", mem[self.mem_viewer_cursor as usize]),
                ));
            }
        });
        if let Some((addr, value)) = &mut self.mem_edit {
            let mut apply = None;
            let mut cancel = false;
            ui.horizontal_wrapped(|ui| {
                ui.monospace(format!("${addr:04X} ="));
                let edit = ui.add(egui::TextEdit::singleline(value).desired_width(45.0).char_limit(2).font(egui::TextStyle::Monospace));
                let parsed = u8::from_str_radix(value.trim(), 16).ok();
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.add_enabled(parsed.is_some(), egui::Button::new("Apply")).clicked() || enter)
                    && let Some(byte) = parsed {
                    apply = Some((*addr, byte));
                }
                cancel = ui.button("Cancel").clicked();
                ui.label("Hex byte (00–FF). ROM edits are in memory only. I/O and cartridge RAM follow hardware write rules.");
            });
            if let Some((addr, byte)) = apply {
                if let Ok(mut gb) = self.gb.lock() {
                    match gb.mmu.debug_write_byte(addr, byte) {
                        Ok(()) => {
                            self.debugger_snapshot = Some(UiSnapshot::from_gb(&mut gb, true));
                            self.mem_edit = None;
                        }
                        Err(error) => self.load_error = Some(error.to_owned()),
                    }
                }
                ui.ctx().request_repaint();
            } else if cancel {
                self.mem_edit = None;
            }
        }
        ui.separator();

        // Render all rows - egui's ScrollArea handles virtualization
        // show_rows expects row_height_sans_spacing - it adds item_spacing.y internally
        let text_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let spacing = ui.spacing();
        let row_height_sans_spacing = text_height.max(spacing.interact_size.y);
        let row_height_with_spacing = row_height_sans_spacing + spacing.item_spacing.y;
        let bytes_per_row = 16usize;
        let total_rows = 0x10000usize.div_ceil(bytes_per_row);

        let scroll_to_row = self.mem_viewer_scroll_to.take();

        let mut scroll_area = egui::ScrollArea::both()
            .id_salt("mem_viewer_scroll")
            .max_height((ui.available_height() - 36.0).max(0.0))
            .auto_shrink([false, false]);

        // Set scroll offset using the same row height that show_rows uses internally
        if let Some(target_row) = scroll_to_row {
            let target_offset = target_row as f32 * row_height_with_spacing;
            scroll_area = scroll_area.vertical_scroll_offset(target_offset);
        }

        scroll_area.show_rows(ui, row_height_sans_spacing, total_rows, |ui, row_range| {
            for row_idx in row_range {
                let row_addr = (row_idx * bytes_per_row) as u16;
                let region = self.mem_region_prefix(row_addr, snapshot);

                ui.horizontal(|ui| {
                    ui.monospace(format!("{}:{:04X}", region, row_addr));
                    ui.add_space(8.0);

                    for col in 0..bytes_per_row {
                        let addr = row_addr.wrapping_add(col as u16);
                        let byte = mem[addr as usize];

                        let is_cursor = addr == self.mem_viewer_cursor;
                        let text = format!("{:02X}", byte);

                        let label = if is_cursor {
                            egui::RichText::new(text)
                                .monospace()
                                .background_color(egui::Color32::from_rgb(0, 80, 160))
                        } else {
                            egui::RichText::new(text).monospace()
                        };
                        let role = self.debugger_state.memory_byte_role(snapshot, addr);
                        let label = label.color(self.ui_config.debugger_colors.color(ui, role));

                        let response = ui.add(egui::Label::new(label).sense(egui::Sense::click()));
                        if response.clicked() {
                            self.mem_viewer_cursor = addr;
                        }
                        if response.double_clicked() && self.paused {
                            self.mem_edit = Some((addr, format!("{byte:02X}")));
                        }

                        if col == 7 {
                            ui.add_space(4.0);
                        }
                    }

                    ui.add_space(8.0);

                    let mut ascii = String::with_capacity(bytes_per_row);
                    for col in 0..bytes_per_row {
                        let addr = row_addr.wrapping_add(col as u16);
                        let byte = mem[addr as usize];
                        let c = if (0x20..=0x7E).contains(&byte) {
                            byte as char
                        } else {
                            '.'
                        };
                        ascii.push(c);
                    }
                    ui.monospace(ascii);
                });
            }
        });

        ui.separator();

        // Status bar showing label at cursor
        let cursor_addr = self.mem_viewer_cursor;
        let cursor_bank = self.bank_for_address(cursor_addr, snapshot);

        let label_info = if let Some(sym) = self.debugger_state.symbols() {
            if let Some((label, offset)) = sym.nearest_label_for(cursor_bank, cursor_addr) {
                if offset == 0 {
                    label.to_string()
                } else {
                    format!("{}+${:X}", label, offset)
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        ui.horizontal(|ui| {
            ui.monospace(format!(
                "{:04X}  {:02X}:{:04X}",
                cursor_addr, cursor_bank, cursor_addr
            ));
            if !label_info.is_empty() {
                ui.monospace(format!("  {}", label_info));
            }
        });
    }

    fn parse_mem_viewer_address(&mut self, input: &str, snapshot: &UiSnapshot) -> Option<u16> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Try parsing as bank:address format (e.g., 00:c000 or 05:4200)
        if let Some((bank_str, addr_str)) = trimmed.split_once(':')
            && let (Ok(bank), Ok(addr)) = (
                u8::from_str_radix(bank_str.trim_start_matches('$'), 16),
                u16::from_str_radix(addr_str.trim_start_matches('$'), 16),
            )
        {
            // The bytes shown belong to the CPU's active mapping.
            if bank != self.bank_for_address(addr, snapshot) {
                self.load_error = Some("The memory viewer shows the currently mapped bank. Select an address in that bank.".into());
                return None;
            }
            return Some(addr);
        }

        // Try parsing as hex number
        if let Some(hex) = trimmed.strip_prefix("$").or(Some(trimmed))
            && let Ok(addr) = u16::from_str_radix(hex, 16)
        {
            return Some(addr);
        }

        // Try symbol lookup
        if let Some(sym) = self.debugger_state.symbols()
            && let Some((_, addr)) = sym.lookup_name(trimmed)
        {
            return Some(addr);
        }

        None
    }

    fn mem_region_prefix(&self, addr: u16, snapshot: &UiSnapshot) -> String {
        match addr {
            0x0000..=0x3FFF => "RO00".to_string(),
            0x4000..=0x7FFF => {
                let bank = snapshot.debugger.active_rom_bank.min(0xFF) as u8;
                format!("RO{:02X}", bank)
            }
            0x8000..=0x9FFF => format!("VR{:02X}", snapshot.debugger.vram_bank),
            0xA000..=0xBFFF => format!("SR{:02X}", snapshot.debugger.sram_bank),
            0xC000..=0xCFFF => "WR00".to_string(),
            0xD000..=0xDFFF => {
                let bank = snapshot.debugger.wram_bank.max(1);
                format!("WR{:02X}", bank)
            }
            0xE000..=0xFDFF => "ECHO".to_string(),
            0xFE00..=0xFE9F => "OAM ".to_string(),
            0xFEA0..=0xFEFF => "----".to_string(),
            0xFF00..=0xFF7F => "I/O ".to_string(),
            0xFF80..=0xFFFE => "HRAM".to_string(),
            0xFFFF => "IE  ".to_string(),
        }
    }

    fn bank_for_address(&self, addr: u16, snapshot: &UiSnapshot) -> u8 {
        match addr {
            0x0000..=0x3FFF => 0,
            0x4000..=0x7FFF => snapshot.debugger.active_rom_bank.min(0xFF) as u8,
            0x8000..=0x9FFF => snapshot.debugger.vram_bank,
            0xA000..=0xBFFF => snapshot.debugger.sram_bank,
            0xC000..=0xCFFF => 0,
            0xD000..=0xDFFF => snapshot.debugger.wram_bank,
            _ => 0,
        }
    }
}
