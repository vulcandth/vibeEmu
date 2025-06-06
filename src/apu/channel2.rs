// src/apu/channel2.rs
use super::{Nr21, Nr22, Nr23, Nr24};

pub struct Channel2 {
    // Registers
    pub nr21: Nr21,
    pub nr22: Nr22,
    pub nr23: Nr23,
    pub nr24: Nr24,

    // Internal state
    enabled: bool,
    length_counter: u16,
    frequency_timer: u16,
    duty_step: u8,

    // Volume Envelope State
    envelope_volume: u8,
    envelope_period_timer: u8,
    envelope_running: bool,
}

impl Channel2 {
    pub fn new() -> Self {
        Self {
            nr21: Nr21::new(),
            nr22: Nr22::new(),
            nr23: Nr23::new(),
            nr24: Nr24::new(),
            enabled: false,
            length_counter: 0,
            frequency_timer: 0,
            duty_step: 0,
            envelope_volume: 0,
            envelope_period_timer: 0,
            envelope_running: false,
        }
    }

    // trigger, clock_length, clock_envelope, tick, get_output_volume methods will be added here
    pub fn trigger(&mut self) {
        if self.nr22.dac_power() {
            self.enabled = true;
        }

        let length_data = self.nr21.initial_length_timer_val();
        self.length_counter = if length_data == 0 { 64 } else { 64 - length_data as u16 };

        let freq_lsb = self.nr23.freq_lo_val() as u16;
        let freq_msb = self.nr24.frequency_msb_val() as u16;
        let period_val = (freq_msb << 8) | freq_lsb;
        self.frequency_timer = (2048 - period_val) * 4;

        self.envelope_volume = self.nr22.initial_volume_val();
        let env_period = self.nr22.envelope_period_val();
        self.envelope_period_timer = if env_period == 0 { 8 } else { env_period };
        self.envelope_running = self.nr22.dac_power() && env_period != 0;

        if !self.nr22.dac_power() { // Ensure channel disabled if DAC is off
            self.enabled = false;
        }
    }

    pub fn clock_length(&mut self) { // Called at 256Hz
        if self.nr24.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

    pub fn clock_envelope(&mut self) { // Called at 64Hz
        if !self.envelope_running || !self.nr22.dac_power() {
            return;
        }

        let env_period = self.nr22.envelope_period_val();
        if env_period == 0 {
            self.envelope_running = false;
            return;
        }

        self.envelope_period_timer -= 1;
        if self.envelope_period_timer == 0 {
            self.envelope_period_timer = if env_period == 0 { 8 } else { env_period };

            let current_volume = self.envelope_volume;
            if self.nr22.envelope_direction_is_increase() {
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
            let freq_lsb = self.nr23.freq_lo_val() as u16;
            let freq_msb = self.nr24.frequency_msb_val() as u16;
            let period_val = (freq_msb << 8) | freq_lsb;
            self.frequency_timer = (2048 - period_val) * 4; // Reload

            self.duty_step = (self.duty_step + 1) % 8;
        }
    }

    pub fn get_output_volume(&self) -> u8 {
        if !self.enabled || !self.nr22.dac_power() {
            return 0;
        }

        let wave_duty = self.nr21.wave_pattern_duty_val();
        let wave_output = match wave_duty {
            0b00 => [0,0,0,0,0,0,0,1][self.duty_step as usize], // 12.5%
            0b01 => [1,0,0,0,0,0,0,1][self.duty_step as usize], // 25%
            0b10 => [1,0,0,0,0,1,1,1][self.duty_step as usize], // 50%
            0b11 => [0,1,1,1,1,1,1,0][self.duty_step as usize], // 75%
            _ => 0,
        };

        if wave_output == 1 {
            self.envelope_volume
        } else {
            0
        }
    }
}
