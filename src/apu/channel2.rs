// src/apu/channel2.rs
use super::{Nr21, Nr22, Nr23, Nr24};
use log::debug;

pub struct Channel2 {
    pub nr21: Nr21,
    pub nr22: Nr22,
    pub nr23: Nr23,
    pub nr24: Nr24,
    pub enabled: bool,
    length_counter: u16,
    frequency_timer: u16,
    duty_step: u8,
    envelope_volume: u8,
    envelope_period_timer: u8,
    envelope_running: bool,
    has_been_triggered_since_power_on: bool,
    force_output_zero_for_next_sample: bool,
}

impl Channel2 {
    pub fn new() -> Self {
        Self {
            nr21: Nr21::new(), nr22: Nr22::new(), nr23: Nr23::new(), nr24: Nr24::new(),
            enabled: false, length_counter: 0, frequency_timer: 0, duty_step: 0,
            envelope_volume: 0, envelope_period_timer: 0, envelope_running: false,
            has_been_triggered_since_power_on: false, force_output_zero_for_next_sample: false,
        }
    }

    pub fn power_on_reset(&mut self) {
        self.has_been_triggered_since_power_on = false;
        self.force_output_zero_for_next_sample = false;
    }

    pub fn trigger(&mut self, current_frame_sequencer_step: u8) {
        debug!(
            "CH2 trigger start: len_cnt={} len_enabled={} fs_step={}",
            self.length_counter,
            self.nr24.is_length_enabled(),
            current_frame_sequencer_step
        );
        if !self.has_been_triggered_since_power_on {
            self.force_output_zero_for_next_sample = true;
            self.has_been_triggered_since_power_on = true;
        }
        if self.nr22.dac_power() { self.enabled = true; } else { self.enabled = false; return; }
        let length_data = self.nr21.initial_length_timer_val();
        let is_max_length_condition_len = length_data == 0;
        if self.length_counter == 0 {
            let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };
            let next_fs_step_will_not_clock_length = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
            let length_is_enabled_on_trigger = self.nr24.is_length_enabled();
            if next_fs_step_will_not_clock_length && length_is_enabled_on_trigger && is_max_length_condition_len {
                actual_load_val_len = 63;
            }
            self.length_counter = actual_load_val_len;
        }
        let old_low_two_bits = self.frequency_timer & 0b11;
        let freq_lsb = self.nr23.freq_lo_val() as u16;
        let freq_msb = self.nr24.frequency_msb_val() as u16;
        let period_val = (freq_msb << 8) | freq_lsb;
        self.frequency_timer = (2048 - period_val) * 4;
        self.frequency_timer = (self.frequency_timer & !0b11) | old_low_two_bits;
        self.duty_step = 0;
        self.envelope_volume = self.nr22.initial_volume_val();
        let env_period_raw = self.nr22.envelope_period_val();
        let mut envelope_timer_load_val = if env_period_raw == 0 { 8 } else { env_period_raw };
        if current_frame_sequencer_step == 6 {
            envelope_timer_load_val += 1;
        }
        self.envelope_period_timer = envelope_timer_load_val;
        self.envelope_running = self.nr22.dac_power() && env_period_raw != 0;
        debug!("CH2 trigger end: len_cnt={}", self.length_counter);
    }

    pub fn get_length_counter(&self) -> u16 { self.length_counter }

    pub fn extra_length_clock(&mut self, trigger_is_set_in_nrx4: bool) {
        if self.length_counter > 0 {
            debug!(
                "CH2 extra_length_clock: before={} trigger={}",
                self.length_counter,
                trigger_is_set_in_nrx4
            );
            self.length_counter -= 1;
            if self.length_counter == 0 && !trigger_is_set_in_nrx4 {
                self.enabled = false;
            }
            debug!("CH2 extra_length_clock: after={}", self.length_counter);
        }
    }

    pub(super) fn is_envelope_running(&self) -> bool { self.envelope_running }
    pub(super) fn get_envelope_volume(&self) -> u8 { self.envelope_volume }
    pub(super) fn set_envelope_volume(&mut self, vol: u8) { self.envelope_volume = vol & 0x0F; }
    pub(super) fn force_disable_channel(&mut self) { self.enabled = false; }

    pub fn clock_length(&mut self) {
        if self.nr24.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 { self.enabled = false; }
        }
    }

    pub fn clock_envelope(&mut self) {
        if !self.envelope_running || !self.nr22.dac_power() { return; }
        let env_period_raw = self.nr22.envelope_period_val();
        if env_period_raw == 0 { self.envelope_running = false; return; }
        self.envelope_period_timer -= 1;
        if self.envelope_period_timer == 0 {
            self.envelope_period_timer = env_period_raw;
            let current_volume = self.envelope_volume;
            if self.nr22.envelope_direction_is_increase() {
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
            let freq_lsb = self.nr23.freq_lo_val() as u16;
            let freq_msb = self.nr24.frequency_msb_val() as u16;
            let period_val = (freq_msb << 8) | freq_lsb;
            self.frequency_timer = (2048 - period_val) * 4;
            if self.has_been_triggered_since_power_on {
                self.duty_step = (self.duty_step + 1) % 8;
            }
        }
    }

    pub fn get_output_volume(&mut self) -> u8 {
        if self.force_output_zero_for_next_sample { self.force_output_zero_for_next_sample = false; return 0; }
        if !self.enabled || !self.nr22.dac_power() { return 0; }
        let wave_duty = self.nr21.wave_pattern_duty_val();
        let wave_output = match wave_duty {
            0b00 => [0,0,0,0,0,0,0,1][self.duty_step as usize],
            0b01 => [1,0,0,0,0,0,0,1][self.duty_step as usize],
            0b10 => [1,0,0,0,0,1,1,1][self.duty_step as usize],
            0b11 => [0,1,1,1,1,1,1,0][self.duty_step as usize],
            _ => 0,
        };
        if wave_output == 1 { self.envelope_volume } else { 0 }
    }

    // In src/apu/channel2.rs, within impl Channel2
    pub fn reload_length_on_enable(&mut self, current_frame_sequencer_step: u8) {
        let length_data = self.nr21.initial_length_timer_val(); // 0-63
        let is_max_length_condition_len = length_data == 0;
        let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };

        let fs_condition_met = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
        // self.nr24.is_length_enabled() should be true
        if fs_condition_met && self.nr24.is_length_enabled() && is_max_length_condition_len {
            actual_load_val_len = 63;
        }
        self.length_counter = actual_load_val_len;
    }
}
