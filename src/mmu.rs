use crate::cartridge::Cartridge;

pub struct Mmu {
    pub wram: [u8; 0x2000],
    pub cart: Option<Cartridge>,
    pub if_reg: u8,
    pub ie_reg: u8,
    sb: u8,
    sc: u8,
    pub serial_out: Vec<u8>,
}

impl Mmu {
    pub fn new() -> Self {
        Self {
            wram: [0; 0x2000],
            cart: None,
            if_reg: 0,
            ie_reg: 0,
            sb: 0,
            sc: 0,
            serial_out: Vec::new(),
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
            0xFF01 => self.sb,
            0xFF02 => self.sc,
            0xFF0F => self.if_reg,
            0xFFFF => self.ie_reg,
            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, val: u8) {
        match addr {
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = val,
            0xFF01 => self.sb = val,
            0xFF02 => {
                self.sc = val;
                if val & 0x80 != 0 {
                    self.serial_out.push(self.sb);
                    self.if_reg |= 0x08;
                    self.sc &= 0x7F;
                }
            }
            0xFF0F => self.if_reg = val,
            0xFFFF => self.ie_reg = val,
            _ => {}
        }
    }

    pub fn take_serial(&mut self) -> Vec<u8> {
        let out = self.serial_out.clone();
        self.serial_out.clear();
        out
    }
}
