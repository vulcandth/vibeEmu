#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
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
