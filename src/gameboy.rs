use crate::{cpu::Cpu, mmu::Mmu};

pub struct GameBoy {
    pub cpu: Cpu,
    pub mmu: Mmu,
}

impl GameBoy {
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            mmu: Mmu::new(),
        }
    }
}

impl Default for GameBoy {
    fn default() -> Self {
        Self::new()
    }
}
