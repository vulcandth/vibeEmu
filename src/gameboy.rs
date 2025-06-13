use crate::{cpu::Cpu, mmu::Mmu};

pub struct GameBoy {
    pub cpu: Cpu,
    pub mmu: Mmu,
    pub cgb: bool,
}

impl GameBoy {
    pub fn new() -> Self {
        Self::new_with_mode(false)
    }

    pub fn new_with_mode(cgb: bool) -> Self {
        Self {
            cpu: Cpu::new(),
            mmu: Mmu::new(),
            cgb,
        }
    }
}

impl Default for GameBoy {
    fn default() -> Self {
        Self::new()
    }
}
