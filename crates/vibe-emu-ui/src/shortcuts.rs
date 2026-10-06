//! One shortcut table for input, menu hints and the controls reference.
use super::*;
use egui::{Key, KeyboardShortcut, Modifiers};

pub const DEBUG_COMMANDS: &[(Action, &str)] = &[
    (Action::TogglePause, "Run / pause"),
    (Action::RunNoBreak, "Run without breakpoints"),
    (Action::StepInto, "Step into"),
    (Action::StepOver, "Step over"),
    (Action::StepOut, "Step out"),
    (Action::RunToCursor, "Run to cursor"),
    (
        Action::RunToCursorNoBreak,
        "Run to cursor without breakpoints",
    ),
    (Action::JumpToCursor, "Jump to cursor"),
    (Action::CallCursor, "Call cursor"),
    (Action::JumpStack, "Jump to stack return address"),
    (Action::ToggleBreakpoint, "Toggle cursor breakpoint"),
    (Action::ReloadSymbols, "Reload symbols"),
];

pub fn defaults(mac: bool) -> Vec<(Action, KeyboardShortcut)> {
    let command = Modifiers::COMMAND;
    let shift_command = command | Modifiers::SHIFT;
    let mut bindings = vec![
        (
            Action::QuickSave,
            KeyboardShortcut::new(Modifiers::NONE, Key::F5),
        ),
        (
            Action::QuickLoad,
            KeyboardShortcut::new(Modifiers::SHIFT, Key::F5),
        ),
        (Action::UndoLoad, KeyboardShortcut::new(command, Key::F5)),
        (Action::OpenRom, KeyboardShortcut::new(command, Key::O)),
        (
            Action::ReloadRom,
            KeyboardShortcut::new(shift_command, Key::R),
        ),
        (Action::CloseRom, KeyboardShortcut::new(command, Key::W)),
        (Action::Reset, KeyboardShortcut::new(command, Key::R)),
        (Action::ToggleMute, KeyboardShortcut::new(command, Key::M)),
        (Action::Settings, KeyboardShortcut::new(command, Key::Comma)),
        (Action::Quit, KeyboardShortcut::new(command, Key::Q)),
        (Action::Play, KeyboardShortcut::new(command, Key::Num1)),
        (Action::Develop, KeyboardShortcut::new(command, Key::Num2)),
        (
            Action::TogglePause,
            KeyboardShortcut::new(Modifiers::NONE, Key::F9),
        ),
        (
            Action::RunNoBreak,
            KeyboardShortcut::new(Modifiers::SHIFT, Key::F9),
        ),
        (
            Action::StepInto,
            KeyboardShortcut::new(Modifiers::NONE, Key::F7),
        ),
        (
            Action::StepOver,
            KeyboardShortcut::new(Modifiers::NONE, Key::F3),
        ),
        (
            Action::StepOut,
            KeyboardShortcut::new(Modifiers::NONE, Key::F8),
        ),
        (
            Action::RunToCursor,
            KeyboardShortcut::new(Modifiers::NONE, Key::F4),
        ),
        (
            Action::RunToCursorNoBreak,
            KeyboardShortcut::new(Modifiers::SHIFT, Key::F4),
        ),
        (
            Action::JumpToCursor,
            KeyboardShortcut::new(Modifiers::NONE, Key::F6),
        ),
        (
            Action::CallCursor,
            KeyboardShortcut::new(Modifiers::SHIFT, Key::F6),
        ),
        (Action::JumpStack, KeyboardShortcut::new(command, Key::F8)),
        (
            Action::ToggleBreakpoint,
            KeyboardShortcut::new(Modifiers::NONE, Key::F2),
        ),
        (
            Action::ReloadSymbols,
            KeyboardShortcut::new(shift_command, Key::L),
        ),
    ];
    bindings.push((
        Action::ToggleFullscreen,
        if mac {
            KeyboardShortcut::new(command | Modifiers::CTRL, Key::F)
        } else {
            KeyboardShortcut::new(Modifiers::NONE, Key::F11)
        },
    ));
    bindings
}

fn consume_exact(input: &mut egui::InputState, shortcut: &KeyboardShortcut) -> bool {
    // egui's consume_shortcut deliberately ignores extra Shift/Alt modifiers.
    // Here F9 and Shift+F9 are different commands, so match the full combination.
    let mut consumed = false;
    input.events.retain(|event| {
        let matches = matches!(event,
            egui::Event::Key { key, pressed: true, modifiers, .. }
            if *key == shortcut.logical_key && modifiers.matches_exact(shortcut.modifiers));
        consumed |= matches;
        !matches
    });
    consumed
}

impl VibeEmuApp {
    pub(super) fn shortcut_text(&self, ctx: &egui::Context, action: Action) -> String {
        if action == Action::Screenshot {
            return format!("{:?}", self.keybinds.screenshot_key());
        }
        defaults(cfg!(target_os = "macos"))
            .into_iter()
            .find(|(a, _)| *a == action)
            .map_or_else(String::new, |(_, shortcut)| ctx.format_shortcut(&shortcut))
    }

    pub(super) fn action_enabled(&self, action: Action) -> bool {
        if self.states.pending.is_some() && (action.requires_game() || action == Action::OpenRom) {
            return false;
        }
        if !action.available(self.current_rom_path.is_some(), self.loading.is_some()) {
            return false;
        }
        if !action.is_debugger() {
            return true;
        }
        if self.ui_config.preferences.workspace != Workspace::Develop
            && !self.show_debugger
            && self.develop_layout.floating_panels().is_empty()
        {
            return false;
        }
        if !self.paused && action != Action::ReloadSymbols {
            return false;
        }
        !matches!(
            action,
            Action::RunToCursor
                | Action::RunToCursorNoBreak
                | Action::JumpToCursor
                | Action::CallCursor
                | Action::ToggleBreakpoint
        ) || self.debugger_state.cursor().is_some()
    }

    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Modified shortcuts remain available in text fields. Unmodified F keys
        // do not execute while editing values or rebinding gameplay input.
        for (action, shortcut) in defaults(cfg!(target_os = "macos")) {
            if action.is_debugger() && (self.show_options || ctx.egui_wants_keyboard_input()) {
                continue;
            }
            if self.action_enabled(action) && ctx.input_mut(|i| consume_exact(i, &shortcut)) {
                self.dispatch_action(ctx, action);
            }
        }
    }

    pub(super) fn draw_shortcuts_reference(&self, ui: &mut egui::Ui) {
        ui.heading("Application shortcuts");
        for (action, label) in [
            (Action::OpenRom, "Open ROM"),
            (Action::ReloadRom, "Reload ROM"),
            (Action::CloseRom, "Close ROM"),
            (Action::ToggleMute, "Mute / unmute"),
            (Action::Reset, "Reset"),
            (Action::Settings, "Settings"),
            (Action::ToggleFullscreen, "Fullscreen"),
            (Action::Play, "Play workspace"),
            (Action::Develop, "Develop workspace"),
            (Action::Quit, "Quit"),
        ]
        .into_iter()
        .chain(DEBUG_COMMANDS.iter().copied())
        {
            ui.label(format!(
                "{} — {label}",
                self.shortcut_text(ui.ctx(), action)
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_shortcut_dispatches_only_its_exact_command() {
        for mac in [false, true] {
            let bindings = defaults(mac);
            for (expected, shortcut) in &bindings {
                let mut modifiers = shortcut.modifiers;
                if modifiers.command {
                    if mac {
                        modifiers.mac_cmd = true;
                    } else {
                        modifiers.ctrl = true;
                    }
                }
                let mut input = egui::InputState::default();
                input.events.push(egui::Event::Key {
                    key: shortcut.logical_key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                });
                let dispatched: Vec<_> = bindings
                    .iter()
                    .filter_map(|(action, candidate)| {
                        consume_exact(&mut input, candidate).then_some(*action)
                    })
                    .collect();
                assert_eq!(dispatched, vec![*expected]);
            }
        }
    }

    #[test]
    fn shortcuts_are_unique_on_each_platform_and_use_command() {
        for mac in [false, true] {
            let bindings = defaults(mac);
            for (n, (_, shortcut)) in bindings.iter().enumerate() {
                assert!(
                    !bindings[..n]
                        .iter()
                        .any(|(_, previous)| previous == shortcut)
                );
            }
            for action in [Action::OpenRom, Action::ToggleMute] {
                assert!(
                    bindings
                        .iter()
                        .find(|(a, _)| *a == action)
                        .unwrap()
                        .1
                        .modifiers
                        .command
                );
            }
        }
    }
}
