// CPU flag bits as documented in gbdev.io/pandocs/The_CPU_Flags.html
const FLAG_Z: u8 = 0x80; // Zero
const FLAG_N: u8 = 0x40; // Subtract
const FLAG_H: u8 = 0x20; // Half Carry
const FLAG_C: u8 = 0x10; // Carry

// Interrupt vectors (gbdev.io/pandocs/Interrupts.html)
const INTERRUPT_VBLANK: u16 = 0x40;
const INTERRUPT_STAT: u16 = 0x48;
const INTERRUPT_TIMER: u16 = 0x50;
const INTERRUPT_SERIAL: u16 = 0x58;
const INTERRUPT_JOYPAD: u16 = 0x60;

// Post-boot CPU state from gbdev.io/pandocs/Power_Up_State.html
const BOOT_PC: u16 = 0x0100;
const BOOT_SP: u16 = 0xFFFE;

const DMG_BOOT_A: u8 = 0x01;
const DMG_BOOT_F: u8 = 0xB0;
const DMG_BOOT_B: u8 = 0x00;
const DMG_BOOT_C: u8 = 0x13;
const DMG_BOOT_D: u8 = 0x00;
const DMG_BOOT_E: u8 = 0xD8;
const DMG_BOOT_H: u8 = 0x01;
const DMG_BOOT_L: u8 = 0x4D;

const CGB_BOOT_A: u8 = 0x11;
const CGB_BOOT_F: u8 = 0x80;
const CGB_BOOT_B: u8 = 0x00;
const CGB_BOOT_C: u8 = 0x00;
const CGB_BOOT_D: u8 = 0xFF;
const CGB_BOOT_E: u8 = 0x56;
const CGB_BOOT_H: u8 = 0x00;
const CGB_BOOT_L: u8 = 0x0D;

// Clock ratios per machine cycle
const CYCLES_PER_M_CYCLE: u16 = 4; // normal speed
const CYCLES_PER_M_CYCLE_DOUBLE: u16 = 2; // double-speed mode

// DMA step durations (gbdev.io/pandocs/OAM_DMA_Transfer.html)
const OAM_DMA_STEP_CYCLES: u8 = 4;
const GDMA_STEP_CYCLES: u8 = 1;

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
    pub stopped: bool,
    pub double_speed: bool,
    halt_bug: bool,
    ime_delay: bool,
}

impl Cpu {
    pub fn new() -> Self {
        Self::new_with_mode(false)
    }

    /// Create a CPU initialized to the post-boot register state for the
    /// selected hardware mode.
    pub fn new_with_mode(cgb: bool) -> Self {
        if cgb {
            Self {
                a: CGB_BOOT_A,
                f: CGB_BOOT_F,
                b: CGB_BOOT_B,
                c: CGB_BOOT_C,
                d: CGB_BOOT_D,
                e: CGB_BOOT_E,
                h: CGB_BOOT_H,
                l: CGB_BOOT_L,
                pc: BOOT_PC,
                sp: BOOT_SP,
                cycles: 0,
                ime: false,
                halted: false,
                stopped: false,
                double_speed: false,
                halt_bug: false,
                ime_delay: false,
            }
        } else {
            Self {
                a: DMG_BOOT_A,
                f: DMG_BOOT_F,
                b: DMG_BOOT_B,
                c: DMG_BOOT_C,
                d: DMG_BOOT_D,
                e: DMG_BOOT_E,
                h: DMG_BOOT_H,
                l: DMG_BOOT_L,
                pc: BOOT_PC,
                sp: BOOT_SP,
                cycles: 0,
                ime: false,
                halted: false,
                stopped: false,
                double_speed: false,
                halt_bug: false,
                ime_delay: false,
            }
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

    #[inline]
    fn tick(&mut self, mmu: &mut crate::mmu::Mmu, m_cycles: u8) {
        let hw_cycles = if self.double_speed {
            CYCLES_PER_M_CYCLE_DOUBLE
        } else {
            CYCLES_PER_M_CYCLE
        } * m_cycles as u16;
        self.cycles += hw_cycles as u64;
        let prev_div = mmu.timer.div;
        mmu.timer.step(hw_cycles, &mut mmu.if_reg);
        let curr_div = mmu.timer.div;
        {
            let mut apu = mmu.apu.lock().unwrap();
            apu.tick(prev_div, curr_div, self.double_speed);
            apu.step(hw_cycles);
        }
        if mmu.ppu.step(hw_cycles, &mut mmu.if_reg) {
            mmu.hdma_hblank_transfer();
        }
        mmu.dma_step(hw_cycles);
    }

    #[inline(always)]
    fn fetch8(&mut self, mmu: &mut crate::mmu::Mmu) -> u8 {
        let val = mmu.read_byte(self.pc);
        self.pc = self.pc.wrapping_add(1);
        self.tick(mmu, 1);
        val
    }

    #[inline(always)]
    fn fetch16(&mut self, mmu: &mut crate::mmu::Mmu) -> u16 {
        let lo = self.fetch8(mmu) as u16;
        let hi = self.fetch8(mmu) as u16;
        (hi << 8) | lo
    }

    #[inline(always)]
    fn read8(&mut self, mmu: &mut crate::mmu::Mmu, addr: u16) -> u8 {
        let val = mmu.read_byte(addr);
        self.tick(mmu, 1);
        val
    }

    #[inline(always)]
    fn read16(&mut self, mmu: &mut crate::mmu::Mmu, addr: u16) -> u16 {
        let lo = self.read8(mmu, addr) as u16;
        let hi = self.read8(mmu, addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    #[inline(always)]
    fn write8(&mut self, mmu: &mut crate::mmu::Mmu, addr: u16, val: u8) {
        mmu.write_byte(addr, val);
        self.tick(mmu, 1);
    }

    #[inline(always)]
    fn write16(&mut self, mmu: &mut crate::mmu::Mmu, addr: u16, val: u16) {
        self.write8(mmu, addr, val as u8);
        self.write8(mmu, addr.wrapping_add(1), (val >> 8) as u8);
    }

    /// Return a formatted string of the current CPU state for debugging.
    pub fn debug_state(&self) -> String {
        format!(
            "AF:{:04X} BC:{:04X} DE:{:04X} HL:{:04X} PC:{:04X} SP:{:04X} CY:{}",
            ((self.a as u16) << 8) | self.f as u16,
            ((self.b as u16) << 8) | self.c as u16,
            ((self.d as u16) << 8) | self.e as u16,
            self.get_hl(),
            self.pc,
            self.sp,
            self.cycles
        )
    }

    fn push_stack(&mut self, mmu: &mut crate::mmu::Mmu, val: u16) {
        self.sp = self.sp.wrapping_sub(1);
        self.write8(mmu, self.sp, (val >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        self.write8(mmu, self.sp, val as u8);
    }

    fn pop_stack(&mut self, mmu: &mut crate::mmu::Mmu) -> u16 {
        let lo = self.read8(mmu, self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        let hi = self.read8(mmu, self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        (hi << 8) | lo
    }

    fn read_reg(&mut self, mmu: &mut crate::mmu::Mmu, index: u8) -> u8 {
        match index {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => self.read8(mmu, self.get_hl()),
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
                self.write8(mmu, addr, val);
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
                self.f =
                    if res == 0 { FLAG_Z } else { 0 } | if val & 0x80 != 0 { FLAG_C } else { 0 };
            }
            0x08..=0x0F => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = val.rotate_right(1);
                self.write_reg(mmu, r, res);
                self.f =
                    if res == 0 { FLAG_Z } else { 0 } | if val & 0x01 != 0 { FLAG_C } else { 0 };
            }
            0x10..=0x17 => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let carry_in = if self.f & FLAG_C != 0 { 1 } else { 0 };
                let res = (val << 1) | carry_in;
                self.write_reg(mmu, r, res);
                self.f =
                    if res == 0 { FLAG_Z } else { 0 } | if val & 0x80 != 0 { FLAG_C } else { 0 };
            }
            0x18..=0x1F => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let carry_in = if self.f & FLAG_C != 0 { 1 } else { 0 };
                let res = (val >> 1) | ((carry_in as u8) << 7);
                self.write_reg(mmu, r, res);
                self.f =
                    if res == 0 { FLAG_Z } else { 0 } | if val & 0x01 != 0 { FLAG_C } else { 0 };
            }
            0x20..=0x27 => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = val << 1;
                self.write_reg(mmu, r, res);
                self.f =
                    if res == 0 { FLAG_Z } else { 0 } | if val & 0x80 != 0 { FLAG_C } else { 0 };
            }
            0x28..=0x2F => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = (val >> 1) | (val & 0x80);
                self.write_reg(mmu, r, res);
                self.f =
                    if res == 0 { FLAG_Z } else { 0 } | if val & 0x01 != 0 { FLAG_C } else { 0 };
            }
            0x30..=0x37 => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = val.rotate_left(4);
                self.write_reg(mmu, r, res);
                self.f = if res == 0 { FLAG_Z } else { 0 };
            }
            0x38..=0x3F => {
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                let res = val >> 1;
                self.write_reg(mmu, r, res);
                self.f =
                    if res == 0 { FLAG_Z } else { 0 } | if val & 0x01 != 0 { FLAG_C } else { 0 };
            }
            0x40..=0x7F => {
                let bit = (opcode - 0x40) >> 3;
                let r = opcode & 0x07;
                let val = self.read_reg(mmu, r);
                self.f =
                    (self.f & FLAG_C) | FLAG_H | if val & (1 << bit) == 0 { FLAG_Z } else { 0 };
                if r == 6 {
                    // BIT (HL) only reads from memory; total timing is 12 cycles
                }
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
                INTERRUPT_VBLANK
            } else if pending & 0x02 != 0 {
                mmu.if_reg &= !0x02;
                INTERRUPT_STAT
            } else if pending & 0x04 != 0 {
                mmu.if_reg &= !0x04;
                INTERRUPT_TIMER
            } else if pending & 0x08 != 0 {
                mmu.if_reg &= !0x08;
                INTERRUPT_SERIAL
            } else {
                mmu.if_reg &= !0x10;
                INTERRUPT_JOYPAD
            };

            let pc = self.pc;
            self.push_stack(mmu, pc);
            self.pc = vector;
            self.ime = false;
            self.tick(mmu, 3);
        } else if self.halted {
            self.halted = false;
        }
    }

    pub fn step(&mut self, mmu: &mut crate::mmu::Mmu) {
        if self.stopped {
            return;
        }
        if mmu.gdma_active() {
            mmu.gdma_step(GDMA_STEP_CYCLES.into());
            self.tick(mmu, 1);
            return;
        }

        if self.halted {
            self.tick(mmu, 1);
            self.handle_interrupts(mmu);
            return;
        }

        let enable_after = self.ime_delay;
        let opcode = if self.halt_bug {
            self.halt_bug = false;
            self.read8(mmu, self.pc)
        } else {
            self.fetch8(mmu)
        };
        match opcode {
            0x00 => {}
            0x01 => {
                let val = self.fetch16(mmu);
                self.set_bc(val);
            }
            0x02 => {
                let addr = self.get_bc();
                self.write8(mmu, addr, self.a);
            }
            0x03 => {
                let val = self.get_bc().wrapping_add(1);
                self.set_bc(val);
                self.tick(mmu, 1);
            }
            0x04 => {
                let res = self.b.wrapping_add(1);
                self.f = (self.f & FLAG_C)
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.b & 0x0F) + 1 > 0x0F {
                        FLAG_H
                    } else {
                        0
                    };
                self.b = res;
            }
            0x05 => {
                let res = self.b.wrapping_sub(1);
                self.f = (self.f & FLAG_C)
                    | FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if self.b & 0x0F == 0 { FLAG_H } else { 0 };
                self.b = res;
            }
            0x06 => {
                let val = self.fetch8(mmu);
                self.b = val;
            }
            0x07 => {
                let carry = (self.a & 0x80) != 0;
                self.a = self.a.rotate_left(1);
                self.f = if carry { FLAG_C } else { 0 };
            }
            0x08 => {
                let addr = self.fetch16(mmu);
                self.write8(mmu, addr, (self.sp & 0xFF) as u8);
                self.write8(mmu, addr.wrapping_add(1), (self.sp >> 8) as u8);
            }
            0x09 => {
                let hl = self.get_hl();
                let bc = self.get_bc();
                let res = hl.wrapping_add(bc);
                self.f = (self.f & FLAG_Z)
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
                self.tick(mmu, 1);
            }
            0x0A => {
                let addr = self.get_bc();
                self.a = self.read8(mmu, addr);
            }
            0x0B => {
                let val = self.get_bc().wrapping_sub(1);
                self.set_bc(val);
                self.tick(mmu, 1);
            }
            0x0C => {
                let res = self.c.wrapping_add(1);
                self.f = (self.f & FLAG_C)
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.c & 0x0F) + 1 > 0x0F {
                        FLAG_H
                    } else {
                        0
                    };
                self.c = res;
            }
            0x0D => {
                let res = self.c.wrapping_sub(1);
                self.f = (self.f & FLAG_C)
                    | FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if self.c & 0x0F == 0 { FLAG_H } else { 0 };
                self.c = res;
            }
            0x0E => {
                let val = self.fetch8(mmu);
                self.c = val;
            }
            0x0F => {
                let carry = (self.a & 0x01) != 0;
                self.a = self.a.rotate_right(1);
                self.f = if carry { FLAG_C } else { 0 };
            }
            0x10 => {
                // STOP
                let _ = self.fetch8(mmu);
                mmu.timer.reset_div(&mut mmu.if_reg);
                if mmu.key1 & 0x01 != 0 {
                    mmu.key1 &= !0x01;
                    mmu.key1 ^= 0x80;
                    self.double_speed = mmu.key1 & 0x80 != 0;
                } else {
                    self.stopped = true;
                }
            }
            0x11 => {
                let val = self.fetch16(mmu);
                self.set_de(val);
            }
            0x12 => {
                let addr = self.get_de();
                self.write8(mmu, addr, self.a);
            }
            0x13 => {
                let val = self.get_de().wrapping_add(1);
                self.set_de(val);
                self.tick(mmu, 1);
            }
            0x14 => {
                let res = self.d.wrapping_add(1);
                self.f = (self.f & FLAG_C)
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.d & 0x0F) + 1 > 0x0F {
                        FLAG_H
                    } else {
                        0
                    };
                self.d = res;
            }
            0x15 => {
                let res = self.d.wrapping_sub(1);
                self.f = (self.f & FLAG_C)
                    | FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if self.d & 0x0F == 0 { FLAG_H } else { 0 };
                self.d = res;
            }
            0x16 => {
                let val = self.fetch8(mmu);
                self.d = val;
            }
            0x17 => {
                let carry = (self.a & 0x80) != 0;
                self.a = (self.a << 1) | if self.f & FLAG_C != 0 { 1 } else { 0 };
                self.f = if carry { FLAG_C } else { 0 };
            }
            0x18 => {
                let offset = self.fetch8(mmu) as i8;
                self.pc = self.pc.wrapping_add(offset as u16);
                self.tick(mmu, 1);
            }
            0x19 => {
                let hl = self.get_hl();
                let de = self.get_de();
                let res = hl.wrapping_add(de);
                self.f = (self.f & FLAG_Z)
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
                self.tick(mmu, 1);
            }
            0x1A => {
                let addr = self.get_de();
                self.a = self.read8(mmu, addr);
            }
            0x1B => {
                let val = self.get_de().wrapping_sub(1);
                self.set_de(val);
                self.tick(mmu, 1);
            }
            0x1C => {
                let res = self.e.wrapping_add(1);
                self.f = (self.f & FLAG_C)
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.e & 0x0F) + 1 > 0x0F {
                        FLAG_H
                    } else {
                        0
                    };
                self.e = res;
            }
            0x1D => {
                let res = self.e.wrapping_sub(1);
                self.f = (self.f & FLAG_C)
                    | FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if self.e & 0x0F == 0 { FLAG_H } else { 0 };
                self.e = res;
            }
            0x1E => {
                let val = self.fetch8(mmu);
                self.e = val;
            }
            0x1F => {
                let carry = (self.a & 0x01) != 0;
                self.a = (self.a >> 1) | if self.f & FLAG_C != 0 { FLAG_Z } else { 0 };
                self.f = if carry { FLAG_C } else { 0 };
            }
            0x20 => {
                let offset = self.fetch8(mmu) as i8;
                if self.f & FLAG_Z == 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    self.tick(mmu, 1);
                }
            }
            0x21 => {
                let val = self.fetch16(mmu);
                self.set_hl(val);
            }
            0x22 => {
                let addr = self.get_hl();
                self.write8(mmu, addr, self.a);
                self.set_hl(addr.wrapping_add(1));
            }
            0x23 => {
                let val = self.get_hl().wrapping_add(1);
                self.set_hl(val);
                self.tick(mmu, 1);
            }
            0x24 => {
                let res = self.h.wrapping_add(1);
                self.f = (self.f & FLAG_C)
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.h & 0x0F) + 1 > 0x0F {
                        FLAG_H
                    } else {
                        0
                    };
                self.h = res;
            }
            0x25 => {
                let res = self.h.wrapping_sub(1);
                self.f = (self.f & FLAG_C)
                    | FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if self.h & 0x0F == 0 { FLAG_H } else { 0 };
                self.h = res;
            }
            0x26 => {
                let val = self.fetch8(mmu);
                self.h = val;
            }
            0x27 => {
                let mut correction = 0u8;
                let mut carry = false;
                if self.f & FLAG_H != 0 || (self.f & FLAG_N == 0 && (self.a & 0x0F) > 9) {
                    correction |= 0x06;
                }
                if self.f & FLAG_C != 0 || (self.f & FLAG_N == 0 && self.a > 0x99) {
                    correction |= 0x60;
                    carry = true;
                }
                if self.f & FLAG_N == 0 {
                    self.a = self.a.wrapping_add(correction);
                } else {
                    self.a = self.a.wrapping_sub(correction);
                }
                self.f = if self.a == 0 { FLAG_Z } else { 0 }
                    | (self.f & FLAG_N)
                    | if carry { FLAG_C } else { 0 };
            }
            0x28 => {
                let offset = self.fetch8(mmu) as i8;
                if self.f & FLAG_Z != 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    self.tick(mmu, 1);
                }
            }
            0x29 => {
                let hl = self.get_hl();
                let res = hl.wrapping_add(hl);
                self.f = (self.f & FLAG_Z)
                    | if ((hl & 0x0FFF) << 1) & 0x1000 != 0 {
                        0x20
                    } else {
                        0
                    }
                    | if (hl as u32 * 2) > 0xFFFF { FLAG_C } else { 0 };
                self.set_hl(res);
                self.tick(mmu, 1);
            }
            0x2A => {
                let addr = self.get_hl();
                self.a = self.read8(mmu, addr);
                self.set_hl(addr.wrapping_add(1));
            }
            0x2B => {
                let val = self.get_hl().wrapping_sub(1);
                self.set_hl(val);
                self.tick(mmu, 1);
            }
            0x2C => {
                let res = self.l.wrapping_add(1);
                self.f = (self.f & FLAG_C)
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.l & 0x0F) + 1 > 0x0F {
                        FLAG_H
                    } else {
                        0
                    };
                self.l = res;
            }
            0x2D => {
                let res = self.l.wrapping_sub(1);
                self.f = (self.f & FLAG_C)
                    | FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if self.l & 0x0F == 0 { FLAG_H } else { 0 };
                self.l = res;
            }
            0x2E => {
                let val = self.fetch8(mmu);
                self.l = val;
            }
            0x2F => {
                self.a ^= 0xFF;
                self.f = (self.f & 0x90) | 0x60;
            }
            0x30 => {
                let offset = self.fetch8(mmu) as i8;
                if self.f & FLAG_C == 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    self.tick(mmu, 1);
                }
            }
            0x31 => {
                let val = self.fetch16(mmu);
                self.sp = val;
            }
            0x32 => {
                let addr = self.get_hl();
                self.write8(mmu, addr, self.a);
                self.set_hl(addr.wrapping_sub(1));
            }
            0x33 => {
                self.sp = self.sp.wrapping_add(1);
                self.tick(mmu, 1);
            }
            0x34 => {
                let addr = self.get_hl();
                let old = self.read8(mmu, addr);
                let val = old.wrapping_add(1);
                self.write8(mmu, addr, val);
                self.f = (self.f & FLAG_C)
                    | if val == 0 { FLAG_Z } else { 0 }
                    | if (old & 0x0F) + 1 > 0x0F { FLAG_H } else { 0 };
            }
            0x35 => {
                let addr = self.get_hl();
                let old = self.read8(mmu, addr);
                let val = old.wrapping_sub(1);
                self.write8(mmu, addr, val);
                self.f = (self.f & FLAG_C)
                    | FLAG_N
                    | if val == 0 { FLAG_Z } else { 0 }
                    | if old & 0x0F == 0 { FLAG_H } else { 0 };
            }
            0x36 => {
                let val = self.fetch8(mmu);
                let addr = self.get_hl();
                self.write8(mmu, addr, val);
            }
            0x37 => {
                self.f = (self.f & FLAG_Z) | FLAG_C;
            }
            0x38 => {
                let offset = self.fetch8(mmu) as i8;
                if self.f & FLAG_C != 0 {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    self.tick(mmu, 1);
                }
            }
            0x39 => {
                let hl = self.get_hl();
                let sp = self.sp;
                let res = hl.wrapping_add(sp);
                self.f = (self.f & FLAG_Z)
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
                self.tick(mmu, 1);
            }
            0x3A => {
                let addr = self.get_hl();
                self.a = self.read8(mmu, addr);
                self.set_hl(addr.wrapping_sub(1));
            }
            0x3B => {
                self.sp = self.sp.wrapping_sub(1);
                self.tick(mmu, 1);
            }
            0x3C => {
                let res = self.a.wrapping_add(1);
                self.f = (self.f & FLAG_C)
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) + 1 > 0x0F {
                        FLAG_H
                    } else {
                        0
                    };
                self.a = res;
            }
            0x3D => {
                let res = self.a.wrapping_sub(1);
                self.f = (self.f & FLAG_C)
                    | FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if self.a & 0x0F == 0 { FLAG_H } else { 0 };
                self.a = res;
            }
            0x3E => {
                let val = self.fetch8(mmu);
                self.a = val;
            }
            0x3F => {
                self.f = (self.f & FLAG_Z) | if self.f & FLAG_C != 0 { 0 } else { FLAG_C };
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
                    6 => self.read8(mmu, self.get_hl()),
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
                        self.write8(mmu, addr, val);
                    }
                    7 => self.a = val,
                    _ => unreachable!(),
                }
            }
            0x76 => {
                let pending = mmu.if_reg & mmu.ie_reg;
                if self.ime || pending == 0 {
                    self.halted = true;
                } else {
                    self.halt_bug = true;
                }
            }
            0x77 => {
                let addr = self.get_hl();
                self.write8(mmu, addr, self.a);
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
                    6 => self.read8(mmu, self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let (res, carry) = self.a.overflowing_add(val);
                self.f = if res == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) + (val & 0x0F) > 0x0F {
                        0x20
                    } else {
                        0
                    }
                    | if carry { FLAG_C } else { 0 };
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
                    6 => self.read8(mmu, self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let carry_in = if self.f & FLAG_C != 0 { 1 } else { 0 };
                let (res1, carry1) = self.a.overflowing_add(val);
                let (res2, carry2) = res1.overflowing_add(carry_in);
                self.f = if res2 == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) + (val & 0x0F) + carry_in > 0x0F {
                        0x20
                    } else {
                        0
                    }
                    | if carry1 || carry2 { FLAG_C } else { 0 };
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
                    6 => self.read8(mmu, self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let (res, borrow) = self.a.overflowing_sub(val);
                self.f = FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) {
                        0x20
                    } else {
                        0
                    }
                    | if borrow { FLAG_C } else { 0 };
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
                    6 => self.read8(mmu, self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let carry_in = if self.f & FLAG_C != 0 { 1 } else { 0 };
                let (res1, borrow1) = self.a.overflowing_sub(val);
                let (res2, borrow2) = res1.overflowing_sub(carry_in);
                self.f = FLAG_N
                    | if res2 == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) < ((val & 0x0F) + carry_in) {
                        0x20
                    } else {
                        0
                    }
                    | if borrow1 || borrow2 { FLAG_C } else { 0 };
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
                    6 => self.read8(mmu, self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                self.a &= val;
                self.f = if self.a == 0 { FLAG_Z } else { 0 } | FLAG_H;
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
                    6 => self.read8(mmu, self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                self.a ^= val;
                self.f = if self.a == 0 { FLAG_Z } else { 0 };
            }
            0xAF => {
                self.a ^= self.a;
                // XOR A resets NF, HF, CF and sets Z
                self.f = 0x80;
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
                    6 => self.read8(mmu, self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                self.a |= val;
                self.f = if self.a == 0 { FLAG_Z } else { 0 };
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
                    6 => self.read8(mmu, self.get_hl()),
                    7 => self.a,
                    _ => unreachable!(),
                };
                let res = self.a.wrapping_sub(val);
                self.f = FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) {
                        0x20
                    } else {
                        0
                    }
                    | if self.a < val { FLAG_C } else { 0 };
            }
            0xC0 => {
                if self.f & FLAG_Z == 0 {
                    self.tick(mmu, 1);
                    self.pc = self.pop_stack(mmu);
                    self.tick(mmu, 1);
                } else {
                    self.tick(mmu, 1);
                }
            }
            0xC1 => {
                let val = self.pop_stack(mmu);
                self.set_bc(val);
            }
            0xC2 => {
                let addr = self.fetch16(mmu);
                if self.f & FLAG_Z == 0 {
                    self.pc = addr;
                    self.tick(mmu, 1);
                }
            }
            0xC3 => {
                let addr = self.fetch16(mmu);
                self.pc = addr;
                self.tick(mmu, 1);
            }
            0xC4 => {
                let addr = self.fetch16(mmu);
                if self.f & FLAG_Z == 0 {
                    self.tick(mmu, 1);
                    self.push_stack(mmu, self.pc);
                    self.pc = addr;
                }
            }
            0xC5 => {
                let val = self.get_bc();
                self.tick(mmu, 1);
                self.push_stack(mmu, val);
            }
            0xC6 => {
                let val = self.fetch8(mmu);
                let (res, carry) = self.a.overflowing_add(val);
                self.f = if res == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) + (val & 0x0F) > 0x0F {
                        0x20
                    } else {
                        0
                    }
                    | if carry { FLAG_C } else { 0 };
                self.a = res;
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
                self.tick(mmu, 1);
                self.push_stack(mmu, self.pc);
                self.pc = target;
                //  self.tick(mmu, 1);
            }
            0xC8 => {
                if self.f & FLAG_Z != 0 {
                    self.tick(mmu, 1);
                    self.pc = self.pop_stack(mmu);
                    self.tick(mmu, 1);
                } else {
                    self.tick(mmu, 1);
                }
            }
            0xC9 => {
                self.pc = self.pop_stack(mmu);
                self.tick(mmu, 1);
            }
            0xCA => {
                let addr = self.fetch16(mmu);
                if self.f & FLAG_Z != 0 {
                    self.pc = addr;
                    self.tick(mmu, 1);
                }
            }
            0xCB => {
                let op = self.fetch8(mmu);
                self.handle_cb(op, mmu);
            }
            0xCC => {
                let addr = self.fetch16(mmu);
                if self.f & FLAG_Z != 0 {
                    self.tick(mmu, 1);
                    self.push_stack(mmu, self.pc);
                    self.pc = addr;
                }
            }
            0xCD => {
                let addr = self.fetch16(mmu);
                self.tick(mmu, 1);
                self.push_stack(mmu, self.pc);
                self.pc = addr;
            }
            0xCE => {
                let val = self.fetch8(mmu);
                let carry_in = if self.f & FLAG_C != 0 { 1 } else { 0 };
                let (res1, carry1) = self.a.overflowing_add(val);
                let (res2, carry2) = res1.overflowing_add(carry_in);
                self.f = if res2 == 0 { FLAG_Z } else { 0 }
                    | if ((self.a & 0x0F) + (val & 0x0F) + carry_in) > 0x0F {
                        0x20
                    } else {
                        0
                    }
                    | if carry1 || carry2 { FLAG_C } else { 0 };
                self.a = res2;
            }
            0xD0 => {
                if self.f & FLAG_C == 0 {
                    self.tick(mmu, 1);
                    self.pc = self.pop_stack(mmu);
                    self.tick(mmu, 1);
                } else {
                    self.tick(mmu, 1);
                }
            }
            0xD1 => {
                let val = self.pop_stack(mmu);
                self.set_de(val);
            }
            0xD2 => {
                let addr = self.fetch16(mmu);
                if self.f & FLAG_C == 0 {
                    self.pc = addr;
                    self.tick(mmu, 1);
                }
            }
            0xD4 => {
                let addr = self.fetch16(mmu);
                if self.f & FLAG_C == 0 {
                    self.tick(mmu, 1);
                    self.push_stack(mmu, self.pc);
                    self.pc = addr;
                }
            }
            0xD5 => {
                let val = self.get_de();
                self.tick(mmu, 1);
                self.push_stack(mmu, val);
            }
            0xD6 => {
                let val = self.fetch8(mmu);
                let (res, borrow) = self.a.overflowing_sub(val);
                self.f = FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) {
                        0x20
                    } else {
                        0
                    }
                    | if borrow { FLAG_C } else { 0 };
                self.a = res;
            }
            0xD8 => {
                if self.f & FLAG_C != 0 {
                    self.tick(mmu, 1);
                    self.pc = self.pop_stack(mmu);
                    self.tick(mmu, 1);
                } else {
                    self.tick(mmu, 1);
                }
            }
            0xD9 => {
                self.pc = self.pop_stack(mmu);
                self.ime = true;
                self.tick(mmu, 1);
            }
            0xDA => {
                let addr = self.fetch16(mmu);
                if self.f & FLAG_C != 0 {
                    self.pc = addr;
                    self.tick(mmu, 1);
                }
            }
            0xDC => {
                let addr = self.fetch16(mmu);
                if self.f & FLAG_C != 0 {
                    self.tick(mmu, 1);
                    self.push_stack(mmu, self.pc);
                    self.pc = addr;
                }
            }
            0xDE => {
                let val = self.fetch8(mmu);
                let carry_in = if self.f & FLAG_C != 0 { 1 } else { 0 };
                let (res1, borrow1) = self.a.overflowing_sub(val);
                let (res2, borrow2) = res1.overflowing_sub(carry_in);
                self.f = FLAG_N
                    | if res2 == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) + carry_in {
                        0x20
                    } else {
                        0
                    }
                    | if borrow1 || borrow2 { FLAG_C } else { 0 };
                self.a = res2;
            }
            0xE0 => {
                let offset = self.fetch8(mmu);
                let addr = 0xFF00u16 | offset as u16;
                self.write8(mmu, addr, self.a);
            }
            0xE1 => {
                let val = self.pop_stack(mmu);
                self.set_hl(val);
            }
            0xE2 => {
                let addr = 0xFF00u16 | self.c as u16;
                self.write8(mmu, addr, self.a);
            }
            0xE5 => {
                let val = self.get_hl();
                self.tick(mmu, 1);
                self.push_stack(mmu, val);
            }
            0xE6 => {
                let val = self.fetch8(mmu);
                self.a &= val;
                self.f = if self.a == 0 { FLAG_Z } else { 0 } | FLAG_H;
            }
            0xE8 => {
                let val = self.fetch8(mmu) as i8 as i16 as u16;
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
                self.tick(mmu, 2);
            }
            0xE9 => {
                self.pc = self.get_hl();
            }
            0xEA => {
                let addr = self.fetch16(mmu);
                self.write8(mmu, addr, self.a);
            }
            0xEE => {
                let val = self.fetch8(mmu);
                self.a ^= val;
                self.f = if self.a == 0 { FLAG_Z } else { 0 };
            }
            0xF0 => {
                let offset = self.fetch8(mmu);
                let addr = 0xFF00u16 | offset as u16;
                self.a = self.read8(mmu, addr);
            }
            0xF1 => {
                let val = self.pop_stack(mmu);
                self.a = (val >> 8) as u8;
                self.f = (val as u8) & 0xF0;
            }
            0xF2 => {
                let addr = 0xFF00u16 | self.c as u16;
                self.a = self.read8(mmu, addr);
            }
            0xF3 => {
                self.ime = false;
            }
            0xF5 => {
                let val = ((self.a as u16) << 8) | (self.f as u16 & 0xF0);
                self.tick(mmu, 1);
                self.push_stack(mmu, val);
            }
            0xF6 => {
                let val = self.fetch8(mmu);
                self.a |= val;
                self.f = if self.a == 0 { FLAG_Z } else { 0 };
            }
            0xF8 => {
                let val = self.fetch8(mmu) as i8 as i16 as u16;
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
                self.tick(mmu, 1);
            }
            0xF9 => {
                self.sp = self.get_hl();
                self.tick(mmu, 1);
            }
            0xFA => {
                let addr = self.fetch16(mmu);
                self.a = self.read8(mmu, addr);
            }
            0xFB => {
                self.ime_delay = true;
            }
            0xFE => {
                let val = self.fetch8(mmu);
                let res = self.a.wrapping_sub(val);
                self.f = FLAG_N
                    | if res == 0 { FLAG_Z } else { 0 }
                    | if (self.a & 0x0F) < (val & 0x0F) {
                        0x20
                    } else {
                        0
                    }
                    | if self.a < val { FLAG_C } else { 0 };
            }
            _ => panic!("unhandled opcode {opcode:02X}"),
        }

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
