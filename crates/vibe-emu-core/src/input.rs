/// Joypad input register (P1/JOYP) and button-state tracking.
#[derive(Debug)]
pub struct Input {
    p1: u8,
    state: u8,
    extra_players: [u8; 3],
}

impl Input {
    /// Create a new `Input` in the power-on state.
    pub fn new() -> Self {
        Self {
            p1: 0xCF,
            state: 0xFF,
            extra_players: [0xff; 3],
        }
    }

    /// Read the current P1 register value based on the selected button row.
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

    /// Read a zero-based SGB controller, combining selected button groups.
    pub fn read_player(&self, player: usize) -> u8 {
        let state = if player == 0 {
            self.state
        } else {
            self.extra_players.get(player - 1).copied().unwrap_or(0xff)
        };
        let mut low = 15;
        if self.p1 & 0x10 == 0 {
            low &= state & 15;
        }
        if self.p1 & 0x20 == 0 {
            low &= state >> 4;
        }
        (self.p1 & 0xf0) | low
    }

    /// Set a zero-based SGB controller's active-low button state.
    pub fn set_player_state(&mut self, player: usize, state: u8) {
        if player == 0 {
            self.state = state;
        } else if let Some(slot) = self.extra_players.get_mut(player - 1) {
            *slot = state;
        }
    }

    /// Update an SGB controller and request a joypad interrupt on a press.
    /// Invalid player indices are ignored, just as in `set_player_state`.
    pub fn update_player_state(&mut self, player: usize, state: u8, if_reg: &mut u8) {
        if player == 0 {
            self.update_state(state, if_reg);
        } else if let Some(slot) = self.extra_players.get_mut(player - 1) {
            if *slot & !state != 0 {
                *if_reg |= 0x10;
            }
            *slot = state;
        }
    }

    /// Write to the P1 register (selects button row).
    pub fn write(&mut self, val: u8) {
        self.p1 = (self.p1 & 0xCF) | (val & 0x30);
    }

    /// Unconditionally overwrite the raw button state byte.
    pub fn set_state(&mut self, state: u8) {
        self.state = state;
    }

    /// Update the input state and set the joypad interrupt flag if any
    /// button transitioned from released to pressed.
    ///
    /// The `state` byte uses Game Boy active-low encoding: bit = 0 means
    /// pressed. Bits 0–3 are the D-pad (right/left/up/down) and bits 4–7 are
    /// the buttons (A/B/Select/Start).
    ///
    /// # Examples
    ///
    /// ```
    /// use vibe_emu_core::input::Input;
    ///
    /// let mut input = Input::new();
    /// let mut if_reg = 0u8;
    ///
    /// // Press the A button (bit 4 active-low = 0).
    /// input.update_state(0b1110_1111, &mut if_reg);
    /// assert_eq!(if_reg & 0x10, 0x10); // joypad interrupt requested
    /// ```
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
