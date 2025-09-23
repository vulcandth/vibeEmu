pub struct Timer {
    /// 16-bit internal divider counter. DIV register is the upper 8 bits.
    pub div: u16,
    /// Timer counter
    pub tima: u8,
    /// Timer modulo
    pub tma: u8,
    /// Timer control
    pub tac: u8,
    last_signal: bool,
    /// Previous value of TMA when a write occurred this cycle
    tma_latch: Option<u8>,
    /// Value to reload TIMA with after an overflow delay
    pending_reload: Option<u8>,
    /// Delay before the pending reload is applied
    reload_delay: u8,
    /// Whether the reload is being applied this cycle
    reloading: bool,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            last_signal: false,
            tma_latch: None,
            pending_reload: None,
            reload_delay: 0,
            reloading: false,
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

    pub fn write(&mut self, addr: u16, val: u8, if_reg: &mut u8) {
        match addr {
            0xFF04 => {
                self.reset_div(if_reg);
            }
            0xFF05 => {
                if self.reloading || (self.pending_reload.is_some() && self.reload_delay == 0) {
                    return;
                }
                self.tima = val;
                if self.pending_reload.is_some() && self.reload_delay > 0 {
                    // writing during cycle A cancels the pending reload
                    self.pending_reload = None;
                    self.reload_delay = 0;
                }
                // writes during cycle B are ignored
            }
            0xFF06 => {
                // Store the old value so that if a reload occurs in the same
                // cycle, the old value will be used.
                self.tma_latch = Some(self.tma);
                self.tma = val;
                if self.pending_reload.is_some() {
                    // update the pending reload regardless of timing so
                    // mid-delay writes affect the current reload value
                    self.pending_reload = Some(val);
                }
                if self.reloading {
                    self.tima = val;
                }
            }
            0xFF07 => {
                let prev = Self::signal_with(self.div, self.tac);
                self.tac = val & 0x07;
                let new = Self::signal_with(self.div, self.tac);
                if prev && !new {
                    let tma_old = self.tma_latch.take();
                    self.increment(if_reg, tma_old);
                }
                self.last_signal = new;
            }
            _ => {}
        }
    }

    /// Advance the timer by `cycles` CPU cycles and update IF when TIMA
    /// overflows.
    pub fn step(&mut self, cycles: u16, if_reg: &mut u8) {
        for _ in 0..cycles {
            self.reloading = false;
            if let Some(val) = self.pending_reload {
                if self.reload_delay == 0 {
                    self.tima = val;
                    *if_reg |= 0x04;
                    self.pending_reload = None;
                    self.reloading = true;
                } else {
                    self.reload_delay -= 1;
                }
            }
            let prev = self.last_signal;
            // Take any pending TMA write for this cycle
            let tma_old = self.tma_latch.take();
            self.div = self.div.wrapping_add(1);
            let new = self.signal();
            if prev && !new {
                self.increment(if_reg, tma_old);
            }
            self.last_signal = new;
        }
    }

    /// Reset the internal divider counter, applying TIMA edge logic.
    pub fn reset_div(&mut self, if_reg: &mut u8) {
        self.reloading = false;
        if let Some(val) = self.pending_reload {
            if self.reload_delay == 0 {
                self.tima = val;
                *if_reg |= 0x04;
                self.pending_reload = None;
                self.reloading = true;
            } else {
                self.reload_delay -= 1;
            }
        }
        let prev = Self::signal_with(self.div, self.tac);
        self.div = 0;
        let new = Self::signal_with(self.div, self.tac);
        if prev && !new {
            let tma_old = self.tma_latch.take();
            self.increment(if_reg, tma_old);
        }
        self.last_signal = new;
    }

    fn increment(&mut self, _if_reg: &mut u8, tma_old: Option<u8>) {
        if self.tima == 0xFF {
            self.tima = 0;
            self.pending_reload = Some(tma_old.unwrap_or(self.tma));
            self.reload_delay = 3;
        } else {
            self.tima = self.tima.wrapping_add(1);
        }
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

    fn timer_bit_with(div: u16, tac: u8) -> u8 {
        match tac & 0x03 {
            0x00 => ((div >> 9) & 1) as u8,
            0x01 => ((div >> 3) & 1) as u8,
            0x02 => ((div >> 5) & 1) as u8,
            0x03 => ((div >> 7) & 1) as u8,
            _ => 0,
        }
    }

    fn signal(&self) -> bool {
        if self.tac & 0x04 == 0 {
            false
        } else {
            self.timer_bit() != 0
        }
    }

    fn signal_with(div: u16, tac: u8) -> bool {
        if tac & 0x04 == 0 {
            false
        } else {
            Self::timer_bit_with(div, tac) != 0
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
