//! Platform-neutral application preferences and action semantics.
//! This crate deliberately owns no windows, devices, or emulation hardware.
use serde::{Deserialize, Serialize};

/// Desktop workspaces share one emulation session.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Workspace {
    #[default]
    Play,
    Develop,
}

/// Actions available through menus, keyboard shortcuts, and controllers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    OpenRom,
    ReloadRom,
    CloseRom,
    TogglePause,
    Reset,
    Screenshot,
    SaveStates,
    QuickSave,
    QuickLoad,
    UndoLoad,
    ToggleMute,
    ToggleFullscreen,
    ToggleMenu,
    Settings,
    Play,
    Develop,
    Quit,
    RunNoBreak,
    StepInto,
    StepOver,
    StepOut,
    RunToCursor,
    RunToCursorNoBreak,
    JumpToCursor,
    CallCursor,
    JumpStack,
    ToggleBreakpoint,
    ReloadSymbols,
}

impl Action {
    /// Commands belonging to the desktop debugger.
    pub const fn is_debugger(self) -> bool {
        matches!(
            self,
            Self::RunNoBreak
                | Self::StepInto
                | Self::StepOver
                | Self::StepOut
                | Self::RunToCursor
                | Self::RunToCursorNoBreak
                | Self::JumpToCursor
                | Self::CallCursor
                | Self::JumpStack
                | Self::ToggleBreakpoint
                | Self::ReloadSymbols
        )
    }
    /// Whether the action requires a loaded, idle (not loading) session.
    pub const fn requires_game(self) -> bool {
        self.is_debugger()
            || matches!(
                self,
                Self::ReloadRom
                    | Self::CloseRom
                    | Self::TogglePause
                    | Self::Reset
                    | Self::Screenshot
                    | Self::SaveStates
                    | Self::QuickSave
                    | Self::QuickLoad
                    | Self::UndoLoad
            )
    }

    /// One availability rule shared by all command surfaces.
    pub const fn available(self, loaded: bool, loading: bool) -> bool {
        if matches!(self, Self::OpenRom) {
            !loading
        } else {
            !self.requires_game() || (loaded && !loading)
        }
    }
}

/// Stable settings categories. Frontends only show categories with capabilities.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    #[default]
    General,
    System,
    Video,
    Audio,
    Controls,
    Capture,
    Peripherals,
    Files,
    Developer,
}

impl SettingsCategory {
    pub const ALL: [Self; 9] = [
        Self::General,
        Self::System,
        Self::Video,
        Self::Audio,
        Self::Controls,
        Self::Capture,
        Self::Peripherals,
        Self::Files,
        Self::Developer,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::System => "System & Boot",
            Self::Video => "Video & Colors",
            Self::Audio => "Audio",
            Self::Controls => "Controls",
            Self::Capture => "Capture",
            Self::Peripherals => "Peripherals",
            Self::Files => "Files",
            Self::Developer => "Developer",
        }
    }

    pub fn matches(self, query: &str) -> bool {
        let keywords = match self {
            Self::General => "theme appearance focus pause status fullscreen",
            Self::System => "model hardware revision boot rom reset reload",
            Self::Video => "palette color sampling filter border scale lcd scanlines",
            Self::Audio => "sound mute volume mono stereo",
            Self::Controls => "keyboard controller gamepad keybind shortcut input",
            Self::Capture => "screenshot png capture directory",
            Self::Peripherals => "serial link cable mobile adapter network dns relay",
            Self::Files => "recent pinned rom files directory",
            Self::Developer => "debugger symbols watchpoint breakpoint font memory workspace",
        };
        let haystack = format!("{} {keywords}", self.label()).to_lowercase();
        query
            .split_whitespace()
            .all(|word| haystack.contains(&word.to_lowercase()))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// Backward-compatible settings added by the workspace redesign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub version: u32,
    pub workspace: Workspace,
    pub theme: Theme,
    pub ui_scale: f32,
    pub pause_on_focus_loss: bool,
    pub background_controllers: bool,
    pub volume: u8,
    pub mono: bool,
    pub speed_percent: u16,
    pub fast_forward_percent: u16,
    pub show_status: bool,
    pub double_click_fullscreen: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            workspace: Workspace::Play,
            theme: Theme::System,
            ui_scale: 1.0,
            pause_on_focus_loss: false,
            background_controllers: false,
            volume: 100,
            mono: false,
            speed_percent: 100,
            fast_forward_percent: 200,
            show_status: true,
            double_click_fullscreen: true,
        }
    }
}

/// Normalize unsupported numeric values from hand-edited configuration.
impl Preferences {
    pub fn normalize(&mut self) {
        self.ui_scale = if self.ui_scale.is_finite() {
            self.ui_scale.clamp(0.75, 2.5)
        } else {
            1.0
        };
        self.volume = self.volume.min(100);
        self.speed_percent = self.speed_percent.clamp(1, 400);
        self.fast_forward_percent = self.fast_forward_percent.clamp(100, 1000);
    }
}

/// Opposing digital directions cancel, leaving other buttons unchanged.
pub const fn neutralize_opposites(mut state: u8) -> u8 {
    if state & 3 == 3 {
        state &= !3;
    }
    if state & 12 == 12 {
        state &= !12;
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrations_and_invalid_preferences() {
        let mut p: Preferences =
            toml::from_str("volume = 250\nspeed_percent = 0\nui_scale = nan").unwrap();
        p.normalize();
        assert_eq!(p.workspace, Workspace::Play);
        assert_eq!((p.volume, p.speed_percent, p.ui_scale), (100, 1, 1.0));
        assert_eq!(
            toml::from_str::<Preferences>(&toml::to_string(&p).unwrap()).unwrap(),
            p
        );
    }
    #[test]
    fn input_and_command_boundaries() {
        assert_eq!(neutralize_opposites(0xff), 0xf0);
        assert_eq!(neutralize_opposites(0x95), 0x95);
        assert!(!Action::Reset.available(false, false));
        assert!(!Action::OpenRom.available(true, true));
        assert!(Action::Settings.available(false, true));
        assert!(SettingsCategory::System.matches("boot model"));
        assert!(!SettingsCategory::Audio.matches("boot"));
    }
}
