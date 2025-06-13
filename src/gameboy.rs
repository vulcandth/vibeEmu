use crate::{cpu::Cpu, mmu::Mmu, ppu::Ppu, apu::Apu, cartridge::Cartridge, timer::Timer};

pub struct GameBoy {
    pub cpu: Cpu,
    pub mmu: Mmu,
    pub ppu: Ppu,
    pub apu: Apu,
    pub timer: Timer,
    pub cart: Option<Cartridge>,
}

impl GameBoy {
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            mmu: Mmu::new(),
            ppu: Ppu::new(),
            apu: Apu::new(),
            timer: Timer::new(),
            cart: None,
        }
    }
}
