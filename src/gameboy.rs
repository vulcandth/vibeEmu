use crate::{apu::Apu, cpu::Cpu, mmu::Mmu, ppu::Ppu, timer::Timer};

pub struct GameBoy {
    pub cpu: Cpu,
    pub mmu: Mmu,
    pub ppu: Ppu,
    pub apu: Apu,
    pub timer: Timer,
}

impl GameBoy {
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            mmu: Mmu::new(),
            ppu: Ppu::new(),
            apu: Apu::new(),
            timer: Timer::new(),
        }
    }
}

impl Default for GameBoy {
    fn default() -> Self {
        Self::new()
    }
}
