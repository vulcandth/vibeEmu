/// Divider and timer unit (DIV/TIMA/TMA/TAC at 0xFF04–0xFF07).
#[derive(Debug)]
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
    /// CPU cycles before an overflow or pending write/reload effect. Ordinary
    /// TIMA increments are independent of other devices and can be combined.
    pub(crate) fn idle_cycles(&self) -> u16 {
        if self.pending_reload.is_some() || self.tma_latch.is_some() {
            return 0;
        }
        if self.tac & 4 == 0 {
            return u16::MAX;
        }
        if self.last_signal != self.signal() {
            return 0;
        }
        let bit = [9, 3, 5, 7][(self.tac & 3) as usize];
        let period = 1u32 << (bit + 1);
        let before_overflow =
            period * (256 - u32::from(self.tima)) - u32::from(self.div) % period - 1;
        before_overflow.min(u32::from(u16::MAX)) as u16
    }

    /// Create a new `Timer` with all registers at their power-on values.
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

    /// Read a timer register at `addr` (0xFF04–0xFF07).
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => (self.div >> 8) as u8,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac | 0xF8,
            _ => 0xFF,
        }
    }

    /// Write `val` to a timer register at `addr` (0xFF04–0xFF07).
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
    ///
    /// # Examples
    ///
    /// ```
    /// use vibe_emu_core::timer::Timer;
    ///
    /// let mut timer = Timer::new();
    /// let mut if_reg = 0u8;
    ///
    /// // Enable timer at 4096 Hz (TAC = 0x04) with TMA = 0.
    /// timer.write(0xFF07, 0x04, &mut if_reg);
    ///
    /// // Step enough cycles for DIV to advance and eventually overflow TIMA.
    /// timer.step(256, &mut if_reg);
    ///
    /// // DIV should now have advanced.
    /// assert!(timer.div > 0);
    /// ```
    pub fn step(&mut self, cycles: u16, if_reg: &mut u8) {
        if cycles == 0 {
            return;
        }

        // Common fast path: with timer disabled and no pending reload/write
        // timing effects, only DIV advances.
        if self.tac & 0x04 == 0 && self.pending_reload.is_none() && self.tma_latch.is_none() {
            self.div = self.div.wrapping_add(cycles);
            self.last_signal = false;
            self.reloading = false;
            return;
        }

        if self.pending_reload.is_none() && self.tma_latch.is_none() {
            let timer_bit = match self.tac & 0x03 {
                0x00 => 9,
                0x01 => 3,
                0x02 => 5,
                0x03 => 7,
                _ => unreachable!(),
            };
            let lower_mask = (1u16 << (timer_bit + 1)) - 1;
            let low = self.div & lower_mask;
            let to_falling = lower_mask.wrapping_sub(low).wrapping_add(1);

            if cycles < to_falling {
                self.div = self.div.wrapping_add(cycles);
                self.last_signal = self.signal();
                self.reloading = false;
                return;
            }

            // Count falling edges directly when no overflow can occur. The
            // delayed reload/write collision path below remains cycle-stepped.
            // Public register fields can be changed directly by callers, so
            // require a synchronized signal before using this edge count.
            if self.tac & 4 != 0 && self.last_signal == self.signal() {
                let edges = (u32::from(low) + u32::from(cycles)) >> (timer_bit + 1);
                if edges <= u32::from(u8::MAX - self.tima) {
                    self.tima += edges as u8;
                    self.div = self.div.wrapping_add(cycles);
                    self.last_signal = self.signal();
                    self.reloading = false;
                    return;
                }
            }
        }

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
        self.reset_div_inner(if_reg, false);
    }

    pub(crate) fn reset_div_for_speed_switch(
        &mut self,
        if_reg: &mut u8,
        revision: crate::hardware::CgbRevision,
    ) {
        // AGE's spsw-tima ROMs verify that STOP misses the first M-cycle
        // of the selected divider bit's high phase. B/C only exhibit this
        // at 4096 Hz; E also exhibits it at 16384 and 65536 Hz.
        let selection = self.tac & 3;
        let delayed =
            selection == 0 || (revision == crate::hardware::CgbRevision::RevE && selection != 1);
        let bit = 1u16 << [9, 3, 5, 7][selection as usize];
        let phase = self.div & (bit * 2 - 1);
        let suppress = delayed && (bit..bit + 4).contains(&phase);
        self.reset_div_inner(if_reg, suppress);
    }

    fn reset_div_inner(&mut self, if_reg: &mut u8, suppress_increment: bool) {
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
        if prev && !new && !suppress_increment {
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

    #[inline]
    fn timer_bit_with(div: u16, tac: u8) -> u8 {
        match tac & 0x03 {
            0x00 => ((div >> 9) & 1) as u8,
            0x01 => ((div >> 3) & 1) as u8,
            0x02 => ((div >> 5) & 1) as u8,
            0x03 => ((div >> 7) & 1) as u8,
            _ => 0,
        }
    }

    #[inline]
    fn signal(&self) -> bool {
        if self.tac & 0x04 == 0 {
            false
        } else {
            self.timer_bit() != 0
        }
    }

    #[inline]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batched_edges_and_idle_deadlines_match_single_cycles() {
        for tac in 0..8 {
            for div in [0, 7, 8, 15, 16, 31, 63, 127, 255, 511, 1023, 0xfffb, 0xffff] {
                for tima in [0, 113, 254, 255] {
                    for cycles in [0, 1, 3, 4, 15, 16, 17, 63, 256, 4096, 65535] {
                        let make = || {
                            let mut timer = Timer::new();
                            timer.div = div;
                            timer.write(0xff07, tac, &mut 0);
                            timer.tima = tima;
                            timer.tma = 0xf7;
                            timer
                        };
                        let mut actual = make();
                        let mut expected = make();
                        let idle = actual.idle_cycles();
                        let mut actual_if = 0xe0;
                        let mut expected_if = actual_if;
                        actual.step(cycles, &mut actual_if);
                        for _ in 0..cycles {
                            expected.step(1, &mut expected_if);
                        }
                        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
                        assert_eq!(actual_if, expected_if);
                        if cycles <= idle {
                            assert_eq!(actual_if, 0xe0);
                            assert!(actual.pending_reload.is_none());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn pending_writes_and_reloads_disable_idle_prediction() {
        let mut timer = Timer::new();
        let mut if_reg = 0;
        timer.write(0xff06, 0x42, &mut if_reg);
        assert_eq!(timer.idle_cycles(), 0);
        timer.step(1, &mut if_reg);
        timer.div = 15;
        timer.write(0xff07, 5, &mut if_reg);
        timer.tima = 255;
        timer.step(1, &mut if_reg);
        for _ in 0..4 {
            assert_eq!(timer.idle_cycles(), 0);
            timer.step(1, &mut if_reg);
        }
        assert_eq!(if_reg, 4);
        assert_eq!(timer.tima, 0x42);
        assert!(timer.idle_cycles() > 0);
        // A caller may change the public divider without updating the edge latch.
        timer.div = 8;
        assert_eq!(timer.idle_cycles(), 0);
    }
}
