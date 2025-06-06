// src/apu/channel4.rs
use super::{Nr41, Nr42, Nr43, Nr44};

pub struct Channel4 {
    // Registers
    pub nr41: Nr41,
    pub nr42: Nr42,
    pub nr43: Nr43,
    pub nr44: Nr44,

    // Internal state
    enabled: bool,
    length_counter: u16,
    frequency_timer: u32, // Using u32 as noise timer values can be larger

    // Volume Envelope State
    envelope_volume: u8,
    envelope_period_timer: u8,
    envelope_running: bool,

    // LFSR State
    lfsr: u16, // 15-bit shift register
    // lfsr_output_bit: bool, // Output is derived from lfsr & 0x01 == 0
}

impl Channel4 {
    pub fn new() -> Self {
        Self {
            nr41: Nr41::new(),
            nr42: Nr42::new(),
            nr43: Nr43::new(),
            nr44: Nr44::new(),
            enabled: false,
            length_counter: 0,
            frequency_timer: 0, // Will be loaded by trigger
            envelope_volume: 0,
            envelope_period_timer: 0,
            envelope_running: false,
            lfsr: 0xFFFF, // Initialized to all 1s
        }
    }

    // trigger, clock_length, clock_envelope, tick, get_output_volume methods will be added here
    pub fn trigger(&mut self) {
        if self.nr42.dac_power() {
            self.enabled = true;
        }

        let length_data = self.nr41.initial_length_timer_val();
        self.length_counter = if length_data == 0 { 64 } else { 64 - length_data as u16 };

        self.update_frequency_timer(); // Load timer based on NR43

        self.envelope_volume = self.nr42.initial_volume_val();
        let env_period = self.nr42.envelope_period_val();
        self.envelope_period_timer = if env_period == 0 { 8 } else { env_period };
        self.envelope_running = self.nr42.dac_power() && env_period != 0;

        self.lfsr = 0xFFFF; // Reset LFSR to all 1s

        if !self.nr42.dac_power() { // Ensure channel disabled if DAC is off
            self.enabled = false;
        }
    }

    fn update_frequency_timer(&mut self) {
        let r = self.nr43.clock_divider_val();
        let s = self.nr43.clock_shift();

        // Formula for timer period in CPU clocks: (if r == 0 { 8 } else { r * 16 }) * (1 << s)
        let divisor_val: u32 = if r == 0 { 8 } else { (r as u32) * 16 };
        self.frequency_timer = divisor_val << s;
    }

    pub fn clock_length(&mut self) { // Called at 256Hz
        if self.nr44.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

    pub fn clock_envelope(&mut self) { // Called at 64Hz
        if !self.envelope_running || !self.nr42.dac_power() {
            return;
        }

        let env_period = self.nr42.envelope_period_val();
        if env_period == 0 {
            self.envelope_running = false;
            return;
        }

        self.envelope_period_timer -= 1;
        if self.envelope_period_timer == 0 {
            self.envelope_period_timer = if env_period == 0 { 8 } else { env_period };

            let current_volume = self.envelope_volume;
            if self.nr42.envelope_direction_is_increase() {
                if current_volume < 15 {
                    self.envelope_volume += 1;
                }
            } else { // Decrease
                if current_volume > 0 {
                    self.envelope_volume -= 1;
                }
            }

            if self.envelope_volume == 0 || self.envelope_volume == 15 {
                self.envelope_running = false;
            }
        }
    }

    pub fn tick(&mut self) {
        if !self.enabled {
            return;
        }

        self.frequency_timer -= 1;
        if self.frequency_timer == 0 {
            self.update_frequency_timer(); // Reload timer

            // Clock the LFSR
            let bit0 = self.lfsr & 0x0001;
            let bit1 = (self.lfsr >> 1) & 0x0001;
            let xor_result = bit0 ^ bit1;

            self.lfsr >>= 1;
            self.lfsr = (self.lfsr & !(1 << 14)) | (xor_result << 14); // Place result in bit 14 (0-indexed)

            if self.nr43.lfsr_width_is_7bit() {
                // If 7-bit mode, result is also XORed into bit 6
                self.lfsr = (self.lfsr & !(1 << 6)) | (xor_result << 6);
            }
        }
    }

    pub fn get_output_volume(&self) -> u8 {
        if !self.enabled || !self.nr42.dac_power() {
            return 0;
        }

        // Output is based on the INVERTED bit 0 of LFSR
        if (self.lfsr & 0x0001) == 0 {
            self.envelope_volume
        } else {
            0
        }
    }
}
