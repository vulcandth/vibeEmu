//! Presentation-only instruction and byte highlighting. No emulated reads occur here.
use eframe::egui::{self, Color32, FontId, TextFormat, text::LayoutJob};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Opcode,
    Operand8,
    Operand16,
    Register,
    Symbol,
    Plain,
}

/// CB's second byte is part of the opcode; STOP's padding is not an immediate.
pub fn byte_role(opcode: u8, offset: usize) -> Role {
    if offset == 0 || (opcode == 0xcb && offset == 1) {
        Role::Opcode
    } else if opcode == 0x10 {
        Role::Plain
    } else {
        match super::code_data::sm83_instr_len(opcode) {
            2 if offset == 1 => Role::Operand8,
            3 if offset < 3 => Role::Operand16,
            _ => Role::Plain,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub enabled: bool,
    /// Optional overrides for opcode, byte operand, word operand, register and symbol.
    pub custom: std::collections::BTreeMap<String, [u8; 3]>,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            enabled: true,
            custom: Default::default(),
        }
    }
}

impl Colors {
    pub fn color(&self, ui: &egui::Ui, role: Role) -> Color32 {
        if !self.enabled || role == Role::Plain {
            return ui.visuals().text_color();
        }
        let index = role as usize;
        let defaults = if ui.visuals().dark_mode {
            [
                [112, 194, 255],
                [245, 195, 100],
                [209, 156, 255],
                [118, 215, 184],
                [245, 166, 183],
            ]
        } else {
            [
                [0, 76, 144],
                [132, 72, 0],
                [108, 42, 157],
                [0, 105, 74],
                [150, 34, 73],
            ]
        };
        let [r, g, b] = self
            .custom
            .get(&index.to_string())
            .copied()
            .unwrap_or(defaults[index]);
        Color32::from_rgb(r, g, b)
    }

    pub fn append(&self, ui: &egui::Ui, job: &mut LayoutJob, text: &str, role: Role) {
        job.append(
            text,
            0.0,
            TextFormat {
                font_id: FontId::monospace(ui.text_style_height(&egui::TextStyle::Monospace)),
                color: self.color(ui, role),
                ..Default::default()
            },
        );
    }

    pub fn instruction(&self, ui: &egui::Ui, job: &mut LayoutJob, text: &str, opcode: u8) {
        let mut first = true;
        for token in text.split_inclusive(|c: char| !(c.is_alphanumeric() || "_.$@".contains(c))) {
            let word =
                token.trim_end_matches(|c: char| !(c.is_alphanumeric() || "_.$@".contains(c)));
            let role = if first && !word.is_empty() {
                first = false;
                Role::Opcode
            } else if matches!(
                word,
                "a" | "b"
                    | "c"
                    | "d"
                    | "e"
                    | "h"
                    | "l"
                    | "af"
                    | "bc"
                    | "de"
                    | "hl"
                    | "hli"
                    | "hld"
                    | "sp"
                    | "nz"
                    | "z"
                    | "nc"
            ) {
                Role::Register
            } else if word.starts_with('$')
                || word.chars().next().is_some_and(|c| c.is_ascii_digit())
            {
                if super::code_data::sm83_instr_len(opcode) == 3 {
                    Role::Operand16
                } else {
                    Role::Operand8
                }
            } else if !word.is_empty() {
                Role::Symbol
            } else {
                Role::Plain
            };
            self.append(ui, job, word, role);
            self.append(ui, job, &token[word.len()..], Role::Plain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_roles_preserve_cb_opcodes_and_word_operands() {
        assert_eq!(byte_role(0xcb, 1), Role::Opcode);
        assert_eq!(byte_role(0x3e, 1), Role::Operand8);
        assert_eq!(byte_role(0xcd, 1), Role::Operand16);
        assert_eq!(byte_role(0xcd, 2), Role::Operand16);
        assert_eq!(byte_role(0x10, 1), Role::Plain);
        assert_eq!(byte_role(0x00, 1), Role::Plain);
    }

    #[test]
    fn custom_colors_and_accessibility_toggle_round_trip() {
        let mut colors = Colors {
            enabled: false,
            ..Default::default()
        };
        colors.custom.insert("2".into(), [230, 190, 80]);
        let text = toml::to_string(&colors).unwrap();
        assert_eq!(toml::from_str::<Colors>(&text).unwrap(), colors);
    }
}
