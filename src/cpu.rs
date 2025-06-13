pub struct Cpu {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub pc: u16,
    pub sp: u16,
    pub cycles: u64,
}

impl Cpu {
    pub fn new() -> Self {
        // Default post-boot state for DMG as documented in Pan Docs. PC is
        // normally 0x0100 after the boot ROM, but tests may override this.
        Self {
            a: 0x01,
            f: 0xB0,
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            pc: 0x0100,
            sp: 0xFFFE,
            cycles: 0,
        }
    }

    fn get_bc(&self) -> u16 {
        ((self.b as u16) << 8) | self.c as u16
    }

    fn set_bc(&mut self, val: u16) {
        self.b = (val >> 8) as u8;
        self.c = val as u8;
    }

    fn get_de(&self) -> u16 {
        ((self.d as u16) << 8) | self.e as u16
    }

    fn set_de(&mut self, val: u16) {
        self.d = (val >> 8) as u8;
        self.e = val as u8;
    }

    fn get_hl(&self) -> u16 {
        ((self.h as u16) << 8) | self.l as u16
    }

    fn set_hl(&mut self, val: u16) {
        self.h = (val >> 8) as u8;
        self.l = val as u8;
    }

    pub fn step(&mut self, mmu: &mut crate::mmu::Mmu) {
        let opcode = mmu.read_byte(self.pc);
        self.pc = self.pc.wrapping_add(1);

        match opcode {
            0x00 => {
                // NOP
                self.cycles += 4;
            }
            0x06 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.b = val;
                self.cycles += 8;
            }
            0x0E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.c = val;
                self.cycles += 8;
            }
            0x16 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.d = val;
                self.cycles += 8;
            }
            0x1E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.e = val;
                self.cycles += 8;
            }
            0x26 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.h = val;
                self.cycles += 8;
            }
            0x2E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.l = val;
                self.cycles += 8;
            }
            0x3E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a = val;
                self.cycles += 8;
            }
            0x77 => {
                let addr = self.get_hl();
                mmu.write_byte(addr, self.a);
                self.cycles += 8;
            }
            0xAF => {
                self.a ^= self.a;
                // XOR A resets NF, HF, CF and sets Z
                self.f = 0x80;
                self.cycles += 4;
            }
            0xC3 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = (hi << 8) | lo;
                self.cycles += 16;
            }
            _ => panic!("unhandled opcode {:02X}", opcode),
        }
    }
}
