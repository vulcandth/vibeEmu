pub struct Ppu {
    pub vram: [u8; 0x2000],
    regs: [u8; 0x0C],
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            vram: [0; 0x2000],
            regs: [0; 0x0C],
        }
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        self.regs[(addr - 0xFF40) as usize]
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        self.regs[(addr - 0xFF40) as usize] = val;
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}
