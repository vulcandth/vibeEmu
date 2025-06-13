pub struct Timer {
    /// 16-bit internal divider counter. DIV register is the upper 8 bits.
    pub div: u16,
    /// Timer counter
    pub tima: u8,
    /// Timer modulo
    pub tma: u8,
    /// Timer control
    pub tac: u8,
    last_bit: bool,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            last_bit: false,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => (self.div >> 8) as u8,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac | 0xF8,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF04 => {
                self.div = 0;
                self.last_bit = false;
            }
            0xFF05 => self.tima = val,
            0xFF06 => self.tma = val,
            0xFF07 => {
                self.tac = val & 0x07;
                self.last_bit = self.timer_bit() != 0;
            }
            _ => {}
        }
    }

    /// Advance the timer by `cycles` CPU cycles and update IF when TIMA
    /// overflows.
    pub fn step(&mut self, cycles: u16, if_reg: &mut u8) {
        for _ in 0..cycles {
            let prev_bit = self.timer_bit();
            self.div = self.div.wrapping_add(1);
            let new_bit = self.timer_bit();
            if self.tac & 0x04 != 0 && prev_bit == 1 && new_bit == 0 {
                if self.tima == 0xFF {
                    self.tima = self.tma;
                    *if_reg |= 0x04;
                } else {
                    self.tima = self.tima.wrapping_add(1);
                }
            }
        }
        self.last_bit = self.timer_bit() != 0;
    }

    fn timer_bit(&self) -> u8 {
        match self.tac & 0x03 {
            0x00 => ((self.div >> 9) & 1) as u8,
            0x01 => ((self.div >> 3) & 1) as u8,
            0x02 => ((self.div >> 5) & 1) as u8,
            0x03 => ((self.div >> 7) & 1) as u8,
            _ => 0,
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
