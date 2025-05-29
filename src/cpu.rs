const ZERO_FLAG_BYTE_POSITION: u8 = 7;
const SUBTRACT_FLAG_BYTE_POSITION: u8 = 6;
const HALF_CARRY_FLAG_BYTE_POSITION: u8 = 5;
const CARRY_FLAG_BYTE_POSITION: u8 = 4;

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
    pub memory: [u8; 0xFFFF + 1],
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            a: 0x00,
            f: 0x00,
            b: 0x00,
            c: 0x00,
            d: 0x00,
            e: 0x00,
            h: 0x00,
            l: 0x00,
            pc: 0x0000,
            sp: 0x0000,
            memory: [0; 0xFFFF + 1],
        }
    }

    pub fn nop(&mut self) {
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn ld_bc_nn(&mut self, n_lo: u8, n_hi: u8) {
        self.b = n_hi;
        self.c = n_lo;
        self.pc = self.pc.wrapping_add(3);
    }

    // Renamed from ld_bc_a
    pub fn ld_bc_mem_a(&mut self) {
        let address = ((self.b as u16) << 8) | (self.c as u16);
        self.memory[address as usize] = self.a;
        self.pc = self.pc.wrapping_add(1);
    }

    // 8-bit Load Instructions

    // LD r, r'
    // LD A, r'
    pub fn ld_a_a(&mut self) { self.pc = self.pc.wrapping_add(1); }
    pub fn ld_a_b(&mut self) { self.a = self.b; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_a_c(&mut self) { self.a = self.c; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_a_d(&mut self) { self.a = self.d; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_a_e(&mut self) { self.a = self.e; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_a_h(&mut self) { self.a = self.h; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_a_l(&mut self) { self.a = self.l; self.pc = self.pc.wrapping_add(1); }

    // LD B, r'
    pub fn ld_b_a(&mut self) { self.b = self.a; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_b_b(&mut self) { self.pc = self.pc.wrapping_add(1); }
    pub fn ld_b_c(&mut self) { self.b = self.c; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_b_d(&mut self) { self.b = self.d; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_b_e(&mut self) { self.b = self.e; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_b_h(&mut self) { self.b = self.h; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_b_l(&mut self) { self.b = self.l; self.pc = self.pc.wrapping_add(1); }

    // LD C, r'
    pub fn ld_c_a(&mut self) { self.c = self.a; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_c_b(&mut self) { self.c = self.b; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_c_c(&mut self) { self.pc = self.pc.wrapping_add(1); }
    pub fn ld_c_d(&mut self) { self.c = self.d; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_c_e(&mut self) { self.c = self.e; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_c_h(&mut self) { self.c = self.h; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_c_l(&mut self) { self.c = self.l; self.pc = self.pc.wrapping_add(1); }

    // LD D, r'
    pub fn ld_d_a(&mut self) { self.d = self.a; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_d_b(&mut self) { self.d = self.b; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_d_c(&mut self) { self.d = self.c; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_d_d(&mut self) { self.pc = self.pc.wrapping_add(1); }
    pub fn ld_d_e(&mut self) { self.d = self.e; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_d_h(&mut self) { self.d = self.h; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_d_l(&mut self) { self.d = self.l; self.pc = self.pc.wrapping_add(1); }

    // LD E, r'
    pub fn ld_e_a(&mut self) { self.e = self.a; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_e_b(&mut self) { self.e = self.b; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_e_c(&mut self) { self.e = self.c; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_e_d(&mut self) { self.e = self.d; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_e_e(&mut self) { self.pc = self.pc.wrapping_add(1); }
    pub fn ld_e_h(&mut self) { self.e = self.h; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_e_l(&mut self) { self.e = self.l; self.pc = self.pc.wrapping_add(1); }

    // LD H, r'
    pub fn ld_h_a(&mut self) { self.h = self.a; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_h_b(&mut self) { self.h = self.b; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_h_c(&mut self) { self.h = self.c; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_h_d(&mut self) { self.h = self.d; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_h_e(&mut self) { self.h = self.e; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_h_h(&mut self) { self.pc = self.pc.wrapping_add(1); }
    pub fn ld_h_l(&mut self) { self.h = self.l; self.pc = self.pc.wrapping_add(1); }

    // LD L, r'
    pub fn ld_l_a(&mut self) { self.l = self.a; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_l_b(&mut self) { self.l = self.b; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_l_c(&mut self) { self.l = self.c; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_l_d(&mut self) { self.l = self.d; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_l_e(&mut self) { self.l = self.e; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_l_h(&mut self) { self.l = self.h; self.pc = self.pc.wrapping_add(1); }
    pub fn ld_l_l(&mut self) { self.pc = self.pc.wrapping_add(1); }

    // LD r, n
    pub fn ld_a_n(&mut self, value: u8) { self.a = value; self.pc = self.pc.wrapping_add(2); }
    pub fn ld_b_n(&mut self, value: u8) { self.b = value; self.pc = self.pc.wrapping_add(2); }
    pub fn ld_c_n(&mut self, value: u8) { self.c = value; self.pc = self.pc.wrapping_add(2); }
    pub fn ld_d_n(&mut self, value: u8) { self.d = value; self.pc = self.pc.wrapping_add(2); }
    pub fn ld_e_n(&mut self, value: u8) { self.e = value; self.pc = self.pc.wrapping_add(2); }
    pub fn ld_h_n(&mut self, value: u8) { self.h = value; self.pc = self.pc.wrapping_add(2); }
    pub fn ld_l_n(&mut self, value: u8) { self.l = value; self.pc = self.pc.wrapping_add(2); }

    // LD r, (HL)
    fn read_hl_mem(&self) -> u8 {
        let address = ((self.h as u16) << 8) | (self.l as u16);
        self.memory[address as usize]
    }
    pub fn ld_a_hl_mem(&mut self) { self.a = self.read_hl_mem(); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_b_hl_mem(&mut self) { self.b = self.read_hl_mem(); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_c_hl_mem(&mut self) { self.c = self.read_hl_mem(); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_d_hl_mem(&mut self) { self.d = self.read_hl_mem(); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_e_hl_mem(&mut self) { self.e = self.read_hl_mem(); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_h_hl_mem(&mut self) { self.h = self.read_hl_mem(); self.pc = self.pc.wrapping_add(1); } // H = (HL)
    pub fn ld_l_hl_mem(&mut self) { self.l = self.read_hl_mem(); self.pc = self.pc.wrapping_add(1); } // L = (HL)

    // LD (HL), r
    fn write_hl_mem(&mut self, value: u8) {
        let address = ((self.h as u16) << 8) | (self.l as u16);
        self.memory[address as usize] = value;
    }
    pub fn ld_hl_mem_a(&mut self) { self.write_hl_mem(self.a); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_hl_mem_b(&mut self) { self.write_hl_mem(self.b); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_hl_mem_c(&mut self) { self.write_hl_mem(self.c); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_hl_mem_d(&mut self) { self.write_hl_mem(self.d); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_hl_mem_e(&mut self) { self.write_hl_mem(self.e); self.pc = self.pc.wrapping_add(1); }
    pub fn ld_hl_mem_h(&mut self) { self.write_hl_mem(self.h); self.pc = self.pc.wrapping_add(1); } // (HL) = H
    pub fn ld_hl_mem_l(&mut self) { self.write_hl_mem(self.l); self.pc = self.pc.wrapping_add(1); } // (HL) = L
    
    // LD (HL), n
    pub fn ld_hl_mem_n(&mut self, value: u8) {
        self.write_hl_mem(value);
        self.pc = self.pc.wrapping_add(2);
    }

    // LD A, (BC) / LD A, (DE) / LD (DE), A
    pub fn ld_a_bc_mem(&mut self) {
        let address = ((self.b as u16) << 8) | (self.c as u16);
        self.a = self.memory[address as usize];
        self.pc = self.pc.wrapping_add(1);
    }
    pub fn ld_a_de_mem(&mut self) {
        let address = ((self.d as u16) << 8) | (self.e as u16);
        self.a = self.memory[address as usize];
        self.pc = self.pc.wrapping_add(1);
    }
    pub fn ld_de_mem_a(&mut self) {
        let address = ((self.d as u16) << 8) | (self.e as u16);
        self.memory[address as usize] = self.a;
        self.pc = self.pc.wrapping_add(1);
    }

    // LD A, (nn) / LD (nn), A
    pub fn ld_a_nn_mem(&mut self, addr_lo: u8, addr_hi: u8) {
        let address = ((addr_hi as u16) << 8) | (addr_lo as u16);
        self.a = self.memory[address as usize];
        self.pc = self.pc.wrapping_add(3);
    }
    pub fn ld_nn_mem_a(&mut self, addr_lo: u8, addr_hi: u8) {
        let address = ((addr_hi as u16) << 8) | (addr_lo as u16);
        self.memory[address as usize] = self.a;
        self.pc = self.pc.wrapping_add(3);
    }

    // LDH (High RAM loads)
    pub fn ldh_a_c_offset_mem(&mut self) {
        let address = 0xFF00 + self.c as u16;
        self.a = self.memory[address as usize];
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn ldh_c_offset_mem_a(&mut self) {
        let address = 0xFF00 + self.c as u16;
        self.memory[address as usize] = self.a;
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn ldh_a_n_offset_mem(&mut self, offset: u8) {
        let address = 0xFF00 + offset as u16;
        self.a = self.memory[address as usize];
        self.pc = self.pc.wrapping_add(2);
    }

    pub fn ldh_n_offset_mem_a(&mut self, offset: u8) {
        let address = 0xFF00 + offset as u16;
        self.memory[address as usize] = self.a;
        self.pc = self.pc.wrapping_add(2);
    }

    // LDI (Load with increment HL)
    pub fn ldi_hl_mem_a(&mut self) {
        let hl = ((self.h as u16) << 8) | (self.l as u16);
        self.memory[hl as usize] = self.a;
        let new_hl = hl.wrapping_add(1);
        self.h = (new_hl >> 8) as u8;
        self.l = new_hl as u8; // Correctly takes lower 8 bits
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn ldi_a_hl_mem(&mut self) {
        let hl = ((self.h as u16) << 8) | (self.l as u16);
        self.a = self.memory[hl as usize];
        let new_hl = hl.wrapping_add(1);
        self.h = (new_hl >> 8) as u8;
        self.l = new_hl as u8; // Correctly takes lower 8 bits
        self.pc = self.pc.wrapping_add(1);
    }

    // LDD (Load with decrement HL)
    pub fn ldd_hl_mem_a(&mut self) {
        let hl = ((self.h as u16) << 8) | (self.l as u16);
        self.memory[hl as usize] = self.a;
        let new_hl = hl.wrapping_sub(1);
        self.h = (new_hl >> 8) as u8;
        self.l = new_hl as u8; // Correctly takes lower 8 bits
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn ldd_a_hl_mem(&mut self) {
        let hl = ((self.h as u16) << 8) | (self.l as u16);
        self.a = self.memory[hl as usize];
        let new_hl = hl.wrapping_sub(1);
        self.h = (new_hl >> 8) as u8;
        self.l = new_hl as u8; // Correctly takes lower 8 bits
        self.pc = self.pc.wrapping_add(1);
    }

    // 16-bit Load Instructions

    // LD rr, nn
    // ld_bc_nn is already implemented
    pub fn ld_de_nn(&mut self, val_lo: u8, val_hi: u8) {
        self.d = val_hi;
        self.e = val_lo;
        self.pc = self.pc.wrapping_add(3);
    }

    pub fn ld_hl_nn(&mut self, val_lo: u8, val_hi: u8) {
        self.h = val_hi;
        self.l = val_lo;
        self.pc = self.pc.wrapping_add(3);
    }

    pub fn ld_sp_nn(&mut self, val_lo: u8, val_hi: u8) {
        self.sp = ((val_hi as u16) << 8) | (val_lo as u16);
        self.pc = self.pc.wrapping_add(3);
    }

    // LD SP, HL
    pub fn ld_sp_hl(&mut self) {
        self.sp = ((self.h as u16) << 8) | (self.l as u16);
        self.pc = self.pc.wrapping_add(1);
    }

    // LD (nn), SP
    pub fn ld_nn_mem_sp(&mut self, addr_lo: u8, addr_hi: u8) {
        let address = ((addr_hi as u16) << 8) | (addr_lo as u16);
        self.memory[address as usize] = (self.sp & 0xFF) as u8; // Store SP low byte
        self.memory[(address.wrapping_add(1)) as usize] = (self.sp >> 8) as u8; // Store SP high byte
        self.pc = self.pc.wrapping_add(3);
    }

    // PUSH rr
    pub fn push_bc(&mut self) {
        self.sp = self.sp.wrapping_sub(1);
        self.memory[self.sp as usize] = self.b;
        self.sp = self.sp.wrapping_sub(1);
        self.memory[self.sp as usize] = self.c;
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn push_de(&mut self) {
        self.sp = self.sp.wrapping_sub(1);
        self.memory[self.sp as usize] = self.d;
        self.sp = self.sp.wrapping_sub(1);
        self.memory[self.sp as usize] = self.e;
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn push_hl(&mut self) {
        self.sp = self.sp.wrapping_sub(1);
        self.memory[self.sp as usize] = self.h;
        self.sp = self.sp.wrapping_sub(1);
        self.memory[self.sp as usize] = self.l;
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn push_af(&mut self) {
        self.sp = self.sp.wrapping_sub(1);
        self.memory[self.sp as usize] = self.a;
        let f_val = self.f & 0xF0; // Ensure lower bits are zero before pushing
        self.sp = self.sp.wrapping_sub(1);
        self.memory[self.sp as usize] = f_val;
        self.pc = self.pc.wrapping_add(1);
    }

    // POP rr
    pub fn pop_bc(&mut self) {
        self.c = self.memory[self.sp as usize];
        self.sp = self.sp.wrapping_add(1);
        self.b = self.memory[self.sp as usize];
        self.sp = self.sp.wrapping_add(1);
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn pop_de(&mut self) {
        self.e = self.memory[self.sp as usize];
        self.sp = self.sp.wrapping_add(1);
        self.d = self.memory[self.sp as usize];
        self.sp = self.sp.wrapping_add(1);
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn pop_hl(&mut self) {
        self.l = self.memory[self.sp as usize];
        self.sp = self.sp.wrapping_add(1);
        self.h = self.memory[self.sp as usize];
        self.sp = self.sp.wrapping_add(1);
        self.pc = self.pc.wrapping_add(1);
    }

    pub fn pop_af(&mut self) {
        let f_val = self.memory[self.sp as usize];
        self.sp = self.sp.wrapping_add(1);
        self.a = self.memory[self.sp as usize];
        self.sp = self.sp.wrapping_add(1);
        self.f = f_val & 0xF0; // Ensure lower bits are zero after popping
        self.pc = self.pc.wrapping_add(1);
    }

    // 8-bit Arithmetic
    fn perform_add_a(&mut self, value: u8) {
        let original_a = self.a;
        let result = original_a.wrapping_add(value);
        self.set_flag_z(result == 0);
        self.set_flag_n(false); // N is 0 for ADD
        self.set_flag_h((original_a & 0x0F).wrapping_add(value & 0x0F) > 0x0F);
        self.set_flag_c(original_a as u16 + value as u16 > 0xFF);
        self.a = result;
    }

    // ADD A, r
    pub fn add_a_a(&mut self) { self.perform_add_a(self.a); self.pc = self.pc.wrapping_add(1); }
    pub fn add_a_b(&mut self) { self.perform_add_a(self.b); self.pc = self.pc.wrapping_add(1); }
    pub fn add_a_c(&mut self) { self.perform_add_a(self.c); self.pc = self.pc.wrapping_add(1); }
    pub fn add_a_d(&mut self) { self.perform_add_a(self.d); self.pc = self.pc.wrapping_add(1); }
    pub fn add_a_e(&mut self) { self.perform_add_a(self.e); self.pc = self.pc.wrapping_add(1); }
    pub fn add_a_h(&mut self) { self.perform_add_a(self.h); self.pc = self.pc.wrapping_add(1); }
    pub fn add_a_l(&mut self) { self.perform_add_a(self.l); self.pc = self.pc.wrapping_add(1); }

    // ADD A, n
    pub fn add_a_n(&mut self, value: u8) {
        self.perform_add_a(value);
        self.pc = self.pc.wrapping_add(2);
    }

    // ADD A, (HL)
    pub fn add_a_hl_mem(&mut self) {
        let value = self.read_hl_mem(); // Uses existing helper
        self.perform_add_a(value);
        self.pc = self.pc.wrapping_add(1);
    }

    fn perform_adc_a(&mut self, value: u8) {
        let original_a = self.a;
        let carry_in = if self.is_flag_c() { 1 } else { 0 };
        let result = original_a.wrapping_add(value).wrapping_add(carry_in);
        self.set_flag_z(result == 0);
        self.set_flag_n(false); // N is 0 for ADC
        self.set_flag_h((original_a & 0x0F).wrapping_add(value & 0x0F).wrapping_add(carry_in) > 0x0F);
        self.set_flag_c((original_a as u16).wrapping_add(value as u16).wrapping_add(carry_in as u16) > 0xFF);
        self.a = result;
    }

    // ADC A, r
    pub fn adc_a_a(&mut self) { self.perform_adc_a(self.a); self.pc = self.pc.wrapping_add(1); }
    pub fn adc_a_b(&mut self) { self.perform_adc_a(self.b); self.pc = self.pc.wrapping_add(1); }
    pub fn adc_a_c(&mut self) { self.perform_adc_a(self.c); self.pc = self.pc.wrapping_add(1); }
    pub fn adc_a_d(&mut self) { self.perform_adc_a(self.d); self.pc = self.pc.wrapping_add(1); }
    pub fn adc_a_e(&mut self) { self.perform_adc_a(self.e); self.pc = self.pc.wrapping_add(1); }
    pub fn adc_a_h(&mut self) { self.perform_adc_a(self.h); self.pc = self.pc.wrapping_add(1); }
    pub fn adc_a_l(&mut self) { self.perform_adc_a(self.l); self.pc = self.pc.wrapping_add(1); }

    // ADC A, n
    pub fn adc_a_n(&mut self, value: u8) {
        self.perform_adc_a(value);
        self.pc = self.pc.wrapping_add(2);
    }

    // ADC A, (HL)
    pub fn adc_a_hl_mem(&mut self) {
        let value = self.read_hl_mem();
        self.perform_adc_a(value);
        self.pc = self.pc.wrapping_add(1);
    }

    fn perform_sub_a(&mut self, value: u8) {
        let original_a = self.a;
        let result = original_a.wrapping_sub(value);
        self.set_flag_z(result == 0);
        self.set_flag_n(true); // N is 1 for SUB
        self.set_flag_h((original_a & 0x0F) < (value & 0x0F));
        self.set_flag_c(original_a < value);
        self.a = result;
    }

    // SUB A, r
    pub fn sub_a_a(&mut self) { self.perform_sub_a(self.a); self.pc = self.pc.wrapping_add(1); }
    pub fn sub_a_b(&mut self) { self.perform_sub_a(self.b); self.pc = self.pc.wrapping_add(1); }
    pub fn sub_a_c(&mut self) { self.perform_sub_a(self.c); self.pc = self.pc.wrapping_add(1); }
    pub fn sub_a_d(&mut self) { self.perform_sub_a(self.d); self.pc = self.pc.wrapping_add(1); }
    pub fn sub_a_e(&mut self) { self.perform_sub_a(self.e); self.pc = self.pc.wrapping_add(1); }
    pub fn sub_a_h(&mut self) { self.perform_sub_a(self.h); self.pc = self.pc.wrapping_add(1); }
    pub fn sub_a_l(&mut self) { self.perform_sub_a(self.l); self.pc = self.pc.wrapping_add(1); }

    // SUB A, n
    pub fn sub_a_n(&mut self, value: u8) {
        self.perform_sub_a(value);
        self.pc = self.pc.wrapping_add(2);
    }

    // SUB A, (HL)
    pub fn sub_a_hl_mem(&mut self) {
        let value = self.read_hl_mem();
        self.perform_sub_a(value);
        self.pc = self.pc.wrapping_add(1);
    }

    fn perform_sbc_a(&mut self, value: u8) {
        let original_a = self.a;
        let carry_val = if self.is_flag_c() { 1 } else { 0 };

        self.a = original_a.wrapping_sub(value).wrapping_sub(carry_val);

        self.set_flag_z(self.a == 0);
        self.set_flag_n(true);
        self.set_flag_h( (original_a & 0x0F) < (value & 0x0F) + carry_val );
        self.set_flag_c( (original_a as u16) < (value as u16) + (carry_val as u16) );
    }

    // SBC A, r
    pub fn sbc_a_a(&mut self) { self.perform_sbc_a(self.a); self.pc = self.pc.wrapping_add(1); }
    pub fn sbc_a_b(&mut self) { self.perform_sbc_a(self.b); self.pc = self.pc.wrapping_add(1); }
    pub fn sbc_a_c(&mut self) { self.perform_sbc_a(self.c); self.pc = self.pc.wrapping_add(1); }
    pub fn sbc_a_d(&mut self) { self.perform_sbc_a(self.d); self.pc = self.pc.wrapping_add(1); }
    pub fn sbc_a_e(&mut self) { self.perform_sbc_a(self.e); self.pc = self.pc.wrapping_add(1); }
    pub fn sbc_a_h(&mut self) { self.perform_sbc_a(self.h); self.pc = self.pc.wrapping_add(1); }
    pub fn sbc_a_l(&mut self) { self.perform_sbc_a(self.l); self.pc = self.pc.wrapping_add(1); }

    // SBC A, n
    pub fn sbc_a_n(&mut self, value: u8) {
        self.perform_sbc_a(value);
        self.pc = self.pc.wrapping_add(2);
    }

    // SBC A, (HL)
    pub fn sbc_a_hl_mem(&mut self) {
        let value = self.read_hl_mem();
        self.perform_sbc_a(value);
        self.pc = self.pc.wrapping_add(1);
    }

    fn perform_and_a(&mut self, value: u8) {
        self.a &= value;
        self.set_flag_z(self.a == 0);
        self.set_flag_n(false);
        self.set_flag_h(true);
        self.set_flag_c(false);
    }

    // AND A, r
    pub fn and_a_a(&mut self) { self.perform_and_a(self.a); self.pc = self.pc.wrapping_add(1); }
    pub fn and_a_b(&mut self) { self.perform_and_a(self.b); self.pc = self.pc.wrapping_add(1); }
    pub fn and_a_c(&mut self) { self.perform_and_a(self.c); self.pc = self.pc.wrapping_add(1); }
    pub fn and_a_d(&mut self) { self.perform_and_a(self.d); self.pc = self.pc.wrapping_add(1); }
    pub fn and_a_e(&mut self) { self.perform_and_a(self.e); self.pc = self.pc.wrapping_add(1); }
    pub fn and_a_h(&mut self) { self.perform_and_a(self.h); self.pc = self.pc.wrapping_add(1); }
    pub fn and_a_l(&mut self) { self.perform_and_a(self.l); self.pc = self.pc.wrapping_add(1); }

    // AND A, n
    pub fn and_a_n(&mut self, value: u8) {
        self.perform_and_a(value);
        self.pc = self.pc.wrapping_add(2);
    }

    // AND A, (HL)
    pub fn and_a_hl_mem(&mut self) {
        let value = self.read_hl_mem();
        self.perform_and_a(value);
        self.pc = self.pc.wrapping_add(1);
    }

    fn perform_or_a(&mut self, value: u8) {
        self.a |= value;
        self.set_flag_z(self.a == 0);
        self.set_flag_n(false);
        self.set_flag_h(false);
        self.set_flag_c(false);
    }

    // OR A, r
    pub fn or_a_a(&mut self) { self.perform_or_a(self.a); self.pc = self.pc.wrapping_add(1); }
    pub fn or_a_b(&mut self) { self.perform_or_a(self.b); self.pc = self.pc.wrapping_add(1); }
    pub fn or_a_c(&mut self) { self.perform_or_a(self.c); self.pc = self.pc.wrapping_add(1); }
    pub fn or_a_d(&mut self) { self.perform_or_a(self.d); self.pc = self.pc.wrapping_add(1); }
    pub fn or_a_e(&mut self) { self.perform_or_a(self.e); self.pc = self.pc.wrapping_add(1); }
    pub fn or_a_h(&mut self) { self.perform_or_a(self.h); self.pc = self.pc.wrapping_add(1); }
    pub fn or_a_l(&mut self) { self.perform_or_a(self.l); self.pc = self.pc.wrapping_add(1); }

    // OR A, n
    pub fn or_a_n(&mut self, value: u8) {
        self.perform_or_a(value);
        self.pc = self.pc.wrapping_add(2);
    }

    // OR A, (HL)
    pub fn or_a_hl_mem(&mut self) {
        let value = self.read_hl_mem();
        self.perform_or_a(value);
        self.pc = self.pc.wrapping_add(1);
    }

    fn perform_xor_a(&mut self, value: u8) {
        self.a ^= value;
        self.set_flag_z(self.a == 0);
        self.set_flag_n(false);
        self.set_flag_h(false);
        self.set_flag_c(false);
    }

    // XOR A, r
    pub fn xor_a_a(&mut self) { self.perform_xor_a(self.a); self.pc = self.pc.wrapping_add(1); }
    pub fn xor_a_b(&mut self) { self.perform_xor_a(self.b); self.pc = self.pc.wrapping_add(1); }
    pub fn xor_a_c(&mut self) { self.perform_xor_a(self.c); self.pc = self.pc.wrapping_add(1); }
    pub fn xor_a_d(&mut self) { self.perform_xor_a(self.d); self.pc = self.pc.wrapping_add(1); }
    pub fn xor_a_e(&mut self) { self.perform_xor_a(self.e); self.pc = self.pc.wrapping_add(1); }
    pub fn xor_a_h(&mut self) { self.perform_xor_a(self.h); self.pc = self.pc.wrapping_add(1); }
    pub fn xor_a_l(&mut self) { self.perform_xor_a(self.l); self.pc = self.pc.wrapping_add(1); }

    // XOR A, n
    pub fn xor_a_n(&mut self, value: u8) {
        self.perform_xor_a(value);
        self.pc = self.pc.wrapping_add(2);
    }

    // XOR A, (HL)
    pub fn xor_a_hl_mem(&mut self) {
        let value = self.read_hl_mem();
        self.perform_xor_a(value);
        self.pc = self.pc.wrapping_add(1);
    }

    fn perform_cp_a(&mut self, value: u8) {
        let original_a = self.a;
        //let _result = original_a.wrapping_sub(value); // Not needed as A is not modified
        self.set_flag_z(original_a == value);
        self.set_flag_n(true);
        self.set_flag_h((original_a & 0x0F) < (value & 0x0F));
        self.set_flag_c(original_a < value);
    }

    // CP A, r
    pub fn cp_a_a(&mut self) { self.perform_cp_a(self.a); self.pc = self.pc.wrapping_add(1); }
    pub fn cp_a_b(&mut self) { self.perform_cp_a(self.b); self.pc = self.pc.wrapping_add(1); }
    pub fn cp_a_c(&mut self) { self.perform_cp_a(self.c); self.pc = self.pc.wrapping_add(1); }
    pub fn cp_a_d(&mut self) { self.perform_cp_a(self.d); self.pc = self.pc.wrapping_add(1); }
    pub fn cp_a_e(&mut self) { self.perform_cp_a(self.e); self.pc = self.pc.wrapping_add(1); }
    pub fn cp_a_h(&mut self) { self.perform_cp_a(self.h); self.pc = self.pc.wrapping_add(1); }
    pub fn cp_a_l(&mut self) { self.perform_cp_a(self.l); self.pc = self.pc.wrapping_add(1); }

    // CP A, n
    pub fn cp_a_n(&mut self, value: u8) {
        self.perform_cp_a(value);
        self.pc = self.pc.wrapping_add(2);
    }

    // CP A, (HL)
    pub fn cp_a_hl_mem(&mut self) {
        let value = self.read_hl_mem();
        self.perform_cp_a(value);
        self.pc = self.pc.wrapping_add(1);
    }

    // Flag Setters
    pub fn set_flag_z(&mut self, value: bool) {
        if value {
            self.f |= 1 << ZERO_FLAG_BYTE_POSITION;
        } else {
            self.f &= !(1 << ZERO_FLAG_BYTE_POSITION);
        }
        self.f &= 0xF0; // Ensure unused bits are zero
    }

    pub fn set_flag_n(&mut self, value: bool) {
        if value {
            self.f |= 1 << SUBTRACT_FLAG_BYTE_POSITION;
        } else {
            self.f &= !(1 << SUBTRACT_FLAG_BYTE_POSITION);
        }
        self.f &= 0xF0; // Ensure unused bits are zero
    }

    pub fn set_flag_h(&mut self, value: bool) {
        if value {
            self.f |= 1 << HALF_CARRY_FLAG_BYTE_POSITION;
        } else {
            self.f &= !(1 << HALF_CARRY_FLAG_BYTE_POSITION);
        }
        self.f &= 0xF0; // Ensure unused bits are zero
    }

    pub fn set_flag_c(&mut self, value: bool) {
        if value {
            self.f |= 1 << CARRY_FLAG_BYTE_POSITION;
        } else {
            self.f &= !(1 << CARRY_FLAG_BYTE_POSITION);
        }
        self.f &= 0xF0; // Ensure unused bits are zero
    }

    // Flag Getters
    pub fn is_flag_z(&self) -> bool {
        (self.f >> ZERO_FLAG_BYTE_POSITION) & 1 != 0
    }

    pub fn is_flag_n(&self) -> bool {
        (self.f >> SUBTRACT_FLAG_BYTE_POSITION) & 1 != 0
    }

    pub fn is_flag_h(&self) -> bool {
        (self.f >> HALF_CARRY_FLAG_BYTE_POSITION) & 1 != 0
    }

    pub fn is_flag_c(&self) -> bool {
        (self.f >> CARRY_FLAG_BYTE_POSITION) & 1 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Imports Cpu, flag constants, etc.

    macro_rules! assert_flags {
        ($cpu:expr, $z:expr, $n:expr, $h:expr, $c:expr $(,)?) => {
            assert_eq!($cpu.is_flag_z(), $z, "Flag Z mismatch");
            assert_eq!($cpu.is_flag_n(), $n, "Flag N mismatch");
            assert_eq!($cpu.is_flag_h(), $h, "Flag H mismatch");
            assert_eq!($cpu.is_flag_c(), $c, "Flag C mismatch");
        };
    }

    fn setup_cpu() -> Cpu {
        Cpu::new() // Initializes PC and SP to 0x0000, memory to 0s.
    }

    mod initial_tests {
        use super::*;

        #[test]
        fn test_nop() {
            let mut cpu = setup_cpu();
            let initial_pc = cpu.pc;
            cpu.nop();
            assert_eq!(cpu.pc, initial_pc + 1, "PC should increment by 1 for NOP");
        }
    }

    mod loads_8bit_reg_reg { 
        use super::*;

        #[test]
        fn test_ld_a_b() {
            let mut cpu = setup_cpu();
            cpu.b = 0xAB;
            cpu.c = 0x12; // Control value to ensure it's not affected
            let initial_pc = cpu.pc;
        
            cpu.ld_a_b();
        
            assert_eq!(cpu.a, 0xAB, "A should be loaded with value from B");
            assert_eq!(cpu.b, 0xAB, "B should remain unchanged");
            assert_eq!(cpu.c, 0x12, "C should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_c_e() {
            let mut cpu = setup_cpu();
            cpu.e = 0xCD;
            cpu.d = 0x34; // Control
            let initial_pc = cpu.pc;

            cpu.ld_c_e();

            assert_eq!(cpu.c, 0xCD, "C should be loaded with value from E");
            assert_eq!(cpu.e, 0xCD, "E should remain unchanged");
            assert_eq!(cpu.d, 0x34, "D should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_h_l() {
            let mut cpu = setup_cpu();
            cpu.l = 0xFE;
            cpu.a = 0x56; // Control
            let initial_pc = cpu.pc;

            cpu.ld_h_l();

            assert_eq!(cpu.h, 0xFE, "H should be loaded with value from L");
            assert_eq!(cpu.l, 0xFE, "L should remain unchanged");
            assert_eq!(cpu.a, 0x56, "A should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_l_a() {
            let mut cpu = setup_cpu();
            cpu.a = 0x89;
            cpu.b = 0x7A; // Control
            let initial_pc = cpu.pc;

            cpu.ld_l_a();

            assert_eq!(cpu.l, 0x89, "L should be loaded with value from A");
            assert_eq!(cpu.a, 0x89, "A should remain unchanged");
            assert_eq!(cpu.b, 0x7A, "B should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_b_b() {
            let mut cpu = setup_cpu();
            cpu.b = 0x67;
            cpu.a = 0xFF; // Control
            let initial_pc = cpu.pc;

            cpu.ld_b_b(); // LD B, B

            assert_eq!(cpu.b, 0x67, "B should remain unchanged (LD B,B)");
            assert_eq!(cpu.a, 0xFF, "A should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_d_a() {
            let mut cpu = setup_cpu();
            cpu.a = 0x1F;
            cpu.e = 0x2E; // Control
            let initial_pc = cpu.pc;

            cpu.ld_d_a();

            assert_eq!(cpu.d, 0x1F, "D should be loaded with value from A");
            assert_eq!(cpu.a, 0x1F, "A should remain unchanged");
            assert_eq!(cpu.e, 0x2E, "E should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_e_b() {
            let mut cpu = setup_cpu();
            cpu.b = 0x9C;
            cpu.d = 0x8D; // Control
            let initial_pc = cpu.pc;

            cpu.ld_e_b();

            assert_eq!(cpu.e, 0x9C, "E should be loaded with value from B");
            assert_eq!(cpu.b, 0x9C, "B should remain unchanged");
            assert_eq!(cpu.d, 0x8D, "D should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }
    }
    mod loads_8bit_reg_n { 
        use super::*;

        #[test]
        fn test_ld_a_n() {
            let mut cpu = setup_cpu();
            let val_n = 0xCD;
            cpu.b = 0x12; // Control value
            let initial_pc = cpu.pc;
        
            cpu.ld_a_n(val_n);
        
            assert_eq!(cpu.a, val_n, "A should be loaded with immediate value n");
            assert_eq!(cpu.b, 0x12, "B should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_b_n() {
            let mut cpu = setup_cpu();
            let val_n = 0xAB;
            cpu.c = 0x34; // Control
            let initial_pc = cpu.pc;

            cpu.ld_b_n(val_n);

            assert_eq!(cpu.b, val_n, "B should be loaded with immediate value n");
            assert_eq!(cpu.c, 0x34, "C should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_c_n() {
            let mut cpu = setup_cpu();
            let val_n = 0x5F;
            cpu.d = 0xEA; // Control
            let initial_pc = cpu.pc;

            cpu.ld_c_n(val_n);

            assert_eq!(cpu.c, val_n, "C should be loaded with immediate value n");
            assert_eq!(cpu.d, 0xEA, "D should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_d_n() {
            let mut cpu = setup_cpu();
            let val_n = 0x23;
            cpu.e = 0x45; // Control
            let initial_pc = cpu.pc;

            cpu.ld_d_n(val_n);

            assert_eq!(cpu.d, val_n, "D should be loaded with immediate value n");
            assert_eq!(cpu.e, 0x45, "E should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_e_n() {
            let mut cpu = setup_cpu();
            let val_n = 0x77;
            cpu.h = 0x88; // Control
            let initial_pc = cpu.pc;

            cpu.ld_e_n(val_n);

            assert_eq!(cpu.e, val_n, "E should be loaded with immediate value n");
            assert_eq!(cpu.h, 0x88, "H should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_h_n() {
            let mut cpu = setup_cpu();
            let val_n = 0x99;
            cpu.l = 0xAA; // Control
            let initial_pc = cpu.pc;

            cpu.ld_h_n(val_n);

            assert_eq!(cpu.h, val_n, "H should be loaded with immediate value n");
            assert_eq!(cpu.l, 0xAA, "L should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_l_n() {
            let mut cpu = setup_cpu();
            let val_n = 0xBB;
            cpu.a = 0xCC; // Control
            let initial_pc = cpu.pc;

            cpu.ld_l_n(val_n);

            assert_eq!(cpu.l, val_n, "L should be loaded with immediate value n");
            assert_eq!(cpu.a, 0xCC, "A should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }
    }
    mod loads_8bit_hl_mem { 
        use super::*;

        #[test]
        fn test_ld_a_hl_mem() {
            let mut cpu = setup_cpu();
            let addr = 0x1234;
            cpu.h = (addr >> 8) as u8;
            cpu.l = (addr & 0xFF) as u8;
            cpu.memory[addr as usize] = 0xAB;
            cpu.b = 0xCD; // Control
            let initial_pc = cpu.pc;
    
            cpu.ld_a_hl_mem();
    
            assert_eq!(cpu.a, 0xAB, "A should be loaded from memory[HL]");
            assert_eq!(cpu.memory[addr as usize], 0xAB, "Memory should be unchanged");
            assert_eq!(cpu.b, 0xCD, "B should be unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_b_hl_mem() {
            let mut cpu = setup_cpu();
            let addr = 0xC001;
            cpu.h = (addr >> 8) as u8;
            cpu.l = (addr & 0xFF) as u8;
            cpu.memory[addr as usize] = 0x55;
            cpu.a = 0xFF; // Control
            let initial_pc = cpu.pc;

            cpu.ld_b_hl_mem();

            assert_eq!(cpu.b, 0x55, "B should be loaded from memory[HL]");
            assert_eq!(cpu.memory[addr as usize], 0x55, "Memory should be unchanged");
            assert_eq!(cpu.a, 0xFF, "A should be unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_l_hl_mem() {
            let mut cpu = setup_cpu();
            let addr = 0xD345;
            cpu.h = (addr >> 8) as u8;
            cpu.l = (addr & 0xFF) as u8; // L will be overwritten
            cpu.memory[addr as usize] = 0x22;
            cpu.a = 0xEE; // Control
            let initial_pc = cpu.pc;
    
            cpu.ld_l_hl_mem();
    
            assert_eq!(cpu.l, 0x22, "L should be loaded from memory[HL]");
            assert_eq!(cpu.memory[addr as usize], 0x22, "Memory should be unchanged");
            assert_eq!(cpu.a, 0xEE, "A should be unchanged");
            assert_eq!(cpu.h, (addr >> 8) as u8, "H should be unchanged"); // Ensure H is not touched
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_hl_mem_a() {
            let mut cpu = setup_cpu();
            let addr = 0x5678;
            cpu.h = (addr >> 8) as u8;
            cpu.l = (addr & 0xFF) as u8;
            cpu.a = 0xEF;
            cpu.b = 0x12; // Control
            let initial_pc = cpu.pc;
    
            cpu.ld_hl_mem_a();
    
            assert_eq!(cpu.memory[addr as usize], 0xEF, "memory[HL] should be loaded from A");
            assert_eq!(cpu.a, 0xEF, "A should be unchanged");
            assert_eq!(cpu.b, 0x12, "B should be unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_hl_mem_c() {
            let mut cpu = setup_cpu();
            let addr = 0x8888;
            cpu.h = (addr >> 8) as u8;
            cpu.l = (addr & 0xFF) as u8;
            cpu.c = 0x7C;
            cpu.d = 0xDD; // Control
            let initial_pc = cpu.pc;

            cpu.ld_hl_mem_c();

            assert_eq!(cpu.memory[addr as usize], 0x7C, "memory[HL] should be loaded from C");
            assert_eq!(cpu.c, 0x7C, "C should be unchanged");
            assert_eq!(cpu.d, 0xDD, "D should be unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }
        
        #[test]
        fn test_ld_hl_mem_h() { // Tests LD (HL), H
            let mut cpu = setup_cpu();
            let addr = 0x1F0A;
            cpu.h = (addr >> 8) as u8; // H is 0x1F
            cpu.l = (addr & 0xFF) as u8; // L is 0x0A
            // Value to store is cpu.h (0x1F)
            cpu.a = 0xBD; // Control
            let initial_pc = cpu.pc;

            cpu.ld_hl_mem_h();

            assert_eq!(cpu.memory[addr as usize], cpu.h, "memory[HL] should be loaded from H");
            assert_eq!(cpu.h, (addr >> 8) as u8, "H should be unchanged");
            assert_eq!(cpu.l, (addr & 0xFF) as u8, "L should be unchanged");
            assert_eq!(cpu.a, 0xBD, "A should be unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_hl_mem_n() {
            let mut cpu = setup_cpu();
            let addr = 0x2244;
            cpu.h = (addr >> 8) as u8;
            cpu.l = (addr & 0xFF) as u8;
            let val_n = 0x66;
            cpu.a = 0x12; // Control
            let initial_pc = cpu.pc;
    
            cpu.ld_hl_mem_n(val_n);
    
            assert_eq!(cpu.memory[addr as usize], val_n, "memory[HL] should be loaded with n");
            assert_eq!(cpu.a, 0x12, "A should be unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }
    }
    mod loads_8bit_indexed_and_indirect { 
        use super::*;

        #[test]
        fn test_ld_bc_mem_a() {
            let mut cpu = setup_cpu();
            // cpu.pc is already 0 from setup_cpu()
            cpu.a = 0xAB; // Value to store
            cpu.b = 0x12; // High byte of address
            cpu.c = 0x34; // Low byte of address
            let target_addr = 0x1234;
    
            cpu.ld_bc_mem_a(); // Method name was already updated in previous steps.
    
            assert_eq!(cpu.memory[target_addr], cpu.a, "Memory at (BC) should be loaded with value of A");
            assert_eq!(cpu.pc, 1, "PC should increment by 1 for LD (BC), A");
            assert_eq!(cpu.a, 0xAB, "Register A should not change for LD (BC), A");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_a_bc_mem() {
            let mut cpu = setup_cpu();
            let addr = 0x1234;
            cpu.b = (addr >> 8) as u8;
            cpu.c = (addr & 0xFF) as u8;
            cpu.memory[addr as usize] = 0xAB;
            let initial_pc = cpu.pc;

            cpu.ld_a_bc_mem();

            assert_eq!(cpu.a, 0xAB, "A should be loaded from memory[BC]");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_a_de_mem() {
            let mut cpu = setup_cpu();
            let addr = 0x5678;
            cpu.d = (addr >> 8) as u8;
            cpu.e = (addr & 0xFF) as u8;
            cpu.memory[addr as usize] = 0xCD;
            let initial_pc = cpu.pc;

            cpu.ld_a_de_mem();

            assert_eq!(cpu.a, 0xCD, "A should be loaded from memory[DE]");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_a_nn_mem() {
            let mut cpu = setup_cpu();
            let addr = 0xABCD;
            let addr_lo = (addr & 0xFF) as u8;
            let addr_hi = (addr >> 8) as u8;
            cpu.memory[addr as usize] = 0xEF;
            let initial_pc = cpu.pc;

            cpu.ld_a_nn_mem(addr_lo, addr_hi);

            assert_eq!(cpu.a, 0xEF, "A should be loaded from memory[nn]");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(3), "PC should increment by 3");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_de_mem_a() {
            let mut cpu = setup_cpu();
            let addr = 0x2345;
            cpu.d = (addr >> 8) as u8;
            cpu.e = (addr & 0xFF) as u8;
            cpu.a = 0xFA;
            let initial_pc = cpu.pc;

            cpu.ld_de_mem_a();

            assert_eq!(cpu.memory[addr as usize], 0xFA, "memory[DE] should be loaded from A");
            assert_eq!(cpu.a, 0xFA, "A should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_nn_mem_a() {
            let mut cpu = setup_cpu();
            let addr = 0xBEEF;
            let addr_lo = (addr & 0xFF) as u8;
            let addr_hi = (addr >> 8) as u8;
            cpu.a = 0x99;
            let initial_pc = cpu.pc;

            cpu.ld_nn_mem_a(addr_lo, addr_hi);

            assert_eq!(cpu.memory[addr as usize], 0x99, "memory[nn] should be loaded from A");
            assert_eq!(cpu.a, 0x99, "A should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(3), "PC should increment by 3");
            assert_flags!(cpu, false, false, false, false);
        }
    }
    mod loads_8bit_high_ram { 
        use super::*;

        #[test]
        fn test_ldh_a_c_offset_mem() {
            let mut cpu = setup_cpu();
            cpu.c = 0x12;
            let addr = 0xFF00 + cpu.c as u16;
            cpu.memory[addr as usize] = 0xAB;
            let initial_pc = cpu.pc;

            cpu.ldh_a_c_offset_mem();

            assert_eq!(cpu.a, 0xAB, "A should be loaded from memory[0xFF00+C]");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ldh_c_offset_mem_a() {
            let mut cpu = setup_cpu();
            cpu.c = 0x15;
            cpu.a = 0xCD;
            let addr = 0xFF00 + cpu.c as u16;
            let initial_pc = cpu.pc;

            cpu.ldh_c_offset_mem_a();

            assert_eq!(cpu.memory[addr as usize], 0xCD, "memory[0xFF00+C] should be loaded from A");
            assert_eq!(cpu.a, 0xCD, "A should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ldh_a_n_offset_mem() {
            let mut cpu = setup_cpu();
            let offset_n = 0x23;
            let addr = 0xFF00 + offset_n as u16;
            cpu.memory[addr as usize] = 0xEF;
            let initial_pc = cpu.pc;

            cpu.ldh_a_n_offset_mem(offset_n);

            assert_eq!(cpu.a, 0xEF, "A should be loaded from memory[0xFF00+n]");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ldh_n_offset_mem_a() {
            let mut cpu = setup_cpu();
            let offset_n = 0x2A;
            cpu.a = 0x55;
            let addr = 0xFF00 + offset_n as u16;
            let initial_pc = cpu.pc;
            
            cpu.ldh_n_offset_mem_a(offset_n);

            assert_eq!(cpu.memory[addr as usize], 0x55, "memory[0xFF00+n] should be loaded from A");
            assert_eq!(cpu.a, 0x55, "A should remain unchanged");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(2), "PC should increment by 2");
            assert_flags!(cpu, false, false, false, false);
        }
    }
    mod loads_8bit_inc_dec_hl { 
        use super::*;

        #[test]
        fn test_ldi_hl_mem_a() {
            let mut cpu = setup_cpu();
            let initial_addr: u16 = 0x1234;
            cpu.h = (initial_addr >> 8) as u8;
            cpu.l = (initial_addr & 0xFF) as u8;
            cpu.a = 0xAB;
            let initial_pc = cpu.pc;
        
            cpu.ldi_hl_mem_a();
        
            assert_eq!(cpu.memory[initial_addr as usize], 0xAB, "Memory at initial HL should get value from A");
            let expected_hl = initial_addr.wrapping_add(1);
            assert_eq!(((cpu.h as u16) << 8) | cpu.l as u16, expected_hl, "HL should increment");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ldi_a_hl_mem() {
            let mut cpu = setup_cpu();
            let initial_addr: u16 = 0x2345;
            cpu.h = (initial_addr >> 8) as u8;
            cpu.l = (initial_addr & 0xFF) as u8;
            cpu.memory[initial_addr as usize] = 0xCD;
            let initial_pc = cpu.pc;

            cpu.ldi_a_hl_mem();

            assert_eq!(cpu.a, 0xCD, "A should be loaded from memory at initial HL");
            let expected_hl = initial_addr.wrapping_add(1);
            assert_eq!(((cpu.h as u16) << 8) | cpu.l as u16, expected_hl, "HL should increment");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ldd_hl_mem_a() {
            let mut cpu = setup_cpu();
            let initial_addr: u16 = 0x3456;
            cpu.h = (initial_addr >> 8) as u8;
            cpu.l = (initial_addr & 0xFF) as u8;
            cpu.a = 0xEF;
            let initial_pc = cpu.pc;

            cpu.ldd_hl_mem_a();

            assert_eq!(cpu.memory[initial_addr as usize], 0xEF, "Memory at initial HL should get value from A");
            let expected_hl = initial_addr.wrapping_sub(1);
            assert_eq!(((cpu.h as u16) << 8) | cpu.l as u16, expected_hl, "HL should decrement");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ldd_a_hl_mem() {
            let mut cpu = setup_cpu();
            let initial_addr: u16 = 0x4567;
            cpu.h = (initial_addr >> 8) as u8;
            cpu.l = (initial_addr & 0xFF) as u8;
            cpu.memory[initial_addr as usize] = 0xFA;
            let initial_pc = cpu.pc;

            cpu.ldd_a_hl_mem();

            assert_eq!(cpu.a, 0xFA, "A should be loaded from memory at initial HL");
            let expected_hl = initial_addr.wrapping_sub(1);
            assert_eq!(((cpu.h as u16) << 8) | cpu.l as u16, expected_hl, "HL should decrement");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ldi_hl_mem_a_wrap() {
            let mut cpu = setup_cpu();
            let initial_addr: u16 = 0xFFFF;
            cpu.h = (initial_addr >> 8) as u8;
            cpu.l = (initial_addr & 0xFF) as u8;
            cpu.a = 0xAB;
            let initial_pc = cpu.pc;

            cpu.ldi_hl_mem_a();

            assert_eq!(cpu.memory[initial_addr as usize], 0xAB);
            assert_eq!(((cpu.h as u16) << 8) | cpu.l as u16, 0x0000, "HL should wrap to 0x0000");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1));
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ldd_a_hl_mem_wrap() {
            let mut cpu = setup_cpu();
            let initial_addr: u16 = 0x0000;
            cpu.h = (initial_addr >> 8) as u8;
            cpu.l = (initial_addr & 0xFF) as u8;
            cpu.memory[initial_addr as usize] = 0xCD;
            let initial_pc = cpu.pc;

            cpu.ldd_a_hl_mem();
            
            assert_eq!(cpu.a, 0xCD);
            assert_eq!(((cpu.h as u16) << 8) | cpu.l as u16, 0xFFFF, "HL should wrap to 0xFFFF");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1));
            assert_flags!(cpu, false, false, false, false);
        }
    }

    mod loads_16bit { 
        use super::*;

        #[test]
        fn test_ld_bc_nn() {
            let mut cpu = setup_cpu();
            let val_lo = 0x34;
            let val_hi = 0x12;
            let initial_pc = cpu.pc; // Should be 0
            cpu.ld_bc_nn(val_lo, val_hi);
            assert_eq!(cpu.b, val_hi, "Register B should be loaded with high byte");
            assert_eq!(cpu.c, val_lo, "Register C should be loaded with low byte");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(3), "PC should increment by 3");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_de_nn() {
            let mut cpu = setup_cpu();
            let val_lo = 0x78;
            let val_hi = 0x56;
            let initial_pc = cpu.pc;
            cpu.ld_de_nn(val_lo, val_hi);
            assert_eq!(cpu.d, val_hi, "Register D should be loaded with high byte");
            assert_eq!(cpu.e, val_lo, "Register E should be loaded with low byte");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(3), "PC should increment by 3");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_hl_nn() {
            let mut cpu = setup_cpu();
            let val_lo = 0xBC;
            let val_hi = 0x9A;
            let initial_pc = cpu.pc;
            cpu.ld_hl_nn(val_lo, val_hi);
            assert_eq!(cpu.h, val_hi, "Register H should be loaded with high byte");
            assert_eq!(cpu.l, val_lo, "Register L should be loaded with low byte");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(3), "PC should increment by 3");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_sp_nn() {
            let mut cpu = setup_cpu();
            let val_lo = 0xFE;
            let val_hi = 0xFF; // SP often initialized to 0xFFFE
            let initial_pc = cpu.pc;
            cpu.ld_sp_nn(val_lo, val_hi);
            assert_eq!(cpu.sp, 0xFFFE, "SP should be loaded with 0xFFFE");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(3), "PC should increment by 3");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_sp_hl() {
            let mut cpu = setup_cpu();
            cpu.h = 0x8C;
            cpu.l = 0x12;
            let initial_hl_val = 0x8C12u16;
            let initial_pc = cpu.pc;
            
            cpu.ld_sp_hl();

            assert_eq!(cpu.sp, initial_hl_val, "SP should be loaded with value from HL");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, false, false, false, false);
        }

        #[test]
        fn test_ld_nn_mem_sp() {
            let mut cpu = setup_cpu();
            cpu.sp = 0xABCD;
            let addr_lo = 0x34;
            let addr_hi = 0x12;
            let target_addr: u16 = 0x1234;
            let initial_pc = cpu.pc;

            cpu.ld_nn_mem_sp(addr_lo, addr_hi);

            assert_eq!(cpu.memory[target_addr as usize], (cpu.sp & 0xFF) as u8, "Memory at nn should store SP low byte");
            assert_eq!(cpu.memory[(target_addr.wrapping_add(1)) as usize], (cpu.sp >> 8) as u8, "Memory at nn+1 should store SP high byte");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(3), "PC should increment by 3");
            assert_flags!(cpu, false, false, false, false);
        }
    }
    mod stack_ops { 
        use super::*;

        #[test]
        fn test_push_bc() {
            let mut cpu = setup_cpu();
            cpu.sp = 0x0100; 
            cpu.b = 0x12;
            cpu.c = 0x34;
            let initial_pc = cpu.pc;
            let initial_f = cpu.f; // PUSH BC should not affect flags
        
            cpu.push_bc();
        
            assert_eq!(cpu.sp, 0x00FE, "SP should decrement by 2");
            assert_eq!(cpu.memory[cpu.sp.wrapping_add(1) as usize], 0x12, "Memory at SP+1 should be B");
            assert_eq!(cpu.memory[cpu.sp as usize], 0x34, "Memory at SP should be C");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_eq!(cpu.f, initial_f, "Flags should be unchanged by PUSH BC");
        }

        #[test]
        fn test_pop_bc() {
            let mut cpu = setup_cpu();
            cpu.sp = 0x00FE;
            cpu.memory[cpu.sp.wrapping_add(1) as usize] = 0xAB; // B
            cpu.memory[cpu.sp as usize] = 0xCD;          // C
            let initial_pc = cpu.pc;
            let initial_f = cpu.f; // POP BC should not affect flags

            cpu.pop_bc();

            assert_eq!(cpu.b, 0xAB, "B should be popped from stack");
            assert_eq!(cpu.c, 0xCD, "C should be popped from stack");
            assert_eq!(cpu.sp, 0x0100, "SP should increment by 2");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_eq!(cpu.f, initial_f, "Flags should be unchanged by POP BC");
        }

        #[test]
        fn test_push_de() {
            let mut cpu = setup_cpu();
            cpu.sp = 0x0100;
            cpu.d = 0x56;
            cpu.e = 0x78;
            let initial_pc = cpu.pc;
            let initial_f = cpu.f;

            cpu.push_de();

            assert_eq!(cpu.sp, 0x00FE, "SP should decrement by 2");
            assert_eq!(cpu.memory[cpu.sp.wrapping_add(1) as usize], 0x56, "Memory at SP+1 should be D");
            assert_eq!(cpu.memory[cpu.sp as usize], 0x78, "Memory at SP should be E");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_eq!(cpu.f, initial_f, "Flags should be unchanged by PUSH DE");
        }

        #[test]
        fn test_pop_de() {
            let mut cpu = setup_cpu();
            cpu.sp = 0x00FE;
            cpu.memory[cpu.sp.wrapping_add(1) as usize] = 0x56; // D
            cpu.memory[cpu.sp as usize] = 0x78;          // E
            let initial_pc = cpu.pc;
            let initial_f = cpu.f;

            cpu.pop_de();

            assert_eq!(cpu.d, 0x56, "D should be popped from stack");
            assert_eq!(cpu.e, 0x78, "E should be popped from stack");
            assert_eq!(cpu.sp, 0x0100, "SP should increment by 2");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_eq!(cpu.f, initial_f, "Flags should be unchanged by POP DE");
        }

        #[test]
        fn test_push_hl() {
            let mut cpu = setup_cpu();
            cpu.sp = 0x0100;
            cpu.h = 0x9A;
            cpu.l = 0xBC;
            let initial_pc = cpu.pc;
            let initial_f = cpu.f;

            cpu.push_hl();

            assert_eq!(cpu.sp, 0x00FE, "SP should decrement by 2");
            assert_eq!(cpu.memory[cpu.sp.wrapping_add(1) as usize], 0x9A, "Memory at SP+1 should be H");
            assert_eq!(cpu.memory[cpu.sp as usize], 0xBC, "Memory at SP should be L");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_eq!(cpu.f, initial_f, "Flags should be unchanged by PUSH HL");
        }

        #[test]
        fn test_pop_hl() {
            let mut cpu = setup_cpu();
            cpu.sp = 0x00FE;
            cpu.memory[cpu.sp.wrapping_add(1) as usize] = 0x9A; // H
            cpu.memory[cpu.sp as usize] = 0xBC;          // L
            let initial_pc = cpu.pc;
            let initial_f = cpu.f;

            cpu.pop_hl();

            assert_eq!(cpu.h, 0x9A, "H should be popped from stack");
            assert_eq!(cpu.l, 0xBC, "L should be popped from stack");
            assert_eq!(cpu.sp, 0x0100, "SP should increment by 2");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_eq!(cpu.f, initial_f, "Flags should be unchanged by POP HL");
        }
        
        #[test]
        fn test_push_af() {
            let mut cpu = setup_cpu();
            cpu.sp = 0x0100;
            cpu.a = 0xAA;
            cpu.f = 0xB7; // Z=1, N=0, H=1, C=1, lower bits 0x07 -> 10110111
            let initial_pc = cpu.pc;
            let initial_f_val_for_assertion = cpu.f; // Store the original F to assert flags are unchanged by PUSH op itself

            cpu.push_af();

            assert_eq!(cpu.sp, 0x00FE, "SP should decrement by 2");
            assert_eq!(cpu.memory[cpu.sp.wrapping_add(1) as usize], 0xAA, "Memory at SP+1 should be A");
            assert_eq!(cpu.memory[cpu.sp as usize], 0xB0, "Memory at SP should be F, masked"); // 0xB7 & 0xF0 = 0xB0
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            // PUSH AF should not change the F register itself, so original flags should be asserted
            cpu.f = initial_f_val_for_assertion; // Restore F for assert_flags! to check original state
            assert_flags!(cpu, true, false, true, true); 
        }

        #[test]
        fn test_pop_af() {
            let mut cpu = setup_cpu();
            cpu.sp = 0x00FE;
            cpu.memory[cpu.sp.wrapping_add(1) as usize] = 0xAB; // Value for A
            cpu.memory[cpu.sp as usize] = 0xF7; // Value for F on stack (Z=1,N=1,H=1,C=1 from 0xF0, lower bits 0x07)
            let initial_pc = cpu.pc;
        
            cpu.pop_af();
        
            assert_eq!(cpu.a, 0xAB, "A should be popped from stack");
            assert_eq!(cpu.f, 0xF0, "F should be popped from stack and lower bits masked"); // 0xF7 & 0xF0 = 0xF0
            assert_eq!(cpu.sp, 0x0100, "SP should increment by 2");
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1), "PC should increment by 1");
            assert_flags!(cpu, true, true, true, true); // Flags reflect popped F value (0xF0)
        }

        #[test]
        fn test_push_sp_wrap_low() { // SP = 0x0001
            let mut cpu = setup_cpu();
            cpu.sp = 0x0001;
            cpu.b = 0x12;
            cpu.c = 0x34;
            let initial_pc = cpu.pc;
            let initial_f = cpu.f;

            cpu.push_bc(); // Pushes B then C

            assert_eq!(cpu.sp, 0xFFFF, "SP should wrap from 0x0001 to 0xFFFF"); // 0x0001 - 2 = 0xFFFF
            assert_eq!(cpu.memory[0x0000], 0x12, "Memory at 0x0000 should be B"); // SP was 0x0001, B written to 0x0000
            assert_eq!(cpu.memory[0xFFFF], 0x34, "Memory at 0xFFFF should be C"); // C written to 0xFFFF
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1));
            assert_eq!(cpu.f, initial_f, "Flags should be unchanged");
        }

        #[test]
        fn test_push_sp_wrap_zero() { // SP = 0x0000
            let mut cpu = setup_cpu();
            cpu.sp = 0x0000;
            cpu.b = 0xAB;
            cpu.c = 0xCD;
            let initial_pc = cpu.pc;
            let initial_f = cpu.f;
            
            cpu.push_bc(); // Pushes B then C

            assert_eq!(cpu.sp, 0xFFFE, "SP should wrap from 0x0000 to 0xFFFE"); // 0x0000 - 2 = 0xFFFE
            assert_eq!(cpu.memory[0xFFFF], 0xAB, "Memory at 0xFFFF should be B"); // SP was 0x0000, B written to 0xFFFF
            assert_eq!(cpu.memory[0xFFFE], 0xCD, "Memory at 0xFFFE should be C"); // C written to 0xFFFE
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1));
            assert_eq!(cpu.f, initial_f, "Flags should be unchanged");
        }
        
        #[test]
        fn test_pop_sp_wrap_high() { // SP = 0xFFFE
            let mut cpu = setup_cpu();
            cpu.sp = 0xFFFE;
            cpu.memory[0xFFFF] = 0x12; // B
            cpu.memory[0xFFFE] = 0x34; // C
            let initial_pc = cpu.pc;
            let initial_f = cpu.f;

            cpu.pop_bc(); // Pops C then B

            assert_eq!(cpu.b, 0x12, "B should be 0x12");
            assert_eq!(cpu.c, 0x34, "C should be 0x34");
            assert_eq!(cpu.sp, 0x0000, "SP should wrap from 0xFFFE to 0x0000"); // 0xFFFE + 2 = 0x0000
            assert_eq!(cpu.pc, initial_pc.wrapping_add(1));
            assert_eq!(cpu.f, initial_f, "Flags should be unchanged");
        }
    }
    
    mod arithmetic_logical_8bit {
        use super::*;
        // Tests for ADD, ADC, SUB, SBC, AND, OR, XOR, CP will go here or in submodules.
    }
}
