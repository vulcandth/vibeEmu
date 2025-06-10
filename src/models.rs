// src/models.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)] // To allow AGB, SGB etc.
pub enum GameBoyModel {
    DMG,  // Original Dot Matrix Gameboy
    SGB,  // Super Game Boy (needs SNES integration, but model exists)
    SGB2, // Super Game Boy 2
    // MGB,   // Game Boy Pocket - Often treated as DMG for emulation purposes
    // CGB0,  // CGB Initial CPU (CPU CGB) - SameBoy specific
    CGB0,       // CGB CPU Revision 0
    CGBA,       // CGB CPU Revision A
    CGBB,       // CGB CPU Revision B
    CGBC,       // CGB CPU Revision C
    CGBD,       // CGB CPU Revision D
    CGBE,       // CGB CPU Revision E (Last CGB revision)
    AGB,        // Game Boy Advance (in GB/CGB compatibility mode)
    AGS,        // Game Boy Advance SP (similar to AGB for GB/CGB mode)
    GenericCGB, // Used if CGB flag is set but specific revision is unknown/unimportant
}

impl GameBoyModel {
    pub fn is_cgb_family(&self) -> bool {
        matches!(
            self,
            GameBoyModel::GenericCGB
                | GameBoyModel::CGB0
                | GameBoyModel::CGBA
                | GameBoyModel::CGBB
                | GameBoyModel::CGBC
                | GameBoyModel::CGBD
                | GameBoyModel::CGBE
        )
    }

    pub fn is_dmg_family(&self) -> bool {
        // Includes MGB (often same as DMG), SGBs for some APU traits
        matches!(
            self,
            GameBoyModel::DMG | GameBoyModel::SGB | GameBoyModel::SGB2
        )
    }

    pub fn is_agb_family(&self) -> bool {
        matches!(self, GameBoyModel::AGB | GameBoyModel::AGS)
    }

    // Example for a specific CGB check that SameBoy might use:
    // SameBoy often has checks like `gb->model <= GB_MODEL_CGB_C` or `gb->model > GB_MODEL_CGB_E`.
    // GB_MODEL_CGB_0 to GB_MODEL_CGB_E map to our CGB0-CGBE
    pub fn is_cgb_c_or_older(&self) -> bool {
        match self {
            GameBoyModel::CGB0 | GameBoyModel::CGBA | GameBoyModel::CGBB | GameBoyModel::CGBC => {
                true
            }
            GameBoyModel::GenericCGB => true, // Assume generic CGB might be older for safety unless specified for a particular feature
            _ => false,
        }
    }

    pub fn is_cgb_d_or_e(&self) -> bool {
        matches!(self, GameBoyModel::CGBD | GameBoyModel::CGBE)
    }

    // Add more specific checks as needed, e.g.:
    // pub fn is_sgb(&self) -> bool {
    //     matches!(self, GameBoyModel::SGB | GameBoyModel::SGB2)
    // }

    // pub fn is_cgb_e_or_later(&self) -> bool {
    //     matches!(self, GameBoyModel::CGBE) // Or add future CGB models if any were made
    // }
}

// Default model if not otherwise specified (e.g. for tests or generic runs)
impl Default for GameBoyModel {
    fn default() -> Self {
        GameBoyModel::DMG // Or GenericCGB depending on desired default
    }
}
