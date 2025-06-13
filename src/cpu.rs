const fn opcode_cycles() -> [u8; 256] {
    let mut arr = [0u8; 256];
    arr[0x00] = 4; // NOP
    arr[0x01] = 12; // LD BC,d16
    arr[0x02] = 8; // LD (BC),A
    arr[0x03] = 8; // INC BC
    arr[0x04] = 4; // INC B
    arr[0x05] = 4; // DEC B
    arr[0x06] = 8; // LD B,d8
    arr[0x07] = 4; // RLCA
    arr[0x08] = 20; // LD (a16),SP
    arr[0x09] = 8; // ADD HL,BC
    arr[0x0A] = 8; // LD A,(BC)
    arr[0x0B] = 8; // DEC BC
    arr[0x0C] = 4; // INC C
    arr[0x0D] = 4; // DEC C
    arr[0x0E] = 8; // LD C,d8
    arr[0x0F] = 4; // RRCA
    arr[0x11] = 12; // LD DE,d16
    arr[0x12] = 8; // LD (DE),A
    arr[0x13] = 8; // INC DE
    arr[0x14] = 4; // INC D
    arr[0x15] = 4; // DEC D
    arr[0x16] = 8; // LD D,d8
    arr[0x17] = 4; // RLA
    arr[0x18] = 12; // JR r8
    arr[0x19] = 8; // ADD HL,DE
    arr[0x1A] = 8; // LD A,(DE)
    arr[0x1B] = 8; // DEC DE
    arr[0x1C] = 4; // INC E
    arr[0x1D] = 4; // DEC E
    arr[0x1E] = 8; // LD E,d8
    arr[0x1F] = 4; // RRA
    arr[0x20] = 8; // JR NZ,r8 (not taken)
    arr[0x21] = 12; // LD HL,d16
    arr[0x22] = 8; // LDI (HL),A
    arr[0x23] = 8; // INC HL
    arr[0x24] = 4; // INC H
    arr[0x25] = 4; // DEC H
    arr[0x26] = 8; // LD H,d8
    arr[0x28] = 8; // JR Z,r8 (not taken)
    arr[0x29] = 8; // ADD HL,HL
    arr[0x2A] = 8; // LDI A,(HL)
    arr[0x2B] = 8; // DEC HL
    arr[0x2C] = 4; // INC L
    arr[0x2D] = 4; // DEC L
    arr[0x2E] = 8; // LD L,d8
    arr[0x2F] = 4; // CPL
    arr[0x27] = 4; // DAA
    arr[0x30] = 8; // JR NC,r8 (not taken)
    arr[0x31] = 12; // LD SP,d16
    arr[0x32] = 8; // LDD (HL),A
    arr[0x33] = 8; // INC SP
    arr[0x34] = 12; // INC (HL)
    arr[0x35] = 12; // DEC (HL)
    arr[0x36] = 12; // LD (HL),d8
    arr[0x37] = 4; // SCF
    arr[0x38] = 8; // JR C,r8 (not taken)
    arr[0x39] = 8; // ADD HL,SP
    arr[0x3A] = 8; // LDD A,(HL)
    arr[0x3B] = 8; // DEC SP
    arr[0x3C] = 4; // INC A
    arr[0x3D] = 4; // DEC A
    arr[0x3E] = 8; // LD A,d8
    arr[0x3F] = 4; // CCF
    arr[0x76] = 4; // HALT
    arr[0x77] = 8; // LD (HL),A
    arr[0xAF] = 4; // XOR A
    arr[0xC3] = 16; // JP a16
    arr[0xC4] = 12; // CALL NZ,a16 (not taken)
    arr[0xC1] = 12; // POP BC
    arr[0xC5] = 16; // PUSH BC
    arr[0xC2] = 12; // JP NZ,a16 (not taken)
    arr[0xC6] = 8; // ADD A,d8
    arr[0xCD] = 24; // CALL a16
    arr[0xC9] = 16; // RET
    arr[0xC0] = 8; // RET NZ (not taken)
    arr[0xC8] = 8; // RET Z (not taken)
    arr[0xCA] = 12; // JP Z,a16 (not taken)
    arr[0xD2] = 12; // JP NC,a16 (not taken)
    arr[0xDA] = 12; // JP C,a16 (not taken)
    arr[0xCE] = 8; // ADC A,d8
    arr[0xCB] = 4; // PREFIX CB
    arr[0xD8] = 8; // RET C (not taken)
    arr[0xD0] = 8; // RET NC (not taken)
    arr[0xD1] = 12; // POP DE
    arr[0xD5] = 16; // PUSH DE
    arr[0xD6] = 8; // SUB d8
    arr[0xD9] = 16; // RETI
    arr[0xDE] = 8; // SBC A,d8
    arr[0xE1] = 12; // POP HL
    arr[0xE5] = 16; // PUSH HL
    arr[0xE0] = 12; // LDH (a8),A
    arr[0xE2] = 8; // LD (C),A
    arr[0xE6] = 8; // AND d8
    arr[0xE8] = 16; // ADD SP,r8
    arr[0xE9] = 4; // JP (HL)
    arr[0xEA] = 16; // LD (a16),A
    arr[0xF1] = 12; // POP AF
    arr[0xF5] = 16; // PUSH AF
    arr[0xF0] = 12; // LDH A,(a8)
    arr[0xF2] = 8; // LD A,(C)
    arr[0xEE] = 8; // XOR d8
    arr[0xF3] = 4; // DI
    arr[0xF6] = 8; // OR d8
    arr[0xF8] = 12; // LD HL,SP+r8
    arr[0xF9] = 8; // LD SP,HL
    arr[0xFA] = 16; // LD A,(a16)
    arr[0xFB] = 4; // EI
    arr[0xFE] = 8; // CP d8
    // ADD A,r
    let mut op = 0x80u8;
    while op <= 0x87 {
        arr[op as usize] = if op & 0x07 == 6 { 8 } else { 4 };
        op += 1;
    }
    // SUB r
    let mut op = 0x90u8;
    while op <= 0x97 {
        arr[op as usize] = if op & 0x07 == 6 { 8 } else { 4 };
        op += 1;
    }
    // AND r
    let mut op = 0xA0u8;
    while op <= 0xA7 {
        arr[op as usize] = if op & 0x07 == 6 { 8 } else { 4 };
        op += 1;
    }
    // XOR r
    let mut op = 0xA8u8;
    while op <= 0xAF {
        arr[op as usize] = if op & 0x07 == 6 { 8 } else { 4 };
        op += 1;
    }
    // OR r
    let mut op = 0xB0u8;
    while op <= 0xB7 {
        arr[op as usize] = if op & 0x07 == 6 { 8 } else { 4 };
        op += 1;
    }
    // CP r
    let mut op = 0xB8u8;
    while op <= 0xBF {
        arr[op as usize] = if op & 0x07 == 6 { 8 } else { 4 };
        op += 1;
    }
    // LD r,r and LD r,(HL)/(HL),r
    let mut op = 0x40u8;
    while op <= 0x7F {
        if op != 0x76 {
            // HALT handled separately
            arr[op as usize] = if op & 0x07 == 6 || (op >> 3) & 0x07 == 6 {
                8
            } else {
                4
            };
        }
        op += 1;
    }
    arr
}

const OPCODE_CYCLES: [u8; 256] = opcode_cycles();

const fn cb_cycles() -> [u8; 256] {
    let mut arr = [8u8; 256];
    let mut op = 0;
    while op < 256 {
        if op & 0x07 == 6 {
            arr[op as usize] = 16;
        }
        op += 1;
    }
    arr
}

const CB_CYCLES: [u8; 256] = cb_cycles();

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

    pub fn get_hl(&self) -> u16 {
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

    fn read_reg(&self, mmu: &mut crate::mmu::Mmu, index: u8) -> u8 {
        match index {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => mmu.read_byte(self.get_hl()),
            7 => self.a,
            _ => unreachable!(),
        }
    }

    fn write_reg(&mut self, mmu: &mut crate::mmu::Mmu, index: u8, val: u8) {
        match index {
            0 => self.b = val,
            1 => self.c = val,
            2 => self.d = val,
            3 => self.e = val,
            4 => self.h = val,
            5 => self.l = val,
            6 => {
                let addr = self.get_hl();
                mmu.write_byte(addr, val);
            }
            7 => self.a = val,
            _ => unreachable!(),
        }
    }

    fn handle_cb(&mut self, opcode: u8, mmu: &mut crate::mmu::Mmu) {
        match opcode {
            0x00..=0x07 => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = val.rotate_left(1);
                self.write_reg(mmu, r, res);
                self.f = if res == 0 { 0x80 } else { 0 } | if val & 0x80 != 0 { 0x10 } else { 0 };
            }
            0x08..=0x0F => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = val.rotate_right(1);
                self.write_reg(mmu, r, res);
                self.f = if res == 0 { 0x80 } else { 0 } | if val & 0x01 != 0 { 0x10 } else { 0 };
            }
            0x10..=0x17 => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let carry_in = if self.f & 0x10 != 0 { 1 } else { 0 };
                let res = (val << 1) | carry_in;
                self.write_reg(mmu, r, res);
                self.f = if res == 0 { 0x80 } else { 0 } | if val & 0x80 != 0 { 0x10 } else { 0 };
            }
            0x18..=0x1F => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let carry_in = if self.f & 0x10 != 0 { 1 } else { 0 };
                let res = (val >> 1) | ((carry_in as u8) << 7);
                self.write_reg(mmu, r, res);
                self.f = if res == 0 { 0x80 } else { 0 } | if val & 0x01 != 0 { 0x10 } else { 0 };
            }
            0x20..=0x27 => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = val << 1;
                self.write_reg(mmu, r, res);
                self.f = if res == 0 { 0x80 } else { 0 } | if val & 0x80 != 0 { 0x10 } else { 0 };
            }
            0x28..=0x2F => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = (val >> 1) | (val & 0x80);
                self.write_reg(mmu, r, res);
                self.f = if res == 0 { 0x80 } else { 0 } | if val & 0x01 != 0 { 0x10 } else { 0 };
            }
            0x30..=0x37 => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = val.rotate_left(4);
                self.write_reg(mmu, r, res);
                self.f = if res == 0 { 0x80 } else { 0 };
            }
            0x38..=0x3F => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = val >> 1;
                self.write_reg(mmu, r, res);
                self.f = if res == 0 { 0x80 } else { 0 } | if val & 0x01 != 0 { 0x10 } else { 0 };
            }
            0x40..=0x7F => {
                let bit = (opcode - 0x40) >> 3;
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                self.f = (self.f & 0x10) | 0x20 | if val & (1 << bit) == 0 { 0x80 } else { 0 };
            }
            0x80..=0xBF => {
                let bit = (opcode - 0x80) >> 3;
                let r = opcode & 0x07;
                let mut val = self.read_reg(mmu, r);
                val &= !(1 << bit);
                self.write_reg(mmu, r, val);
            }
            0xC0..=0xFF => {
                let bit = (opcode - 0xC0) >> 3;
                let r = opcode & 0x07;
                let mut val = self.read_reg(mmu, r);
                val |= 1 << bit;
                self.write_reg(mmu, r, val);
            }
        }
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
            mmu.timer.step(4, &mut mmu.if_reg);
            self.handle_interrupts(mmu);
            return;
        }

        let enable_after = self.ime_delay;
        let opcode = mmu.read_byte(self.pc);
        self.pc = self.pc.wrapping_add(1);
        let mut extra_cycles = 0u8;

        match opcode {
            0x00 => {}
            0x01 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                self.set_bc((hi << 8) | lo);
            }
            0x02 => {
                let addr = self.get_bc();
                mmu.write_byte(addr, self.a);
            }
            0x03 => {
                let val = self.get_bc().wrapping_add(1);
                self.set_bc(val);
            }
            0x04 => {
                let res = self.b.wrapping_add(1);
                self.f = (self.f & 0x10)
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.b & 0x0F) + 1 > 0x0F { 0x20 } else { 0 };
                self.b = res;
            }
            0x05 => {
                let res = self.b.wrapping_sub(1);
                self.f = (self.f & 0x10)
                    | 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if self.b & 0x0F == 0 { 0x20 } else { 0 };
                self.b = res;
            }
            0x06 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.b = val;
            }
            0x07 => {
                let carry = (self.a & 0x80) != 0;
                self.a = self.a.rotate_left(1);
                self.f = if carry { 0x10 } else { 0 };
            }
            0x08 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                let addr = (hi << 8) | lo;
                mmu.write_byte(addr, (self.sp & 0xFF) as u8);
                mmu.write_byte(addr.wrapping_add(1), (self.sp >> 8) as u8);
            }
            0x09 => {
                let hl = self.get_hl();
                let bc = self.get_bc();
                let res = hl.wrapping_add(bc);
                self.f = (self.f & 0x80)
                    | if ((hl & 0x0FFF) + (bc & 0x0FFF)) & 0x1000 != 0 {
                        0x20
                    } else {
                        0
                    }
                    | if (hl as u32 + bc as u32) > 0xFFFF {
                        0x10
                    } else {
                        0
                    };
                self.set_hl(res);
            }
            0x0A => {
                let addr = self.get_bc();
                self.a = mmu.read_byte(addr);
            }
            0x0B => {
                let val = self.get_bc().wrapping_sub(1);
                self.set_bc(val);
            }
            0x0C => {
                let res = self.c.wrapping_add(1);
                self.f = (self.f & 0x10)
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.c & 0x0F) + 1 > 0x0F { 0x20 } else { 0 };
                self.c = res;
            }
            0x0D => {
                let res = self.c.wrapping_sub(1);
                self.f = (self.f & 0x10)
                    | 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if self.c & 0x0F == 0 { 0x20 } else { 0 };
                self.c = res;
            }
            0x0E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.c = val;
            }
            0x0F => {
                let carry = (self.a & 0x01) != 0;
                self.a = self.a.rotate_right(1);
                self.f = if carry { 0x10 } else { 0 };
            }
            0x11 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                self.set_de((hi << 8) | lo);
            }
            0x12 => {
                let addr = self.get_de();
                mmu.write_byte(addr, self.a);
            }
            0x13 => {
                let val = self.get_de().wrapping_add(1);
                self.set_de(val);
            }
            0x14 => {
                let res = self.d.wrapping_add(1);
                self.f = (self.f & 0x10)
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.d & 0x0F) + 1 > 0x0F { 0x20 } else { 0 };
                self.d = res;
            }
            0x15 => {
                let res = self.d.wrapping_sub(1);
                self.f = (self.f & 0x10)
                    | 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if self.d & 0x0F == 0 { 0x20 } else { 0 };
                self.d = res;
            }
            0x16 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.d = val;
            }
            0x17 => {
                let carry = (self.a & 0x80) != 0;
                self.a = (self.a << 1) | if self.f & 0x10 != 0 { 1 } else { 0 };
                self.f = if carry { 0x10 } else { 0 };
            }
            0x18 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                self.pc = self.pc.wrapping_add(offset as u16);
            }
            0x19 => {
                let hl = self.get_hl();
                let de = self.get_de();
                let res = hl.wrapping_add(de);
                self.f = (self.f & 0x80)
                    | if ((hl & 0x0FFF) + (de & 0x0FFF)) & 0x1000 != 0 {
                        0x20
                    } else {
                        0
                    }
                    | if (hl as u32 + de as u32) > 0xFFFF {
                        0x10
                    } else {
                        0
                    };
                self.set_hl(res);
            }
            0x1A => {
                let addr = self.get_de();
                self.a = mmu.read_byte(addr);
            }
            0x1B => {
                let val = self.get_de().wrapping_sub(1);
                self.set_de(val);
            }
            0x1C => {
                let res = self.e.wrapping_add(1);
                self.f = (self.f & 0x10)
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.e & 0x0F) + 1 > 0x0F { 0x20 } else { 0 };
                self.e = res;
            }
            0x1D => {
                let res = self.e.wrapping_sub(1);
                self.f = (self.f & 0x10)
                    | 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if self.e & 0x0F == 0 { 0x20 } else { 0 };
                self.e = res;
            }
            0x1E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.e = val;
            }
            0x1F => {
                let carry = (self.a & 0x01) != 0;
                self.a = (self.a >> 1) | if self.f & 0x10 != 0 { 0x80 } else { 0 };
                self.f = if carry { 0x10 } else { 0 };
            }
            0x20 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f & 0x80 == 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    extra_cycles = 4;
                }
            }
            0x21 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                self.set_hl((hi << 8) | lo);
            }
            0x22 => {
                let addr = self.get_hl();
                mmu.write_byte(addr, self.a);
                self.set_hl(addr.wrapping_add(1));
            }
            0x23 => {
                let val = self.get_hl().wrapping_add(1);
                self.set_hl(val);
            }
            0x24 => {
                let res = self.h.wrapping_add(1);
                self.f = (self.f & 0x10)
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.h & 0x0F) + 1 > 0x0F { 0x20 } else { 0 };
                self.h = res;
            }
            0x25 => {
                let res = self.h.wrapping_sub(1);
                self.f = (self.f & 0x10)
                    | 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if self.h & 0x0F == 0 { 0x20 } else { 0 };
                self.h = res;
            }
            0x26 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.h = val;
            }
            0x28 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f & 0x80 != 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    extra_cycles = 4;
                }
            }
            0x29 => {
                let hl = self.get_hl();
                let res = hl.wrapping_add(hl);
                self.f = (self.f & 0x80)
                    | if ((hl & 0x0FFF) << 1) & 0x1000 != 0 {
                        0x20
                    } else {
                        0
                    }
                    | if (hl as u32 * 2) > 0xFFFF { 0x10 } else { 0 };
                self.set_hl(res);
            }
            0x2A => {
                let addr = self.get_hl();
                self.a = mmu.read_byte(addr);
                self.set_hl(addr.wrapping_add(1));
            }
            0x2B => {
                let val = self.get_hl().wrapping_sub(1);
                self.set_hl(val);
            }
            0x2C => {
                let res = self.l.wrapping_add(1);
                self.f = (self.f & 0x10)
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.l & 0x0F) + 1 > 0x0F { 0x20 } else { 0 };
                self.l = res;
            }
            0x2D => {
                let res = self.l.wrapping_sub(1);
                self.f = (self.f & 0x10)
                    | 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if self.l & 0x0F == 0 { 0x20 } else { 0 };
                self.l = res;
            }
            0x2E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.l = val;
            }
            0x2F => {
                self.a ^= 0xFF;
                self.f = (self.f & 0x90) | 0x60;
            }
            0x27 => {
                let mut correction = 0u8;
                let mut carry = false;
                if self.f & 0x20 != 0 || (self.f & 0x40 == 0 && (self.a & 0x0F) > 9) {
                    correction |= 0x06;
                }
                if self.f & 0x10 != 0 || (self.f & 0x40 == 0 && self.a > 0x99) {
                    correction |= 0x60;
                    carry = true;
                }
                if self.f & 0x40 == 0 {
                    self.a = self.a.wrapping_add(correction);
                } else {
                    self.a = self.a.wrapping_sub(correction);
                }
                self.f = if self.a == 0 { 0x80 } else { 0 }
                    | (self.f & 0x40)
                    | if carry { 0x10 } else { 0 };
            }
            0x30 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f & 0x10 == 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    extra_cycles = 4;
                }
            }
            0x31 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                self.sp = (hi << 8) | lo;
            }
            0x32 => {
                let addr = self.get_hl();
                mmu.write_byte(addr, self.a);
                self.set_hl(addr.wrapping_sub(1));
            }
            0x33 => {
                self.sp = self.sp.wrapping_add(1);
            }
            0x34 => {
                let addr = self.get_hl();
                let val = mmu.read_byte(addr).wrapping_add(1);
                let old = mmu.read_byte(addr);
                mmu.write_byte(addr, val);
                self.f = (self.f & 0x10)
                    | if val == 0 { 0x80 } else { 0 }
                    | if (old & 0x0F) + 1 > 0x0F { 0x20 } else { 0 };
            }
            0x35 => {
                let addr = self.get_hl();
                let old = mmu.read_byte(addr);
                let val = old.wrapping_sub(1);
                mmu.write_byte(addr, val);
                self.f = (self.f & 0x10)
                    | 0x40
                    | if val == 0 { 0x80 } else { 0 }
                    | if old & 0x0F == 0 { 0x20 } else { 0 };
            }
            0x36 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let addr = self.get_hl();
                mmu.write_byte(addr, val);
            }
            0x37 => {
                self.f = (self.f & 0x80) | 0x10;
            }
            0x38 => {
                let offset = mmu.read_byte(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f & 0x10 != 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    extra_cycles = 4;
                }
            }
            0x39 => {
                let hl = self.get_hl();
                let sp = self.sp;
                let res = hl.wrapping_add(sp);
                self.f = (self.f & 0x80)
                    | if ((hl & 0x0FFF) + (sp & 0x0FFF)) & 0x1000 != 0 {
                        0x20
                    } else {
                        0
                    }
                    | if (hl as u32 + sp as u32) > 0xFFFF {
                        0x10
                    } else {
                        0
                    };
                self.set_hl(res);
            }
            0x3A => {
                let addr = self.get_hl();
                self.a = mmu.read_byte(addr);
                self.set_hl(addr.wrapping_sub(1));
            }
            0x3B => {
                self.sp = self.sp.wrapping_sub(1);
            }
            0x3C => {
                let res = self.a.wrapping_add(1);
                self.f = (self.f & 0x10)
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) + 1 > 0x0F { 0x20 } else { 0 };
                self.a = res;
            }
            0x3D => {
                let res = self.a.wrapping_sub(1);
                self.f = (self.f & 0x10)
                    | 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if self.a & 0x0F == 0 { 0x20 } else { 0 };
                self.a = res;
            }
            0x3E => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a = val;
            }
            0x3F => {
                self.f = (self.f & 0x80) | if self.f & 0x10 != 0 { 0 } else { 0x10 };
            }
            opcode @ 0x40..=0x7F if opcode != 0x76 => {
                let dest = (opcode >> 3) & 0x07;
                let src = opcode & 0x07;
                let val = match src {
                    0 => self.b,
                    1 => self.c,
                    2 => self.d,
                    3 => self.e,
                    4 => self.h,
                    5 => self.l,
                    6 => mmu.read_byte(self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                match dest {
                    0 => self.b = val,
                    1 => self.c = val,
                    2 => self.d = val,
                    3 => self.e = val,
                    4 => self.h = val,
                    5 => self.l = val,
                    6 => {
                        let addr = self.get_hl();
                        mmu.write_byte(addr, val);
                    }
                    7 => self.a = val,
                    _ => unreachable!(),
                }
            }
            0x76 => {
                self.halted = true;
            }
            0x77 => {
                let addr = self.get_hl();
                mmu.write_byte(addr, self.a);
            }
            opcode @ 0x80..=0x87 => {
                let src = opcode & 0x07;
                let val = match src {
                    0 => self.b,
                    1 => self.c,
                    2 => self.d,
                    3 => self.e,
                    4 => self.h,
                    5 => self.l,
                    6 => mmu.read_byte(self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let (res, carry) = self.a.overflowing_add(val);
                self.f = if res == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) + (val & 0x0F) > 0x0F {
                        0x20
                    } else {
                        0
                    }
                    | if carry { 0x10 } else { 0 };
                self.a = res;
            }
            opcode @ 0x88..=0x8F => {
                let src = opcode & 0x07;
                let val = match src {
                    0 => self.b,
                    1 => self.c,
                    2 => self.d,
                    3 => self.e,
                    4 => self.h,
                    5 => self.l,
                    6 => mmu.read_byte(self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let carry_in = if self.f & 0x10 != 0 { 1 } else { 0 };
                let (res1, carry1) = self.a.overflowing_add(val);
                let (res2, carry2) = res1.overflowing_add(carry_in);
                self.f = if res2 == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) + (val & 0x0F) + carry_in > 0x0F {
                        0x20
                    } else {
                        0
                    }
                    | if carry1 || carry2 { 0x10 } else { 0 };
                self.a = res2;
            }
            opcode @ 0x90..=0x97 => {
                let src = opcode & 0x07;
                let val = match src {
                    0 => self.b,
                    1 => self.c,
                    2 => self.d,
                    3 => self.e,
                    4 => self.h,
                    5 => self.l,
                    6 => mmu.read_byte(self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let (res, borrow) = self.a.overflowing_sub(val);
                self.f = 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) {
                        0x20
                    } else {
                        0
                    }
                    | if borrow { 0x10 } else { 0 };
                self.a = res;
            }
            opcode @ 0x98..=0x9F => {
                let src = opcode & 0x07;
                let val = match src {
                    0 => self.b,
                    1 => self.c,
                    2 => self.d,
                    3 => self.e,
                    4 => self.h,
                    5 => self.l,
                    6 => mmu.read_byte(self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let carry_in = if self.f & 0x10 != 0 { 1 } else { 0 };
                let (res1, borrow1) = self.a.overflowing_sub(val);
                let (res2, borrow2) = res1.overflowing_sub(carry_in);
                self.f = 0x40
                    | if res2 == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) < ((val & 0x0F) + carry_in) {
                        0x20
                    } else {
                        0
                    }
                    | if borrow1 || borrow2 { 0x10 } else { 0 };
                self.a = res2;
            }
            opcode @ 0xA0..=0xA7 => {
                let src = opcode & 0x07;
                let val = match src {
                    0 => self.b,
                    1 => self.c,
                    2 => self.d,
                    3 => self.e,
                    4 => self.h,
                    5 => self.l,
                    6 => mmu.read_byte(self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                self.a &= val;
                self.f = if self.a == 0 { 0x80 } else { 0 } | 0x20;
            }
            opcode @ 0xA8..=0xAE => {
                let src = opcode & 0x07;
                let val = match src {
                    0 => self.b,
                    1 => self.c,
                    2 => self.d,
                    3 => self.e,
                    4 => self.h,
                    5 => self.l,
                    6 => mmu.read_byte(self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                self.a ^= val;
                self.f = if self.a == 0 { 0x80 } else { 0 };
            }
            opcode @ 0xB0..=0xB7 => {
                let src = opcode & 0x07;
                let val = match src {
                    0 => self.b,
                    1 => self.c,
                    2 => self.d,
                    3 => self.e,
                    4 => self.h,
                    5 => self.l,
                    6 => mmu.read_byte(self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                self.a |= val;
                self.f = if self.a == 0 { 0x80 } else { 0 };
            }
            opcode @ 0xB8..=0xBF => {
                let src = opcode & 0x07;
                let val = match src {
                    0 => self.b,
                    1 => self.c,
                    2 => self.d,
                    3 => self.e,
                    4 => self.h,
                    5 => self.l,
                    6 => mmu.read_byte(self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let res = self.a.wrapping_sub(val);
                self.f = 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) {
                        0x20
                    } else {
                        0
                    }
                    | if self.a < val { 0x10 } else { 0 };
            }
            0xAF => {
                self.a ^= self.a;
                // XOR A resets NF, HF, CF and sets Z
                self.f = 0x80;
            }
            0xC0 => {
                if self.f & 0x80 == 0 {
                    self.pc = self.pop_stack(mmu);
                    extra_cycles = 12;
                }
            }
            0xC1 => {
                let val = self.pop_stack(mmu);
                self.set_bc(val);
            }
            0xC5 => {
                let val = self.get_bc();
                self.push_stack(mmu, val);
            }
            0xC2 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                if self.f & 0x80 == 0 {
                    self.pc = (hi << 8) | lo;
                    extra_cycles = 4;
                }
            }
            0xC3 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = (hi << 8) | lo;
            }
            0xCA => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                if self.f & 0x80 != 0 {
                    self.pc = (hi << 8) | lo;
                    extra_cycles = 4;
                }
            }
            0xC4 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                if self.f & 0x80 == 0 {
                    self.push_stack(mmu, self.pc);
                    self.pc = (hi << 8) | lo;
                    extra_cycles = 12;
                }
            }
            0xC8 => {
                if self.f & 0x80 != 0 {
                    self.pc = self.pop_stack(mmu);
                    extra_cycles = 12;
                }
            }
            0xC6 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let (res, carry) = self.a.overflowing_add(val);
                self.f = if res == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) + (val & 0x0F) > 0x0F {
                        0x20
                    } else {
                        0
                    }
                    | if carry { 0x10 } else { 0 };
                self.a = res;
            }
            0xC9 => {
                self.pc = self.pop_stack(mmu);
            }
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                let target = match opcode {
                    0xC7 => 0x00,
                    0xCF => 0x08,
                    0xD7 => 0x10,
                    0xDF => 0x18,
                    0xE7 => 0x20,
                    0xEF => 0x28,
                    0xF7 => 0x30,
                    0xFF => 0x38,
                    _ => unreachable!(),
                };
                self.push_stack(mmu, self.pc);
                self.pc = target;
            }
            0xCD => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                let addr = (hi << 8) | lo;
                self.push_stack(mmu, self.pc);
                self.pc = addr;
            }
            0xCC => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                if self.f & 0x80 != 0 {
                    self.push_stack(mmu, self.pc);
                    self.pc = (hi << 8) | lo;
                    extra_cycles = 12;
                }
            }
            0xCE => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let carry_in = if self.f & 0x10 != 0 { 1 } else { 0 };
                let (res1, carry1) = self.a.overflowing_add(val);
                let (res2, carry2) = res1.overflowing_add(carry_in);
                self.f = if res2 == 0 { 0x80 } else { 0 }
                    | if ((self.a & 0x0F) + (val & 0x0F) + carry_in) > 0x0F {
                        0x20
                    } else {
                        0
                    }
                    | if carry1 || carry2 { 0x10 } else { 0 };
                self.a = res2;
            }
            0xD2 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                if self.f & 0x10 == 0 {
                    self.pc = (hi << 8) | lo;
                    extra_cycles = 4;
                }
            }
            0xD0 => {
                if self.f & 0x10 == 0 {
                    self.pc = self.pop_stack(mmu);
                    extra_cycles = 12;
                }
            }
            0xD1 => {
                let val = self.pop_stack(mmu);
                self.set_de(val);
            }
            0xD4 => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                if self.f & 0x10 == 0 {
                    self.push_stack(mmu, self.pc);
                    self.pc = (hi << 8) | lo;
                    extra_cycles = 12;
                }
            }
            0xD5 => {
                let val = self.get_de();
                self.push_stack(mmu, val);
            }
            0xD6 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let (res, borrow) = self.a.overflowing_sub(val);
                self.f = 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) {
                        0x20
                    } else {
                        0
                    }
                    | if borrow { 0x10 } else { 0 };
                self.a = res;
            }
            0xD8 => {
                if self.f & 0x10 != 0 {
                    self.pc = self.pop_stack(mmu);
                    extra_cycles = 12;
                }
            }
            0xD9 => {
                self.pc = self.pop_stack(mmu);
                self.ime = true;
            }
            0xDA => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                if self.f & 0x10 != 0 {
                    self.pc = (hi << 8) | lo;
                    extra_cycles = 4;
                }
            }
            0xDC => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                if self.f & 0x10 != 0 {
                    self.push_stack(mmu, self.pc);
                    self.pc = (hi << 8) | lo;
                    extra_cycles = 12;
                }
            }
            0xDE => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let carry_in = if self.f & 0x10 != 0 { 1 } else { 0 };
                let (res1, borrow1) = self.a.overflowing_sub(val);
                let (res2, borrow2) = res1.overflowing_sub(carry_in);
                self.f = 0x40
                    | if res2 == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) + carry_in {
                        0x20
                    } else {
                        0
                    }
                    | if borrow1 || borrow2 { 0x10 } else { 0 };
                self.a = res2;
            }
            0xE0 => {
                let offset = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let addr = 0xFF00u16 | offset as u16;
                mmu.write_byte(addr, self.a);
            }
            0xE2 => {
                let addr = 0xFF00u16 | self.c as u16;
                mmu.write_byte(addr, self.a);
            }
            0xE1 => {
                let val = self.pop_stack(mmu);
                self.set_hl(val);
            }
            0xE5 => {
                let val = self.get_hl();
                self.push_stack(mmu, val);
            }
            0xE6 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a &= val;
                self.f = if self.a == 0 { 0x80 } else { 0 } | 0x20;
            }
            0xE8 => {
                let val = mmu.read_byte(self.pc) as i8 as i16 as u16;
                self.pc = self.pc.wrapping_add(1);
                let sp = self.sp;
                let result = sp.wrapping_add(val);
                self.f = if ((sp & 0xF) + (val & 0xF)) > 0xF {
                    0x20
                } else {
                    0
                } | if ((sp & 0xFF) + (val & 0xFF)) > 0xFF {
                    0x10
                } else {
                    0
                };
                self.sp = result;
            }
            0xE9 => {
                self.pc = self.get_hl();
            }
            0xEA => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                let addr = (hi << 8) | lo;
                mmu.write_byte(addr, self.a);
            }
            0xF0 => {
                let offset = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let addr = 0xFF00u16 | offset as u16;
                self.a = mmu.read_byte(addr);
            }
            0xF2 => {
                let addr = 0xFF00u16 | self.c as u16;
                self.a = mmu.read_byte(addr);
            }
            0xF1 => {
                let val = self.pop_stack(mmu);
                self.a = (val >> 8) as u8;
                self.f = (val as u8) & 0xF0;
            }
            0xF5 => {
                let val = ((self.a as u16) << 8) | (self.f as u16 & 0xF0);
                self.push_stack(mmu, val);
            }
            0xEE => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a ^= val;
                self.f = if self.a == 0 { 0x80 } else { 0 };
            }
            0xF3 => {
                self.ime = false;
            }
            0xF6 => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a |= val;
                self.f = if self.a == 0 { 0x80 } else { 0 };
            }
            0xF8 => {
                let val = mmu.read_byte(self.pc) as i8 as i16 as u16;
                self.pc = self.pc.wrapping_add(1);
                let sp = self.sp;
                let res = sp.wrapping_add(val);
                self.f = if ((sp & 0xF) + (val & 0xF)) > 0xF {
                    0x20
                } else {
                    0
                } | if ((sp & 0xFF) + (val & 0xFF)) > 0xFF {
                    0x10
                } else {
                    0
                };
                self.set_hl(res);
            }
            0xF9 => {
                self.sp = self.get_hl();
            }
            0xFA => {
                let lo = mmu.read_byte(self.pc) as u16;
                let hi = mmu.read_byte(self.pc.wrapping_add(1)) as u16;
                self.pc = self.pc.wrapping_add(2);
                let addr = (hi << 8) | lo;
                self.a = mmu.read_byte(addr);
            }
            0xFB => {
                self.ime_delay = true;
            }
            0xFE => {
                let val = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let res = self.a.wrapping_sub(val);
                self.f = 0x40
                    | if res == 0 { 0x80 } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) {
                        0x20
                    } else {
                        0
                    }
                    | if self.a < val { 0x10 } else { 0 };
            }
            0xCB => {
                let op = mmu.read_byte(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.handle_cb(op, mmu);
                extra_cycles = CB_CYCLES[op as usize];
            }
            _ => panic!("unhandled opcode {:02X}", opcode),
        }

        let cycles = OPCODE_CYCLES[opcode as usize] as u16 + extra_cycles as u16;
        self.cycles += cycles as u64;
        mmu.timer.step(cycles, &mut mmu.if_reg);

        if enable_after {
            self.ime = true;
            self.ime_delay = false;
        }
        self.handle_interrupts(mmu);
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}
