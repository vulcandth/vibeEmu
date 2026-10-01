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
    AutoPreferCgb,
    AutoPreferSgb,
    ForceDmg,
    ForceCgb,
    ForceCgbSgb,
    CgbInitialSgbBorder,
    ForceMgb,
    ForceSgb,
    ForceSgb2,
    ForceAgb0,
    ForceAgb,
}

impl EmulationMode {
    pub fn model(self, cart_cgb: bool) -> vibe_emu_core::hardware::Model {
        use vibe_emu_core::hardware::Model;
        match self {
            Self::Auto | Self::AutoPreferCgb | Self::AutoPreferSgb => {
                Model::from_cgb_flag(cart_cgb)
            }
            Self::ForceDmg => Model::from_cgb_flag(false),
            Self::ForceCgb | Self::ForceCgbSgb | Self::CgbInitialSgbBorder => {
                Model::from_cgb_flag(true)
            }
            Self::ForceMgb => Model::Mgb,
            Self::ForceSgb => Model::Sgb,
            Self::ForceSgb2 => Model::Sgb2,
            Self::ForceAgb0 => Model::Agb0,
            Self::ForceAgb => Model::Agb,
        }
    }

    pub fn model_for_cart(
        self,
        cart: &vibe_emu_core::cartridge::Cartridge,
    ) -> vibe_emu_core::hardware::Model {
        let sgb = cart.rom.get(0x146) == Some(&3) && cart.rom.get(0x14b) == Some(&0x33);
        if sgb && (self == Self::AutoPreferSgb || (self == Self::AutoPreferCgb && !cart.cgb)) {
            vibe_emu_core::hardware::Model::Sgb
        } else {
            self.model(cart.cgb)
        }
    }
}

/// Stable ordering used by boot ROM settings, loaders and validation.
pub const BOOT_MODELS: [(&str, vibe_emu_core::hardware::Model); 7] = {
    use vibe_emu_core::hardware::{CgbRevision, DmgRevision, Model};
    [
        ("DMG", Model::Dmg(DmgRevision::RevB)),
        ("MGB", Model::Mgb),
        ("SGB", Model::Sgb),
        ("SGB2", Model::Sgb2),
        ("CGB", Model::Cgb(CgbRevision::RevE)),
        ("AGB0", Model::Agb0),
        ("AGB", Model::Agb),
    ]
};

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
    pub debugger_colors: crate::ui::highlight::Colors,
    pub dmg_revision: u8,
    pub cgb_revision: u8,
    pub dmg_palette: Option<[u32; 4]>,
    pub preferences: vibe_emu_frontend::Preferences,
    pub pinned_roms: Vec<PathBuf>,
    pub screenshot_directory: Option<PathBuf>,
    pub dmg_bootrom_path: Option<PathBuf>,
    pub cgb_bootrom_path: Option<PathBuf>,
    pub mgb_bootrom_path: Option<PathBuf>,
    pub sgb_bootrom_path: Option<PathBuf>,
    pub sgb2_bootrom_path: Option<PathBuf>,
    pub agb0_bootrom_path: Option<PathBuf>,
    pub agb_bootrom_path: Option<PathBuf>,
    pub show_sgb_border: bool,
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
            debugger_colors: crate::ui::highlight::Colors::default(),
            dmg_revision: 2,
            cgb_revision: 5,
            dmg_palette: None,
            preferences: vibe_emu_frontend::Preferences::default(),
            pinned_roms: Vec::new(),
            screenshot_directory: None,
            dmg_bootrom_path: None,
            cgb_bootrom_path: None,
            mgb_bootrom_path: None,
            sgb_bootrom_path: None,
            sgb2_bootrom_path: None,
            agb0_bootrom_path: None,
            agb_bootrom_path: None,
            show_sgb_border: true,
            recent_roms: Vec::new(),
            window_size: WindowSize::default(),
            sound_enabled: true,
            emulation_mode: EmulationMode::default(),
            video_filter: VideoFilterConfig::default(),
            serial: SerialConfig::default(),
        }
    }
}

impl UiConfig {
    pub fn bootrom_paths(&self) -> [Option<PathBuf>; 7] {
        [
            self.dmg_bootrom_path.clone(),
            self.mgb_bootrom_path.clone(),
            self.sgb_bootrom_path.clone(),
            self.sgb2_bootrom_path.clone(),
            self.cgb_bootrom_path.clone(),
            self.agb0_bootrom_path.clone(),
            self.agb_bootrom_path.clone(),
        ]
    }

    pub fn set_bootrom_paths(&mut self, paths: [Option<PathBuf>; 7]) {
        [
            self.dmg_bootrom_path,
            self.mgb_bootrom_path,
            self.sgb_bootrom_path,
            self.sgb2_bootrom_path,
            self.cgb_bootrom_path,
            self.agb0_bootrom_path,
            self.agb_bootrom_path,
        ] = paths;
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
        Ok(mut cfg) => {
            cfg.preferences.normalize();
            cfg
        }
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

    let text = toml::to_string_pretty(cfg).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibe_emu_core::{cartridge::Cartridge, hardware::Model};

    #[test]
    fn old_settings_and_all_model_bootroms_round_trip() {
        let mut cfg: UiConfig = toml::from_str("dmg_bootrom_path = 'old-dmg.bin'\ncgb_bootrom_path = 'old-cgb.bin'\nemulation_mode = 'force-cgb-sgb'\n").unwrap();
        assert_eq!(cfg.emulation_mode, EmulationMode::ForceCgbSgb);
        assert_eq!(
            cfg.preferences.workspace,
            vibe_emu_frontend::Workspace::Play
        );
        assert_eq!((cfg.dmg_revision, cfg.cgb_revision), (2, 5));
        assert_eq!(cfg.preferences.volume, 100);
        assert!(cfg.dmg_palette.is_none());
        assert!(cfg.show_sgb_border);
        assert_eq!(cfg.bootrom_paths()[4], Some(PathBuf::from("old-cgb.bin")));
        assert!(cfg.sgb_bootrom_path.is_none());
        let paths = BOOT_MODELS.map(|(label, _)| Some(PathBuf::from(format!("{label}.bin"))));
        cfg.set_bootrom_paths(paths.clone());
        cfg.emulation_mode = EmulationMode::CgbInitialSgbBorder;
        cfg.show_sgb_border = false;
        let restored: UiConfig = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(restored.bootrom_paths(), paths);
        assert_eq!(restored.emulation_mode, cfg.emulation_mode);
        assert!(!restored.show_sgb_border);
    }

    #[test]
    fn automatic_modes_honor_both_flags_and_licensee() {
        for cgb in [false, true] {
            for sgb in [false, true] {
                for licensee in [0, 0x33] {
                    let mut rom = vec![0; 0x8000];
                    rom[0x143] = if cgb { 0x80 } else { 0 };
                    rom[0x146] = if sgb { 3 } else { 0 };
                    rom[0x14b] = licensee;
                    let cart = Cartridge::from_bytes(rom);
                    assert_eq!(
                        EmulationMode::Auto.model_for_cart(&cart),
                        Model::from_cgb_flag(cgb)
                    );
                    assert_eq!(
                        EmulationMode::AutoPreferCgb.model_for_cart(&cart),
                        if !cgb && sgb && licensee == 0x33 {
                            Model::Sgb
                        } else {
                            Model::from_cgb_flag(cgb)
                        }
                    );
                    assert_eq!(
                        EmulationMode::AutoPreferSgb.model_for_cart(&cart),
                        if sgb && licensee == 0x33 {
                            Model::Sgb
                        } else {
                            Model::from_cgb_flag(cgb)
                        }
                    );
                }
            }
        }
    }
}
