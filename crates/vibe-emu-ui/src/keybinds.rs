use eframe::egui::Key;
use log::{info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingKey {
    Key(Key),
    ShiftLeft,
    ShiftRight,
}

impl std::fmt::Debug for BindingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Key(key) => key.fmt(f),
            Self::ShiftLeft => f.write_str("Left Shift"),
            Self::ShiftRight => f.write_str("Right Shift"),
        }
    }
}

impl From<Key> for BindingKey {
    fn from(key: Key) -> Self {
        Self::Key(key)
    }
}

impl BindingKey {
    pub fn down(self, input: &eframe::egui::InputState, shift: [bool; 2]) -> bool {
        match self {
            Self::Key(key) => input.key_down(key),
            Self::ShiftLeft => shift[0],
            Self::ShiftRight => shift[1],
        }
    }

    pub fn pressed(self, input: &eframe::egui::InputState, shift: [bool; 2]) -> bool {
        match self {
            Self::Key(key) => input.key_pressed(key),
            Self::ShiftLeft => shift[0],
            Self::ShiftRight => shift[1],
        }
    }
}

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
    joypad: HashMap<BindingKey, u8>,
    pause: BindingKey,
    fast_forward: BindingKey,
    screenshot: BindingKey,
    quit: BindingKey,
    quit_bound: bool,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::defaults()
    }
}

impl KeyBindings {
    pub fn defaults() -> Self {
        let mut joypad = HashMap::new();
        joypad.insert(BindingKey::Key(Key::ArrowRight), 0x01);
        joypad.insert(BindingKey::Key(Key::ArrowLeft), 0x02);
        joypad.insert(BindingKey::Key(Key::ArrowUp), 0x04);
        joypad.insert(BindingKey::Key(Key::ArrowDown), 0x08);
        joypad.insert(BindingKey::Key(Key::S), 0x20); // B
        joypad.insert(BindingKey::Key(Key::A), 0x10); // A
        joypad.insert(BindingKey::Key(Key::Tab), 0x40); // Select
        joypad.insert(BindingKey::Key(Key::Enter), 0x80); // Start

        Self {
            joypad,
            pause: BindingKey::Key(Key::P),
            fast_forward: BindingKey::Key(Key::Space),
            screenshot: BindingKey::Key(Key::F12),
            quit: BindingKey::Key(Key::Escape),
            quit_bound: false,
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
                "screenshot" => bindings.screenshot = code,
                "quit" => {
                    bindings.quit = code;
                    bindings.quit_bound = true;
                }
                other => warn!(
                    "Ignoring unknown keybind name '{other}' in {}:{}",
                    path.display(),
                    line_no + 1
                ),
            }
        }

        bindings
    }

    pub fn joypad_mask_for(&self, key: BindingKey) -> Option<u8> {
        self.joypad.get(&key).copied()
    }

    pub fn pause_key(&self) -> BindingKey {
        self.pause
    }

    pub fn fast_forward_key(&self) -> BindingKey {
        self.fast_forward
    }

    pub fn quit_is_bound(&self) -> bool {
        self.quit_bound
    }

    pub fn quit_key(&self) -> BindingKey {
        self.quit
    }

    pub fn screenshot_key(&self) -> BindingKey {
        self.screenshot
    }

    pub fn iter(&self) -> impl Iterator<Item = (String, &BindingKey)> {
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

    pub fn key_for_joypad_mask(&self, mask: u8) -> Option<BindingKey> {
        self.joypad
            .iter()
            .find(|&(_, &m)| m == mask)
            .map(|(k, _)| *k)
    }

    pub fn rebind(&mut self, target: crate::RebindTarget, key: impl Into<BindingKey>) {
        let key = key.into();
        match target {
            crate::RebindTarget::Joypad(mask) => {
                self.joypad.retain(|_, &mut m| m != mask);
                self.joypad.insert(key, mask);
            }
            crate::RebindTarget::Pause => self.pause = key,
            crate::RebindTarget::FastForward => self.fast_forward = key,
            crate::RebindTarget::Screenshot => self.screenshot = key,
            crate::RebindTarget::Quit => {
                self.quit = key;
                self.quit_bound = true;
            }
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
        lines.push(format!("screenshot = {}", key_to_string(self.screenshot)));
        if self.quit_bound {
            lines.push(format!("quit = {}", key_to_string(self.quit)));
        }

        let content = lines.join("\n");
        std::fs::write(path, content)?;
        info!("Saved keybinds to {}", path.display());
        Ok(())
    }
}

fn key_to_string(key: BindingKey) -> String {
    let key = match key {
        BindingKey::Key(key) => key,
        BindingKey::ShiftLeft => return "ShiftLeft".into(),
        BindingKey::ShiftRight => return "ShiftRight".into(),
    };
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
        Key::F1 => "F1".to_string(),
        Key::F2 => "F2".to_string(),
        Key::F3 => "F3".to_string(),
        Key::F4 => "F4".to_string(),
        Key::F5 => "F5".to_string(),
        Key::F6 => "F6".to_string(),
        Key::F7 => "F7".to_string(),
        Key::F8 => "F8".to_string(),
        Key::F9 => "F9".to_string(),
        Key::F10 => "F10".to_string(),
        Key::F11 => "F11".to_string(),
        Key::F12 => "F12".to_string(),
        other => format!("{other:?}"),
    }
}

fn parse_key(raw: &str) -> Option<BindingKey> {
    match raw.trim() {
        "ShiftLeft" | "LShift" => return Some(BindingKey::ShiftLeft),
        "ShiftRight" | "RShift" => return Some(BindingKey::ShiftRight),
        _ => {}
    }
    parse_egui_key(raw).map(BindingKey::Key)
}

fn parse_egui_key(raw: &str) -> Option<Key> {
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
        "F1" => Some(Key::F1),
        "F2" => Some(Key::F2),
        "F3" => Some(Key::F3),
        "F4" => Some(Key::F4),
        "F5" => Some(Key::F5),
        "F6" => Some(Key::F6),
        "F7" => Some(Key::F7),
        "F8" => Some(Key::F8),
        "F9" => Some(Key::F9),
        "F10" => Some(Key::F10),
        "F11" => Some(Key::F11),
        "F12" => Some(Key::F12),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_and_right_shift_bindings_survive_restart_and_drive_distinct_buttons() {
        let path = std::env::temp_dir().join(format!(
            "vibeemu-shift-bindings-{}.toml",
            std::process::id()
        ));
        let mut bindings = KeyBindings::defaults();
        bindings.rebind(crate::RebindTarget::Joypad(0x40), BindingKey::ShiftLeft);
        bindings.rebind(crate::RebindTarget::Joypad(0x80), BindingKey::ShiftRight);
        bindings.rebind(crate::RebindTarget::FastForward, BindingKey::ShiftRight);
        bindings.save_to_file(&path).unwrap();
        let restored = KeyBindings::load_from_file(&path);
        assert_eq!(
            restored.key_for_joypad_mask(0x40),
            Some(BindingKey::ShiftLeft)
        );
        assert_eq!(
            restored.key_for_joypad_mask(0x80),
            Some(BindingKey::ShiftRight)
        );
        assert_eq!(restored.fast_forward_key(), BindingKey::ShiftRight);
        let input = eframe::egui::InputState::default();
        let select = restored.key_for_joypad_mask(0x40).unwrap();
        assert!(select.down(&input, [true, false]));
        assert!(!select.down(&input, [false, true]));
        assert!(!select.down(&input, [false, false]));
        assert!(select.pressed(&input, [true, false]));
        assert_eq!(restored.key_for_joypad_mask(0x10), Some(Key::A.into()));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn escape_menu_default_and_explicit_legacy_quit_survive_round_trip() {
        let directory =
            std::env::temp_dir().join(format!("vibeemu-keybinds-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("bindings.toml");
        KeyBindings::defaults().save_to_file(&path).unwrap();
        assert!(!KeyBindings::load_from_file(&path).quit_is_bound());
        std::fs::write(&path, "a = Q\nquit = Escape\n").unwrap();
        let bindings = KeyBindings::load_from_file(&path);
        assert!(bindings.quit_is_bound());
        assert_eq!(bindings.quit_key(), BindingKey::Key(Key::Escape));
        assert_eq!(
            bindings.key_for_joypad_mask(0x10),
            Some(BindingKey::Key(Key::Q))
        );
        bindings.save_to_file(&path).unwrap();
        assert!(KeyBindings::load_from_file(&path).quit_is_bound());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
