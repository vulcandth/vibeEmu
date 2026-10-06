#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
/// DMG hardware revision.
///
/// Used to model revision-specific quirks that affect timing and observable
/// behavior.
#[derive(serde::Serialize, serde::Deserialize)]
pub enum DmgRevision {
    /// Original DMG revision 0.
    Rev0,
    /// DMG revision A.
    RevA,
    /// DMG revision B.
    RevB,
    #[default]
    /// DMG revision C (default).
    RevC,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
/// CGB hardware revision.
///
/// Used to model revision-specific quirks (e.g. PPU and APU behaviors) that
/// differ across CGB motherboard revisions.
#[derive(serde::Serialize, serde::Deserialize)]
pub enum CgbRevision {
    /// Original CGB revision 0.
    Rev0,
    /// CGB revision A.
    RevA,
    /// CGB revision B.
    RevB,
    /// CGB revision C.
    RevC,
    /// CGB revision D.
    RevD,
    #[default]
    /// CGB revision E (default).
    RevE,
}

impl CgbRevision {
    #[inline]
    /// Returns whether this revision supports the DE window behavior.
    pub const fn supports_de_window(self) -> bool {
        matches!(self, CgbRevision::RevD | CgbRevision::RevE)
    }

    #[inline]
    /// Returns whether this revision exhibits the PCM mask glitch.
    pub const fn has_pcm_mask_glitch(self) -> bool {
        matches!(
            self,
            CgbRevision::Rev0 | CgbRevision::RevA | CgbRevision::RevB | CgbRevision::RevC
        )
    }
}

/// Hardware model and revision of the emulated system.
///
/// Combines the system family with its board/silicon revision
/// into a single typed value. This eliminates the `cgb: bool` parameter that
/// was previously threaded through every constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Model {
    /// Original Game Boy (DMG) with the given board revision.
    Dmg(DmgRevision),
    /// Game Boy Color (CGB) with the given board revision.
    Cgb(CgbRevision),
    /// Game Boy Pocket / Light.
    Mgb,
    /// Super Game Boy (NTSC clock), Game Boy subsystem.
    Sgb,
    /// Super Game Boy 2, Game Boy subsystem.
    Sgb2,
    /// Game Boy Advance in GB compatibility mode, original boot ROM.
    Agb0,
    /// Game Boy Advance in GB compatibility mode, revised boot ROM.
    Agb,
}

impl Model {
    /// Returns `true` when this model has Color hardware (CGB or AGB).
    #[inline]
    pub const fn is_cgb(self) -> bool {
        matches!(self, Model::Cgb(_) | Model::Agb0 | Model::Agb)
    }

    /// Returns `true` when this model has monochrome hardware (DMG, MGB or SGB).
    #[inline]
    pub const fn is_dmg(self) -> bool {
        !self.is_cgb()
    }

    /// Returns the DMG board revision, if this is a DMG model.
    #[inline]
    pub const fn dmg_revision(self) -> Option<DmgRevision> {
        match self {
            Model::Dmg(rev) => Some(rev),
            _ => None,
        }
    }

    /// Returns the CGB timing profile. AGB inherits the late CGB profile;
    /// AGB-specific differences are selected with [`Self::is_agb`].
    #[inline]
    pub const fn cgb_revision(self) -> Option<CgbRevision> {
        match self {
            Model::Cgb(rev) => Some(rev),
            Model::Agb0 | Model::Agb => Some(CgbRevision::RevE),
            _ => None,
        }
    }

    /// Whether this is an Advance running a GB/GBC cartridge.
    pub const fn is_agb(self) -> bool {
        matches!(self, Self::Agb0 | Self::Agb)
    }

    /// Whether this is a Super Game Boy's monochrome subsystem.
    pub const fn is_sgb(self) -> bool {
        matches!(self, Self::Sgb | Self::Sgb2)
    }

    /// Dot clock in Hz. SGB1 derives its clock from the NTSC SNES master clock.
    pub const fn clock_hz(self) -> u32 {
        if matches!(self, Self::Sgb) {
            4_295_454
        } else {
            4_194_304
        }
    }

    /// Build a `Model` from a CGB-mode flag, using default revisions.
    #[inline]
    pub fn from_cgb_flag(cgb: bool) -> Self {
        if cgb {
            Model::Cgb(CgbRevision::default())
        } else {
            Model::Dmg(DmgRevision::default())
        }
    }
}

impl Default for Model {
    fn default() -> Self {
        Model::Dmg(DmgRevision::default())
    }
}
