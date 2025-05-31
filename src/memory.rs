// src/memory.rs

pub struct Memory {
    wram: [u8; 0x2000], // Work RAM (8KB) - C000-DFFF
    hram: [u8; 0x007F], // High RAM (127 bytes) - FF80-FFFE
}

impl Memory {
    pub fn new() -> Self {
        Self {
            wram: [0; 0x2000],
            hram: [0; 0x007F],
        }
    }

    // Reads a byte from WRAM or HRAM given a global Game Boy address
    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0xC000..=0xDFFF => { // WRAM
                let offset = (addr - 0xC000) as usize;
                self.wram[offset]
            }
            0xE000..=0xFDFF => { // Echo RAM (mirror of WRAM 0xC000-0xDDFF)
                // Address is masked to 0x1FFF, equivalent to `(addr - 0xE000) % 0x2000`
                // but handles the wrap-around for the 8KB WRAM correctly.
                // For example, 0xE000 maps to 0xC000, 0xFDFF maps to 0xDDFF.
                let offset = (addr & 0x1FFF) as usize; // equivalent to (addr - 0xC000) or (addr - 0xE000) within WRAM range
                self.wram[offset]
            }
            0xFF80..=0xFFFE => { // HRAM
                let offset = (addr - 0xFF80) as usize;
                self.hram[offset]
            }
            _ => {
                // This should ideally be caught by the Bus, but as a safeguard:
                panic!(
                    "Memory read attempt at unhandled address: {:#04X}. This address should be handled by the Bus or is invalid.",
                    addr
                );
            }
        }
    }

    // Writes a byte to WRAM or HRAM given a global Game Boy address
    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0xC000..=0xDFFF => { // WRAM
                let offset = (addr - 0xC000) as usize;
                self.wram[offset] = value;
            }
            0xE000..=0xFDFF => { // Echo RAM (mirror of WRAM 0xC000-0xDDFF)
                let offset = (addr & 0x1FFF) as usize;
                self.wram[offset] = value;
            }
            0xFF80..=0xFFFE => { // HRAM
                let offset = (addr - 0xFF80) as usize;
                self.hram[offset] = value;
            }
            _ => {
                // This should ideally be caught by the Bus, but as a safeguard:
                panic!(
                    "Memory write attempt at unhandled address: {:#04X} with value {:#02X}. This address should be handled by the Bus or is invalid.",
                    addr, value
                );
            }
        }
    }
}
