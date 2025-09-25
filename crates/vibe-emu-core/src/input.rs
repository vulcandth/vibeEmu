pub struct Input {
    p1: u8,
    state: u8,
}

impl Input {
    pub fn new() -> Self {
        Self {
            p1: 0xCF,
            state: 0xFF,
        }
    }

    pub fn read(&self) -> u8 {
        let mut res = self.p1 & 0xF0;
        if self.p1 & 0x10 == 0 {
            res |= self.state & 0x0F;
        } else if self.p1 & 0x20 == 0 {
            res |= (self.state >> 4) & 0x0F;
        } else {
            res |= 0x0F;
        }
        res
    }

    pub fn write(&mut self, val: u8) {
        self.p1 = (self.p1 & 0xCF) | (val & 0x30);
    }

    pub fn set_state(&mut self, state: u8) {
        self.state = state;
    }

    /// Update the input state and set the joypad interrupt flag if any
    /// button transitioned from released to pressed.
    pub fn update_state(&mut self, state: u8, if_reg: &mut u8) {
        // Bits are active-low: 0 = pressed
        let newly_pressed = self.state & !state;
        if newly_pressed != 0 {
            *if_reg |= 0x10; // Joypad interrupt
        }
        self.state = state;
    }
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}
