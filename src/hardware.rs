#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DmgRevision {
    Rev0,
    RevA,
    RevB,
    #[default]
    RevC,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CgbRevision {
    Rev0,
    RevA,
    RevB,
    RevC,
    RevD,
    #[default]
    RevE,
}

impl CgbRevision {
    #[inline]
    pub const fn supports_de_window(self) -> bool {
        matches!(self, CgbRevision::RevD | CgbRevision::RevE)
    }

    #[inline]
    pub const fn has_pcm_mask_glitch(self) -> bool {
        matches!(
            self,
            CgbRevision::Rev0 | CgbRevision::RevA | CgbRevision::RevB | CgbRevision::RevC
        )
    }
}
