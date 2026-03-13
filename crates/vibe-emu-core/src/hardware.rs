#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
/// DMG hardware revision.
///
/// Used to model revision-specific quirks that affect timing and observable
/// behavior.
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
/// Combines the system family (DMG or CGB) with its board/silicon revision
/// into a single typed value. This eliminates the `cgb: bool` parameter that
/// was previously threaded through every constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Original Game Boy (DMG) with the given board revision.
    Dmg(DmgRevision),
    /// Game Boy Color (CGB) with the given board revision.
    Cgb(CgbRevision),
}

impl Model {
    /// Returns `true` when this model is a CGB (Game Boy Color).
    #[inline]
    pub const fn is_cgb(self) -> bool {
        matches!(self, Model::Cgb(_))
    }

    /// Returns `true` when this model is a DMG (original Game Boy).
    #[inline]
    pub const fn is_dmg(self) -> bool {
        matches!(self, Model::Dmg(_))
    }

    /// Returns the DMG board revision, if this is a DMG model.
    #[inline]
    pub const fn dmg_revision(self) -> Option<DmgRevision> {
        match self {
            Model::Dmg(rev) => Some(rev),
            Model::Cgb(_) => None,
        }
    }

    /// Returns the CGB board revision, if this is a CGB model.
    #[inline]
    pub const fn cgb_revision(self) -> Option<CgbRevision> {
        match self {
            Model::Cgb(rev) => Some(rev),
            Model::Dmg(_) => None,
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
