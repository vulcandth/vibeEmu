pub struct Mmu {
    pub wram: [u8; 0x2000],
}

impl Mmu {
    pub fn new() -> Self {
        Self { wram: [0; 0x2000] }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, val: u8) {
        match addr {
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = val,
            _ => {},
        }
    }
}
