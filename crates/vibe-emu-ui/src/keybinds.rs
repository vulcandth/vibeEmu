use log::warn;
use std::collections::HashMap;
use std::path::Path;
use winit::keyboard::KeyCode;

/// Keyboard bindings for the desktop UI.
///
/// This is intentionally minimal: a simple key/value file format with one binding per line.
///
/// Format:
/// - Lines: `name = KeyCode`
/// - `#` starts a comment
///
/// Names:
/// - `up`, `down`, `left`, `right`, `a`, `b`, `start`, `select`
/// - `pause`, `fast_forward`, `quit`
///
/// KeyCode examples:
/// - `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`
/// - `Enter`, `Escape`, `Space`, `ShiftLeft`, `ShiftRight`
/// - `A`..`Z` (letters)
/// - `0`..`9` (digits)
#[derive(Clone)]
pub struct KeyBindings {
    joypad: HashMap<KeyCode, u8>,
    pause: KeyCode,
    fast_forward: KeyCode,
    quit: KeyCode,
}

impl KeyBindings {
    pub fn defaults() -> Self {
        let mut joypad = HashMap::new();
        // Joypad bitmask matches the rest of the UI: 0 = pressed.
        joypad.insert(KeyCode::ArrowRight, 0x01);
        joypad.insert(KeyCode::ArrowLeft, 0x02);
        joypad.insert(KeyCode::ArrowUp, 0x04);
        joypad.insert(KeyCode::ArrowDown, 0x08);
        joypad.insert(KeyCode::KeyS, 0x20); // B
        joypad.insert(KeyCode::KeyA, 0x10); // A
        joypad.insert(KeyCode::ShiftLeft, 0x40); // Select
        joypad.insert(KeyCode::ShiftRight, 0x40); // Select
        joypad.insert(KeyCode::Enter, 0x80); // Start

        Self {
            joypad,
            pause: KeyCode::KeyP,
            fast_forward: KeyCode::Space,
            quit: KeyCode::Escape,
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
            let Some(code) = parse_key_code(value) else {
                warn!(
                    "Ignoring keybinds line {}:{} (unknown KeyCode '{value}')",
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

    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out = String::new();
        out.push_str("# vibeEmu keybinds\n");
        out.push_str("# Format: name = KeyCode\n");
        out.push_str(
            "# Names: up, down, left, right, a, b, start, select, pause, fast_forward, quit\n\n",
        );

        let fmt = |code: KeyCode| format!("{code:?}");
        let key_for_mask = |mask: u8| {
            let mut keys: Vec<KeyCode> = self
                .joypad
                .iter()
                .filter_map(|(&k, &m)| (m == mask).then_some(k))
                .collect();
            keys.sort_by_key(|k| fmt(*k));
            keys.first().copied()
        };

        {
            let mut write = |name: &str, code: Option<KeyCode>| {
                if let Some(code) = code {
                    out.push_str(&format!("{name} = {}\n", fmt(code)));
                } else {
                    out.push_str(&format!("# {name} = <unbound>\n"));
                }
            };

            write("up", key_for_mask(0x04));
            write("down", key_for_mask(0x08));
            write("left", key_for_mask(0x02));
            write("right", key_for_mask(0x01));
            write("a", key_for_mask(0x10));
            write("b", key_for_mask(0x20));
            write("start", key_for_mask(0x80));
            write("select", key_for_mask(0x40));
        }
        out.push('\n');

        {
            let mut write = |name: &str, code: Option<KeyCode>| {
                if let Some(code) = code {
                    out.push_str(&format!("{name} = {}\n", fmt(code)));
                } else {
                    out.push_str(&format!("# {name} = <unbound>\n"));
                }
            };
            write("pause", Some(self.pause));
            write("fast_forward", Some(self.fast_forward));
            write("quit", Some(self.quit));
        }

        std::fs::write(path, out)
    }

    pub fn joypad_mask_for(&self, code: KeyCode) -> Option<u8> {
        self.joypad.get(&code).copied()
    }

    pub fn key_for_joypad_mask(&self, mask: u8) -> Option<KeyCode> {
        self.joypad
            .iter()
            .find_map(|(&k, &m)| if m == mask { Some(k) } else { None })
    }

    pub fn set_joypad_binding(&mut self, mask: u8, code: KeyCode) {
        self.joypad.retain(|_, &mut m| m != mask);
        self.joypad.insert(code, mask);
    }

    pub fn set_pause_key(&mut self, code: KeyCode) {
        self.pause = code;
    }

    pub fn set_fast_forward_key(&mut self, code: KeyCode) {
        self.fast_forward = code;
    }

    pub fn set_quit_key(&mut self, code: KeyCode) {
        self.quit = code;
    }

    pub fn pause_key(&self) -> KeyCode {
        self.pause
    }

    pub fn fast_forward_key(&self) -> KeyCode {
        self.fast_forward
    }

    pub fn quit_key(&self) -> KeyCode {
        self.quit
    }
}

fn parse_key_code(raw: &str) -> Option<KeyCode> {
    let s = raw.trim();

    // Common named keys.
    match s {
        "ArrowUp" => return Some(KeyCode::ArrowUp),
        "ArrowDown" => return Some(KeyCode::ArrowDown),
        "ArrowLeft" => return Some(KeyCode::ArrowLeft),
        "ArrowRight" => return Some(KeyCode::ArrowRight),
        "Enter" => return Some(KeyCode::Enter),
        "Escape" => return Some(KeyCode::Escape),
        "Space" => return Some(KeyCode::Space),
        "ShiftLeft" => return Some(KeyCode::ShiftLeft),
        "ShiftRight" => return Some(KeyCode::ShiftRight),
        "Tab" => return Some(KeyCode::Tab),
        _ => {}
    }

    // Letters: allow "A".."Z".
    // Also accept winit's Debug formatting: "KeyA".."KeyZ".
    if let Some(letter) = s.strip_prefix("Key")
        && letter.len() == 1
        && letter.as_bytes()[0].is_ascii_alphabetic()
    {
        return parse_key_code(letter);
    }

    // Digits: allow "0".."9".
    // Also accept winit's Debug formatting: "Digit0".."Digit9".
    if let Some(d) = s.strip_prefix("Digit")
        && d.len() == 1
        && d.as_bytes()[0].is_ascii_digit()
    {
        return parse_key_code(d);
    }

    if s.len() == 1 {
        let c = s.chars().next()?;
        if c.is_ascii_alphabetic() {
            return match c.to_ascii_uppercase() {
                'A' => Some(KeyCode::KeyA),
                'B' => Some(KeyCode::KeyB),
                'C' => Some(KeyCode::KeyC),
                'D' => Some(KeyCode::KeyD),
                'E' => Some(KeyCode::KeyE),
                'F' => Some(KeyCode::KeyF),
                'G' => Some(KeyCode::KeyG),
                'H' => Some(KeyCode::KeyH),
                'I' => Some(KeyCode::KeyI),
                'J' => Some(KeyCode::KeyJ),
                'K' => Some(KeyCode::KeyK),
                'L' => Some(KeyCode::KeyL),
                'M' => Some(KeyCode::KeyM),
                'N' => Some(KeyCode::KeyN),
                'O' => Some(KeyCode::KeyO),
                'P' => Some(KeyCode::KeyP),
                'Q' => Some(KeyCode::KeyQ),
                'R' => Some(KeyCode::KeyR),
                'S' => Some(KeyCode::KeyS),
                'T' => Some(KeyCode::KeyT),
                'U' => Some(KeyCode::KeyU),
                'V' => Some(KeyCode::KeyV),
                'W' => Some(KeyCode::KeyW),
                'X' => Some(KeyCode::KeyX),
                'Y' => Some(KeyCode::KeyY),
                'Z' => Some(KeyCode::KeyZ),
                _ => None,
            };
        }

        if c.is_ascii_digit() {
            return match c {
                '0' => Some(KeyCode::Digit0),
                '1' => Some(KeyCode::Digit1),
                '2' => Some(KeyCode::Digit2),
                '3' => Some(KeyCode::Digit3),
                '4' => Some(KeyCode::Digit4),
                '5' => Some(KeyCode::Digit5),
                '6' => Some(KeyCode::Digit6),
                '7' => Some(KeyCode::Digit7),
                '8' => Some(KeyCode::Digit8),
                '9' => Some(KeyCode::Digit9),
                _ => None,
            };
        }
    }

    // Also accept winit-style names like "KeyA" / "Digit1".
    match s {
        "KeyA" => Some(KeyCode::KeyA),
        "KeyB" => Some(KeyCode::KeyB),
        "KeyC" => Some(KeyCode::KeyC),
        "KeyD" => Some(KeyCode::KeyD),
        "KeyE" => Some(KeyCode::KeyE),
        "KeyF" => Some(KeyCode::KeyF),
        "KeyG" => Some(KeyCode::KeyG),
        "KeyH" => Some(KeyCode::KeyH),
        "KeyI" => Some(KeyCode::KeyI),
        "KeyJ" => Some(KeyCode::KeyJ),
        "KeyK" => Some(KeyCode::KeyK),
        "KeyL" => Some(KeyCode::KeyL),
        "KeyM" => Some(KeyCode::KeyM),
        "KeyN" => Some(KeyCode::KeyN),
        "KeyO" => Some(KeyCode::KeyO),
        "KeyP" => Some(KeyCode::KeyP),
        "KeyQ" => Some(KeyCode::KeyQ),
        "KeyR" => Some(KeyCode::KeyR),
        "KeyS" => Some(KeyCode::KeyS),
        "KeyT" => Some(KeyCode::KeyT),
        "KeyU" => Some(KeyCode::KeyU),
        "KeyV" => Some(KeyCode::KeyV),
        "KeyW" => Some(KeyCode::KeyW),
        "KeyX" => Some(KeyCode::KeyX),
        "KeyY" => Some(KeyCode::KeyY),
        "KeyZ" => Some(KeyCode::KeyZ),
        "Digit0" => Some(KeyCode::Digit0),
        "Digit1" => Some(KeyCode::Digit1),
        "Digit2" => Some(KeyCode::Digit2),
        "Digit3" => Some(KeyCode::Digit3),
        "Digit4" => Some(KeyCode::Digit4),
        "Digit5" => Some(KeyCode::Digit5),
        "Digit6" => Some(KeyCode::Digit6),
        "Digit7" => Some(KeyCode::Digit7),
        "Digit8" => Some(KeyCode::Digit8),
        "Digit9" => Some(KeyCode::Digit9),
        _ => None,
    }
}
