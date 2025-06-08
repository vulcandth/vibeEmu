// src/apu/channel3.rs
use super::{Nr30, Nr31, Nr32, Nr33, Nr34};

pub struct Channel3 {
    pub nr30: Nr30,
    pub nr31: Nr31,
    pub nr32: Nr32,
    pub nr33: Nr33,
    pub nr34: Nr34,
    pub enabled: bool,
    length_counter: u16,
    frequency_timer: u16,
    sample_index: u8,
    current_sample_buffer: u8,
}

impl Channel3 {
    pub fn new() -> Self {
        Self {
            nr30: Nr30::new(), nr31: Nr31::new(), nr32: Nr32::new(), nr33: Nr33::new(), nr34: Nr34::new(),
            enabled: false, length_counter: 0, frequency_timer: 0, sample_index: 0, current_sample_buffer: 0,
        }
    }

    pub fn trigger(&mut self, _wave_ram_on_trigger: &[u8;16], current_frame_sequencer_step: u8) {
        self.enabled = self.nr30.dac_on();
        if self.length_counter == 0 {
            let length_data = self.nr31.sound_length_val();
            let is_max_length_condition = length_data == 0;
            let mut actual_load_val = if is_max_length_condition { 256 } else { 256 - (length_data as u16) };
            let next_fs_step_will_not_clock_length = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
            let length_is_enabled_on_trigger = self.nr34.is_length_enabled();
            if next_fs_step_will_not_clock_length && length_is_enabled_on_trigger && is_max_length_condition {
                actual_load_val = 255;
            }
            self.length_counter = actual_load_val;
        }

        let freq_lsb = self.nr33.freq_lo_val() as u16;
        let freq_msb = self.nr34.frequency_msb_val() as u16;
        let period_val = (freq_msb << 8) | freq_lsb;
        self.frequency_timer = (2048 - period_val) * 2;
        self.sample_index = 0;
        if !self.nr30.dac_on() { self.enabled = false; }
    }

    pub fn get_length_counter(&self) -> u16 { self.length_counter }

    pub fn extra_length_clock(&mut self, trigger_is_set_in_nrx4: bool) {
        if self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 && !trigger_is_set_in_nrx4 {
                self.enabled = false;
            }
        }
    }

    pub fn clock_length(&mut self) {
        if self.nr34.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 { self.enabled = false; }
        }
    }

    pub fn tick(&mut self, wave_ram: &[u8; 16]) {
        if !self.enabled { return; }
        self.frequency_timer = self.frequency_timer.saturating_sub(1);
        if self.frequency_timer == 0 {
            let freq_lsb = self.nr33.freq_lo_val() as u16;
            let freq_msb = self.nr34.frequency_msb_val() as u16;
            let period_val = (freq_msb << 8) | freq_lsb;
            self.frequency_timer = (2048 - period_val) * 2;
            if self.nr30.dac_on() && self.enabled {
                self.sample_index = (self.sample_index + 1) % 32;
                let byte_index = (self.sample_index / 2) as usize;
                let sample_byte = wave_ram[byte_index];
                self.current_sample_buffer = if self.sample_index % 2 == 0 {
                    (sample_byte >> 4) & 0x0F
                } else {
                    sample_byte & 0x0F
                };
            }
        }
    }

    pub fn get_output_sample(&self) -> u8 {
        if !self.enabled || !self.nr30.dac_on() { return 0; }
        let output_nibble = self.current_sample_buffer;
        let shifted_nibble = output_nibble >> self.nr32.get_volume_shift();
        shifted_nibble
    }

    // In src/apu/channel3.rs, within impl Channel3
    pub fn reload_length_on_enable(&mut self, current_frame_sequencer_step: u8) {
        let length_data = self.nr31.sound_length_val(); // 0-255
        let is_max_length_condition_len = length_data == 0;
        // Max length for channel 3 is 256
        let mut actual_load_val_len = if is_max_length_condition_len { 256 } else { 256 - length_data as u16 };

        let fs_condition_met = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
        // self.nr34.is_length_enabled() should be true
        if fs_condition_met && self.nr34.is_length_enabled() && is_max_length_condition_len {
            actual_load_val_len = 255;
        }
        self.length_counter = actual_load_val_len;
    }
}
