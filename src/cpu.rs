const fn opcode_cycles() -> [u8; 256] {
    let mut arr = [0u8; 256];
    arr[0x00] = 4; // NOP
    arr[0x06] = 8; // LD B,d8
    arr[0x0E] = 8; // LD C,d8
    arr[0x16] = 8; // LD D,d8
    arr[0x1E] = 8; // LD E,d8
    arr[0x26] = 8; // LD H,d8
    arr[0x2E] = 8; // LD L,d8
    arr[0x3E] = 8; // LD A,d8
    arr[0x77] = 8; // LD (HL),A
    arr[0xAF] = 4; // XOR A
    arr[0x18] = 12; // JR r8
    arr[0x20] = 8; // JR NZ,r8 (not taken)
    arr[0x28] = 8; // JR Z,r8 (not taken)
    arr[0x30] = 8; // JR NC,r8 (not taken)
    arr[0x38] = 8; // JR C,r8 (not taken)
    arr[0xC3] = 16; // JP a16
    arr[0xF3] = 4; // DI
    arr[0xFB] = 4; // EI
    arr[0x76] = 4; // HALT
    arr[0xD9] = 16; // RETI
    arr
}

const OPCODE_CYCLES: [u8; 256] = opcode_cycles();

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
    pub ime: bool,
    pub halted: bool,
    ime_delay: bool,
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
            ime: false,
            halted: false,
            ime_delay: false,
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

    fn push_stack(&mut self, mmu: &mut crate::mmu::Mmu, val: u16) {
        self.sp = self.sp.wrapping_sub(1);
        mmu.write_byte(self.sp, (val >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        mmu.write_byte(self.sp, val as u8);
    }

    fn pop_stack(&mut self, mmu: &mut crate::mmu::Mmu) -> u16 {
        let lo = mmu.read_byte(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        let hi = mmu.read_byte(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        (hi << 8) | lo
    }

    fn handle_interrupts(&mut self, mmu: &mut crate::mmu::Mmu) {
        let pending = mmu.if_reg & mmu.ie_reg;
        if pending == 0 {
            return;
        }

        if self.ime {
            self.halted = false;

            let vector = if pending & 0x01 != 0 {
                mmu.if_reg &= !0x01;
                0x40
            } else if pending & 0x02 != 0 {
                mmu.if_reg &= !0x02;
                0x48
            } else if pending & 0x04 != 0 {
                mmu.if_reg &= !0x04;
                0x50
            } else if pending & 0x08 != 0 {
                mmu.if_reg &= !0x08;
                0x58
            } else {
                mmu.if_reg &= !0x10;
                0x60
            };

            let pc = self.pc;
            self.push_stack(mmu, pc);
            self.pc = vector;
            self.ime = false;
            self.cycles += 20;
        } else if self.halted {
            self.halted = false;
        }
    }

    pub fn step(&mut self, mmu: &mut crate::mmu::Mmu) {
        if self.halted {
            self.cycles += 4;
            self.handle_interrupts(mmu);
            return;
        }

        let enable_after = self.ime_delay;
        let opcode = mmu.read_byte(self.pc);
        self.pc = self.pc.wrapping_add(1);
        let mut extra_cycles = 0u8;

        match opcode {
            0x00 => {}
            0x06 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.b = val;
            }
            0x0E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.c = val;
            }
            0x16 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.d = val;
            }
            0x1E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.e = val;
            }
            0x26 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.h = val;
            }
            0x2E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.l = val;
            }
            0x3E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a = val;
            }
            0x77 => {
                let addr = self.get_hl();
                mmu.write_byte(addr, self.a);
            }
            0xAF => {
                self.a ^= self.a;
                // XOR A resets NF, HF, CF and sets Z
                self.f = 0x80;
            }
            0x18 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                self.pc = self.pc.wrapping_add(offset as u16);
            }
            0x20 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f & 0x80 == 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    extra_cycles = 4;
                }
            }
            0x28 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f & 0x80 != 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    extra_cycles = 4;
                }
            }
            0x30 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f & 0x10 == 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    extra_cycles = 4;
                }
            }
            0x38 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f & 0x10 != 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    extra_cycles = 4;
                }
            }
            0xC3 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = (hi << 8) | lo;
            }
            0xF3 => {
                self.ime = false;
            }
            0xFB => {
                self.ime_delay = true;
            }
            0x76 => {
                self.halted = true;
            }
            0xD9 => {
                self.pc = self.pop_stack(mmu);
                self.ime = true;
            }
            _ => panic!("unhandled opcode {:02X}", opcode),
        }

        self.cycles += OPCODE_CYCLES[opcode as usize] as u64 + extra_cycles as u64;

        if enable_after {
            self.ime = true;
            self.ime_delay = false;
        }
        self.handle_interrupts(mmu);
    }
}
