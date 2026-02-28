use log::warn;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SerialPeripheralKind {
    #[default]
    None,
    MobileAdapter,
    LinkCable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SerialConfig {
    pub peripheral: SerialPeripheralKind,
    pub link_host: String,
    pub link_port: String,
    pub mobile_dns1: String,
    pub mobile_dns2: String,
    pub mobile_relay: String,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            peripheral: SerialPeripheralKind::None,
            link_host: "127.0.0.1".to_string(),
            link_port: "5000".to_string(),
            mobile_dns1: String::new(),
            mobile_dns2: String::new(),
            mobile_relay: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum EmulationMode {
    #[default]
    Auto,
    ForceDmg,
    ForceCgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AxisFilter {
    #[default]
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DisplayEffect {
    #[default]
    None,
    Scanlines,
    LcdGrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VideoFilterConfig {
    pub horizontal: AxisFilter,
    pub vertical: AxisFilter,
    pub effect: DisplayEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WindowSize {
    #[serde(rename = "1x")]
    X1,
    #[serde(rename = "2x")]
    #[default]
    X2,
    #[serde(rename = "3x")]
    X3,
    #[serde(rename = "4x")]
    X4,
    #[serde(rename = "5x")]
    X5,
    #[serde(rename = "6x")]
    X6,
    #[serde(rename = "fullscreen")]
    Fullscreen,
    #[serde(rename = "fullscreen-stretched")]
    FullscreenStretched,
}

impl WindowSize {
    pub fn scale_factor_px(&self) -> Option<u32> {
        match self {
            Self::X1 => Some(1),
            Self::X2 => Some(2),
            Self::X3 => Some(3),
            Self::X4 => Some(4),
            Self::X5 => Some(5),
            Self::X6 => Some(6),
            Self::Fullscreen | Self::FullscreenStretched => None,
        }
    }

    pub fn is_fullscreen(self) -> bool {
        matches!(self, Self::Fullscreen | Self::FullscreenStretched)
    }

    pub fn use_integer_scaling(self) -> bool {
        !matches!(self, Self::FullscreenStretched)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub dmg_bootrom_path: Option<PathBuf>,
    pub cgb_bootrom_path: Option<PathBuf>,
    pub recent_roms: Vec<PathBuf>,
    pub window_size: WindowSize,
    pub sound_enabled: bool,
    pub emulation_mode: EmulationMode,
    pub video_filter: VideoFilterConfig,
    pub serial: SerialConfig,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            dmg_bootrom_path: None,
            cgb_bootrom_path: None,
            recent_roms: Vec::new(),
            window_size: WindowSize::default(),
            sound_enabled: true,
            emulation_mode: EmulationMode::default(),
            video_filter: VideoFilterConfig::default(),
            serial: SerialConfig::default(),
        }
    }
}

pub fn default_ui_config_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("vibeemu").join("ui.toml");
        }
    }

    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("vibeemu").join("ui.toml");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("vibeemu")
            .join("ui.toml");
    }

    PathBuf::from("ui.toml")
}

pub fn load_from_file(path: &PathBuf) -> UiConfig {
    let text = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return UiConfig::default(),
    };

    match toml::from_str::<UiConfig>(&text) {
        Ok(cfg) => cfg,
        Err(e) => {
            warn!(
                "Failed to parse UI config {}: {e}; using defaults",
                path.display()
            );
            UiConfig::default()
        }
    }
}

pub fn save_to_file(path: &PathBuf, cfg: &UiConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let text = toml::to_string_pretty(cfg).unwrap_or_else(|_| String::new());
    std::fs::write(path, text)
}
