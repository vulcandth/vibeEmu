use eframe::egui::Key;
use log::{info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn default_keybinds_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("vibeemu").join("keybinds.toml");
        }
    }

    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("vibeemu").join("keybinds.toml");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("vibeemu")
            .join("keybinds.toml");
    }

    PathBuf::from("keybinds.toml")
}

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

    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut lines = Vec::new();
        lines.push("# vibeEmu keybinds configuration".to_string());
        lines.push(String::new());

        let joypad_names = [
            (0x04, "up"),
            (0x08, "down"),
            (0x02, "left"),
            (0x01, "right"),
            (0x10, "a"),
            (0x20, "b"),
            (0x40, "select"),
            (0x80, "start"),
        ];

        for (mask, name) in joypad_names {
            if let Some(key) = self.key_for_joypad_mask(mask) {
                lines.push(format!("{} = {}", name, key_to_string(key)));
            }
        }

        lines.push(String::new());
        lines.push(format!("pause = {}", key_to_string(self.pause)));
        lines.push(format!(
            "fast_forward = {}",
            key_to_string(self.fast_forward)
        ));
        lines.push(format!("quit = {}", key_to_string(self.quit)));

        let content = lines.join("\n");
        std::fs::write(path, content)?;
        info!("Saved keybinds to {}", path.display());
        Ok(())
    }
}

fn key_to_string(key: Key) -> String {
    match key {
        Key::ArrowUp => "Up".to_string(),
        Key::ArrowDown => "Down".to_string(),
        Key::ArrowLeft => "Left".to_string(),
        Key::ArrowRight => "Right".to_string(),
        Key::Enter => "Enter".to_string(),
        Key::Escape => "Escape".to_string(),
        Key::Space => "Space".to_string(),
        Key::Tab => "Tab".to_string(),
        Key::Backspace => "Backspace".to_string(),
        Key::A => "A".to_string(),
        Key::B => "B".to_string(),
        Key::C => "C".to_string(),
        Key::D => "D".to_string(),
        Key::E => "E".to_string(),
        Key::F => "F".to_string(),
        Key::G => "G".to_string(),
        Key::H => "H".to_string(),
        Key::I => "I".to_string(),
        Key::J => "J".to_string(),
        Key::K => "K".to_string(),
        Key::L => "L".to_string(),
        Key::M => "M".to_string(),
        Key::N => "N".to_string(),
        Key::O => "O".to_string(),
        Key::P => "P".to_string(),
        Key::Q => "Q".to_string(),
        Key::R => "R".to_string(),
        Key::S => "S".to_string(),
        Key::T => "T".to_string(),
        Key::U => "U".to_string(),
        Key::V => "V".to_string(),
        Key::W => "W".to_string(),
        Key::X => "X".to_string(),
        Key::Y => "Y".to_string(),
        Key::Z => "Z".to_string(),
        Key::Num0 => "0".to_string(),
        Key::Num1 => "1".to_string(),
        Key::Num2 => "2".to_string(),
        Key::Num3 => "3".to_string(),
        Key::Num4 => "4".to_string(),
        Key::Num5 => "5".to_string(),
        Key::Num6 => "6".to_string(),
        Key::Num7 => "7".to_string(),
        Key::Num8 => "8".to_string(),
        Key::Num9 => "9".to_string(),
        other => format!("{other:?}"),
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
