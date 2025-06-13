use crate::cartridge::Cartridge;

pub struct Mmu {
    pub wram: [u8; 0x2000],
    pub cart: Option<Cartridge>,
    pub if_reg: u8,
    pub ie_reg: u8,
}

impl Mmu {
    pub fn new() -> Self {
        Self {
            wram: [0; 0x2000],
            cart: None,
            if_reg: 0,
            ie_reg: 0,
        }
    }

    pub fn load_cart(&mut self, cart: Cartridge) {
        self.cart = Some(cart);
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                if let Some(cart) = &self.cart {
                    cart.rom.get(addr as usize).copied().unwrap_or(0xFF)
                } else {
                    0xFF
                }
            }
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xFF0F => self.if_reg,
            0xFFFF => self.ie_reg,
            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, val: u8) {
        match addr {
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = val,
            0xFF0F => self.if_reg = val,
            0xFFFF => self.ie_reg = val,
            _ => {}
        }
    }
}
