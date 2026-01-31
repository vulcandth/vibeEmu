use eframe::egui::Key;
use log::warn;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone)]
pub struct KeyBindings {
    joypad: HashMap<Key, u8>,
    pause: Key,
    fast_forward: Key,
    quit: Key,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::defaults()
    }
}

impl KeyBindings {
    pub fn defaults() -> Self {
        let mut joypad = HashMap::new();
        joypad.insert(Key::ArrowRight, 0x01);
        joypad.insert(Key::ArrowLeft, 0x02);
        joypad.insert(Key::ArrowUp, 0x04);
        joypad.insert(Key::ArrowDown, 0x08);
        joypad.insert(Key::S, 0x20); // B
        joypad.insert(Key::A, 0x10); // A
        joypad.insert(Key::Tab, 0x40); // Select (egui doesn't distinguish Shift L/R easily)
        joypad.insert(Key::Enter, 0x80); // Start

        Self {
            joypad,
            pause: Key::P,
            fast_forward: Key::Space,
            quit: Key::Escape,
        }
    }

    pub fn load_from_file(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            warn!(
                "Failed to read keybinds file {}; using defaults",
                path.display()
            );
            return Self::defaults();
        };

        let mut bindings = Self::defaults();

        for (line_no, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            let Some((name, value)) = line.split_once('=') else {
                warn!(
                    "Ignoring invalid keybinds line {}:{} (expected name = value)",
                    path.display(),
                    line_no + 1
                );
                continue;
            };

            let name = name.trim();
            let value = value.trim();
            let Some(code) = parse_key(value) else {
                warn!(
                    "Ignoring keybinds line {}:{} (unknown Key '{value}')",
                    path.display(),
                    line_no + 1
                );
                continue;
            };

            match name {
                "up" => {
                    bindings.joypad.retain(|_, &mut m| m != 0x04);
                    bindings.joypad.insert(code, 0x04);
                }
                "down" => {
                    bindings.joypad.retain(|_, &mut m| m != 0x08);
                    bindings.joypad.insert(code, 0x08);
                }
                "left" => {
                    bindings.joypad.retain(|_, &mut m| m != 0x02);
                    bindings.joypad.insert(code, 0x02);
                }
                "right" => {
                    bindings.joypad.retain(|_, &mut m| m != 0x01);
                    bindings.joypad.insert(code, 0x01);
                }
                "a" => {
                    bindings.joypad.retain(|_, &mut m| m != 0x10);
                    bindings.joypad.insert(code, 0x10);
                }
                "b" => {
                    bindings.joypad.retain(|_, &mut m| m != 0x20);
                    bindings.joypad.insert(code, 0x20);
                }
                "start" => {
                    bindings.joypad.retain(|_, &mut m| m != 0x80);
                    bindings.joypad.insert(code, 0x80);
                }
                "select" => {
                    bindings.joypad.retain(|_, &mut m| m != 0x40);
                    bindings.joypad.insert(code, 0x40);
                }
                "pause" => bindings.pause = code,
                "fast_forward" => bindings.fast_forward = code,
                "quit" => bindings.quit = code,
                other => warn!(
                    "Ignoring unknown keybind name '{other}' in {}:{}",
                    path.display(),
                    line_no + 1
                ),
            }
        }

        bindings
    }

    pub fn joypad_mask_for(&self, key: Key) -> Option<u8> {
        self.joypad.get(&key).copied()
    }

    pub fn pause_key(&self) -> Key {
        self.pause
    }

    pub fn fast_forward_key(&self) -> Key {
        self.fast_forward
    }

    pub fn quit_key(&self) -> Key {
        self.quit
    }

    pub fn iter(&self) -> impl Iterator<Item = (String, &Key)> {
        let joypad_names = [
            (0x01, "right"),
            (0x02, "left"),
            (0x04, "up"),
            (0x08, "down"),
            (0x10, "a"),
            (0x20, "b"),
            (0x40, "select"),
            (0x80, "start"),
        ];

        joypad_names
            .into_iter()
            .filter_map(|(mask, name)| {
                self.joypad
                    .iter()
                    .find(|&(_, m)| *m == mask)
                    .map(|(k, _)| (name.to_string(), k))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn key_for_joypad_mask(&self, mask: u8) -> Option<Key> {
        self.joypad
            .iter()
            .find(|&(_, &m)| m == mask)
            .map(|(k, _)| *k)
    }

    pub fn rebind(&mut self, target: crate::RebindTarget, key: Key) {
        match target {
            crate::RebindTarget::Joypad(mask) => {
                self.joypad.retain(|_, &mut m| m != mask);
                self.joypad.insert(key, mask);
            }
            crate::RebindTarget::Pause => self.pause = key,
            crate::RebindTarget::FastForward => self.fast_forward = key,
            crate::RebindTarget::Quit => self.quit = key,
        }
    }
}

fn parse_key(raw: &str) -> Option<Key> {
    let s = raw.trim();

    match s {
        "ArrowUp" | "Up" => Some(Key::ArrowUp),
        "ArrowDown" | "Down" => Some(Key::ArrowDown),
        "ArrowLeft" | "Left" => Some(Key::ArrowLeft),
        "ArrowRight" | "Right" => Some(Key::ArrowRight),
        "Enter" => Some(Key::Enter),
        "Escape" => Some(Key::Escape),
        "Space" => Some(Key::Space),
        "Tab" => Some(Key::Tab),
        "Backspace" => Some(Key::Backspace),
        _ => {
            if s.len() == 1 {
                let c = s.chars().next()?;
                if c.is_ascii_alphabetic() {
                    return match c.to_ascii_uppercase() {
                        'A' => Some(Key::A),
                        'B' => Some(Key::B),
                        'C' => Some(Key::C),
                        'D' => Some(Key::D),
                        'E' => Some(Key::E),
                        'F' => Some(Key::F),
                        'G' => Some(Key::G),
                        'H' => Some(Key::H),
                        'I' => Some(Key::I),
                        'J' => Some(Key::J),
                        'K' => Some(Key::K),
                        'L' => Some(Key::L),
                        'M' => Some(Key::M),
                        'N' => Some(Key::N),
                        'O' => Some(Key::O),
                        'P' => Some(Key::P),
                        'Q' => Some(Key::Q),
                        'R' => Some(Key::R),
                        'S' => Some(Key::S),
                        'T' => Some(Key::T),
                        'U' => Some(Key::U),
                        'V' => Some(Key::V),
                        'W' => Some(Key::W),
                        'X' => Some(Key::X),
                        'Y' => Some(Key::Y),
                        'Z' => Some(Key::Z),
                        _ => None,
                    };
                }
                if c.is_ascii_digit() {
                    return match c {
                        '0' => Some(Key::Num0),
                        '1' => Some(Key::Num1),
                        '2' => Some(Key::Num2),
                        '3' => Some(Key::Num3),
                        '4' => Some(Key::Num4),
                        '5' => Some(Key::Num5),
                        '6' => Some(Key::Num6),
                        '7' => Some(Key::Num7),
                        '8' => Some(Key::Num8),
                        '9' => Some(Key::Num9),
                        _ => None,
                    };
                }
            }
            None
        }
    }
}
