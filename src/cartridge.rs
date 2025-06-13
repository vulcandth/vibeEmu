pub struct Cartridge {
    pub rom: Vec<u8>,
    pub ram: Vec<u8>,
}

impl Cartridge {
    pub fn load(data: Vec<u8>) -> Self {
        Self {
            rom: data,
            ram: vec![0; 0x2000],
        }
    }

    pub fn read_ram(&self, addr: u16) -> u8 {
        self.ram.get(addr as usize).copied().unwrap_or(0xFF)
    }

    pub fn write_ram(&mut self, addr: u16, val: u8) {
        if let Some(b) = self.ram.get_mut(addr as usize) {
            *b = val;
        }
    }
}
