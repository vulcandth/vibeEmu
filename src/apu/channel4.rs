// src/apu/channel4.rs
use super::{Nr41, Nr42, Nr43, Nr44};

pub struct Channel4 {
    pub nr41: Nr41,
    pub nr42: Nr42,
    pub nr43: Nr43,
    pub nr44: Nr44,
    pub enabled: bool,
    length_counter: u16,
    frequency_timer: u32,
    envelope_volume: u8,
    envelope_period_timer: u8,
    envelope_running: bool,
    lfsr: u16,
}

impl Channel4 {
    pub fn new() -> Self {
        Self {
            nr41: Nr41::new(), nr42: Nr42::new(), nr43: Nr43::new(), nr44: Nr44::new(),
            enabled: false, length_counter: 0, frequency_timer: 0,
            envelope_volume: 0, envelope_period_timer: 0, envelope_running: false,
            lfsr: 0xFFFF,
        }
    }

    pub fn trigger(&mut self, current_frame_sequencer_step: u8) {
        if self.nr42.dac_power() { self.enabled = true; }
        else { self.enabled = false; return; }
        let length_data = self.nr41.initial_length_timer_val();
        let is_max_length_condition_len = length_data == 0;
        let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };
        let next_fs_step_will_not_clock_length = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
        let length_is_enabled_on_trigger = self.nr44.is_length_enabled();
        if next_fs_step_will_not_clock_length && length_is_enabled_on_trigger && is_max_length_condition_len {
            actual_load_val_len = 63;
        }
        self.length_counter = actual_load_val_len;
        self.update_frequency_timer();
        self.envelope_volume = self.nr42.initial_volume_val();
        let env_period_raw = self.nr42.envelope_period_val();
        let mut envelope_timer_load_val = if env_period_raw == 0 { 8 } else { env_period_raw };
        if current_frame_sequencer_step == 6 {
            envelope_timer_load_val += 1;
        }
        self.envelope_period_timer = envelope_timer_load_val;
        self.envelope_running = self.nr42.dac_power() && env_period_raw != 0;
        self.lfsr = 0xFFFF;
        if !self.nr42.dac_power() { self.enabled = false; }
    }

    fn update_frequency_timer(&mut self) {
        let r = self.nr43.clock_divider_val();
        let s = self.nr43.clock_shift();
        let divisor_val: u32 = if r == 0 { 8 } else { (r as u32) * 16 };
        self.frequency_timer = divisor_val << s;
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

    pub(super) fn is_envelope_running(&self) -> bool { self.envelope_running }
    pub(super) fn get_envelope_volume(&self) -> u8 { self.envelope_volume }
    pub(super) fn set_envelope_volume(&mut self, vol: u8) { self.envelope_volume = vol & 0x0F; }
    pub(super) fn force_disable_channel(&mut self) { self.enabled = false; }

    pub fn clock_length(&mut self) {
        if self.nr44.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 { self.enabled = false; }
        }
    }

    pub fn clock_envelope(&mut self) {
        if !self.envelope_running || !self.nr42.dac_power() { return; }
        let env_period_raw = self.nr42.envelope_period_val();
        if env_period_raw == 0 { self.envelope_running = false; return; }
        self.envelope_period_timer -= 1;
        if self.envelope_period_timer == 0 {
            self.envelope_period_timer = env_period_raw;
            let current_volume = self.envelope_volume;
            if self.nr42.envelope_direction_is_increase() {
                if current_volume < 15 { self.envelope_volume += 1; }
            } else {
                if current_volume > 0 { self.envelope_volume -= 1; }
            }
            if self.envelope_volume == 0 || self.envelope_volume == 15 { self.envelope_running = false; }
        }
    }

    pub fn tick(&mut self) {
        if !self.enabled { return; }
        self.frequency_timer = self.frequency_timer.saturating_sub(1);
        if self.frequency_timer == 0 {
            self.update_frequency_timer();
            let clock_shift_s = self.nr43.clock_shift();
            if clock_shift_s >= 14 {
                return;
            }
            let bit0 = self.lfsr & 0x0001;
            let bit1 = (self.lfsr >> 1) & 0x0001;
            let xor_result = bit0 ^ bit1;
            self.lfsr >>= 1;
            self.lfsr = (self.lfsr & !(1 << 14)) | (xor_result << 14);
            if self.nr43.lfsr_width_is_7bit() {
                self.lfsr = (self.lfsr & !(1 << 6)) | (xor_result << 6);
            }
        }
    }

    pub fn get_output_volume(&self) -> u8 {
        if !self.enabled || !self.nr42.dac_power() { return 0; }
        if (self.lfsr & 0x0001) == 0 { self.envelope_volume } else { 0 }
    }

    // In src/apu/channel4.rs, within impl Channel4
    pub fn reload_length_on_enable(&mut self, current_frame_sequencer_step: u8) {
        let length_data = self.nr41.initial_length_timer_val(); // 0-63
        let is_max_length_condition_len = length_data == 0;
        let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };

        let fs_condition_met = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
        // self.nr44.is_length_enabled() should be true
        if fs_condition_met && self.nr44.is_length_enabled() && is_max_length_condition_len {
            actual_load_val_len = 63;
        }
        self.length_counter = actual_load_val_len;
    }
}
