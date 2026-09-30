use super::*;
use ui::highlight::{Role, byte_role};

impl VibeEmuApp {
    pub(super) fn draw_disassembly_pane(&mut self, ui: &mut egui::Ui, snapshot: &UiSnapshot) {
        let Some(mem) = &snapshot.debugger.mem_image else {
            ui.label("Memory not available (emulator running)");
            return;
        };
        let pc = snapshot.cpu.pc;
        let bank = snapshot.debugger.active_rom_bank.min(0xff) as u8;
        // Each viewport follows independently; one panel must not consume another
        // panel's follow request. Manual Go still takes precedence in this frame.
        let follow_id = ui.id().with("last-disassembly-pc");
        let changed = ui.data_mut(|data| {
            let previous = data.get_temp::<(u16, u8)>(follow_id);
            data.insert_temp(follow_id, (pc, bank));
            previous != Some((pc, bank))
        });
        let target = self
            .debugger_state
            .take_pending_scroll(pc)
            .or_else(|| changed.then_some(pc));
        let bank_for = |addr| {
            if addr < 0x4000 {
                0
            } else if addr < 0x8000 {
                bank
            } else {
                0xff
            }
        };
        let addresses =
            ui::disasm::instruction_addresses(mem.as_slice(), &[pc, target.unwrap_or(pc)]);
        let mut rows = Vec::with_capacity(addresses.len());
        let mut target_row = None;
        for addr in addresses {
            if self
                .debugger_state
                .first_label_for(bank_for(addr), addr)
                .is_some()
            {
                rows.push((addr, true));
            }
            if target == Some(addr) {
                target_row = Some(rows.len());
            }
            rows.push((addr, false));
        }
        let colors = self.ui_config.debugger_colors.clone();
        ui.horizontal_wrapped(|ui| {
            for (label, role) in [("Opcode", Role::Opcode), ("8-bit operand", Role::Operand8), ("16-bit operand", Role::Operand16)] {
                ui.colored_label(colors.color(ui, role), label);
            }
        }).response.on_hover_text("Colors describe the displayed decoding; arbitrary data can also decode as instructions. Configure colors in Settings > Developer.");
        let height = ui
            .text_style_height(&egui::TextStyle::Monospace)
            .max(ui.spacing().interact_size.y);
        let stride = height + ui.spacing().item_spacing.y;
        let mut scroll = egui::ScrollArea::both()
            .auto_shrink([false, false])
            .id_salt("disasm_scroll");
        if let Some(row) = target_row {
            scroll = scroll.vertical_scroll_offset(
                (row as f32 * stride - ui.available_height() / 2.0).max(0.0),
            );
        }
        scroll.show_rows(ui, height, rows.len(), |ui, range| {
            for index in range {
                let (addr, label_row) = rows[index];
                let bp = BreakpointSpec {
                    addr,
                    bank: bank_for(addr),
                };
                if label_row {
                    let name = self
                        .debugger_state
                        .first_label_for(bp.bank, addr)
                        .unwrap_or_default();
                    let (short_name, local) = label_name(name);
                    ui.horizontal(|ui| {
                        ui.set_min_height(height);
                        // Match the breakpoint gutter and the two-character PC
                        // marker before the bank:address column below.
                        let font_size = ui.text_style_height(&egui::TextStyle::Monospace);
                        let space = ui.fonts_mut(|fonts| {
                            fonts.glyph_width(&egui::FontId::monospace(font_size), ' ')
                        });
                        ui.add_space(
                            16.0 + ui.spacing().item_spacing.x
                                + space * if local { 4.0 } else { 2.0 },
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("{short_name}:"))
                                    .monospace()
                                    .strong()
                                    .size(font_size + if local { 0.0 } else { 1.0 })
                                    .background_color(if local {
                                        egui::Color32::TRANSPARENT
                                    } else {
                                        ui.visuals().faint_bg_color
                                    })
                                    .color(colors.color(ui, Role::Symbol)),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        )
                        .on_hover_text(name);
                    });
                    continue;
                }
                let bytes = [
                    mem[addr as usize],
                    mem[addr.wrapping_add(1) as usize],
                    mem[addr.wrapping_add(2) as usize],
                ];
                let (mut text, len, operand_target) = ui::disasm::decode_sm83(&bytes, addr);
                if let Some(target) = operand_target
                    && let Some(name) = self
                        .debugger_state
                        .first_label_for(bank_for(target), target)
                        .or_else(|| self.debugger_state.first_label_for(0, target))
                {
                    text = text.replace(&format!("${target:04X}"), name);
                }
                let is_pc = addr == pc;
                let selected = self.debugger_state.cursor() == Some(bp);
                let mut job = egui::text::LayoutJob::default();
                let marker = if is_pc { ">" } else { " " };
                colors.append(
                    ui,
                    &mut job,
                    &format!("{marker} {:02X}:{addr:04X}  ", bp.bank),
                    Role::Plain,
                );
                for (offset, byte) in bytes.iter().enumerate().take(len as usize) {
                    colors.append(
                        ui,
                        &mut job,
                        &format!("{byte:02X} "),
                        byte_role(bytes[0], offset),
                    );
                }
                colors.append(
                    ui,
                    &mut job,
                    &" ".repeat((3 - len as usize) * 3 + 1),
                    Role::Plain,
                );
                colors.instruction(ui, &mut job, &text, bytes[0]);
                if is_pc || selected {
                    let background = if is_pc {
                        ui.visuals().selection.bg_fill.gamma_multiply(0.45)
                    } else {
                        ui.visuals().faint_bg_color
                    };
                    for section in &mut job.sections {
                        section.format.background = background;
                    }
                }
                ui.horizontal(|ui| {
                    let enabled = self.debugger_state.has_breakpoint(&bp);
                    let symbol = match enabled {
                        Some(true) => "●",
                        Some(false) => "○",
                        None => " ",
                    };
                    if ui
                        .add_sized(
                            [16.0, height],
                            egui::Button::new(
                                egui::RichText::new(symbol).color(egui::Color32::RED),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Toggle breakpoint")
                        .clicked()
                    {
                        self.debugger_state.toggle_breakpoint(bp);
                    }
                    if ui
                        .add(
                            egui::Label::new(job)
                                .wrap_mode(egui::TextWrapMode::Extend)
                                .sense(egui::Sense::click()),
                        )
                        .clicked()
                    {
                        self.debugger_state.set_cursor(bp);
                    }
                });
            }
        });
    }
}

fn label_name(name: &str) -> (&str, bool) {
    name.find('.')
        .map_or((name, false), |dot| (&name[dot..], true))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rgbds_local_labels_keep_their_short_name() {
        assert_eq!(super::label_name("DelayFrame"), ("DelayFrame", false));
        assert_eq!(super::label_name("DelayFrame.wait"), (".wait", true));
    }
}
