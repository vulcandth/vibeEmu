// src/apu/channel4.rs
use super::{Nr41, Nr42, Nr43, Nr44};
use crate::bus::SystemMode; // Added import

pub struct Channel4 {
    pub nr41: Nr41,
    pub nr42: Nr42,
    pub nr43: Nr43,
    pub nr44: Nr44,
    pub enabled: bool,
    length_counter: u16,
    // frequency_timer: u32, // Replaced by sample_countdown
    pub(super) envelope_volume: u8, // Made pub(super)
    pub(super) envelope_period_timer: u8, // Made pub(super)
    pub(super) lfsr: u16, // Linear Feedback Shift Register, should be 15-bit (0x7FFF mask on use) // Made pub(super)

    // Fields for NRx2 envelope glitch logic
    pub envelope_clock_active: bool,
    pub envelope_clock_should_lock: bool,
    pub envelope_clock_locked: bool,

    // New/Modified fields for NR44 alignment
    pub(super) sample_countdown: u16, // Replaces frequency_timer for sample generation timing, made pub(super)
    pub(super) length_enabled_internal: bool, // Made pub(super)
    pulsed_on_trigger: bool,
    current_lfsr_sample_is_on: bool, // Output of LFSR (inverted or not)
    dmg_delayed_start_timer: u8,
    pub(super) lfsr_narrow_mode: bool, // From NR43, made pub(super)
    pub(super) countdown_was_reloaded_by_tick: bool, // New field, made pub(super)
}

impl Channel4 {
    pub fn new() -> Self {
        Self {
            nr41: Nr41::new(), nr42: Nr42::new(), nr43: Nr43::new(), nr44: Nr44::new(),
            enabled: false, length_counter: 0,
            sample_countdown: 0,
            envelope_volume: 0, envelope_period_timer: 0,
            lfsr: 0xFFFF,

            envelope_clock_active: false,
            envelope_clock_should_lock: false,
            envelope_clock_locked: false,

            length_enabled_internal: false,
            pulsed_on_trigger: false,
            current_lfsr_sample_is_on: false,
            dmg_delayed_start_timer: 0,
            lfsr_narrow_mode: false, // Initialized
            countdown_was_reloaded_by_tick: false,
        }
    }

    pub fn power_on_reset(&mut self) {
        self.enabled = false;
        self.length_counter = 0;
        self.sample_countdown = 0;
        self.envelope_volume = 0;
        self.envelope_period_timer = 0;
        self.lfsr = 0xFFFF;
        self.envelope_clock_active = false;
        self.envelope_clock_should_lock = false;
        self.envelope_clock_locked = false;
        self.length_enabled_internal = false;
        self.pulsed_on_trigger = false;
        self.current_lfsr_sample_is_on = false;
        self.dmg_delayed_start_timer = 0;
        self.countdown_was_reloaded_by_tick = false;
    }

    pub fn trigger(&mut self, system_mode: SystemMode, lf_div_for_alignment: u8, current_frame_sequencer_step: u8) {
        self.pulsed_on_trigger = true;
        self.envelope_clock_active = false;
        self.envelope_clock_should_lock = false;
        self.envelope_clock_locked = false;

        let lf_div_is_odd = (lf_div_for_alignment & 1) != 0;
        if !matches!(system_mode, SystemMode::CGB_0 | SystemMode::CGB_A | SystemMode::CGB_B | SystemMode::CGB_C | SystemMode::CGB_D | SystemMode::CGB_E | SystemMode::AGB) {
            if (lf_div_for_alignment & 3) != 0 {
                self.dmg_delayed_start_timer = 6 - if lf_div_is_odd { 1 } else { 0 };
                if self.nr42.dac_power() { self.enabled = true; } else { self.enabled = false; }
                return;
            }
        }
        self.dmg_delayed_start_timer = 0;
        self.perform_trigger_core(system_mode, lf_div_for_alignment, current_frame_sequencer_step);
    }

    pub fn perform_trigger_core(&mut self, system_mode: SystemMode, lf_div_for_alignment: u8, current_frame_sequencer_step: u8) {
        if self.nr42.dac_power() { self.enabled = true; } else { self.enabled = false; return; }

        if self.length_counter == 0 {
            self.length_counter = 64;
            self.length_enabled_internal = false;
        }

        let r = self.nr43.clock_divider_val();
        let s = self.nr43.clock_shift();
        let divisor_val: u16 = if r == 0 { 8 } else { (r as u16) * 16 };
        self.sample_countdown = divisor_val << s;

        self.envelope_volume = self.nr42.initial_volume_val();
        let period_reg_val = self.nr42.envelope_period_val();
        self.envelope_period_timer = if period_reg_val == 0 { 8 } else { period_reg_val };

        self.countdown_was_reloaded_by_tick = false;

        let is_new_period_zero = period_reg_val == 0;
        let is_direction_increase = self.nr42.envelope_direction_is_increase();
        if !is_new_period_zero {
            self.envelope_clock_active = true;
            self.envelope_clock_should_lock = (self.envelope_volume == 0xF && is_direction_increase) ||
                                              (self.envelope_volume == 0x0 && !is_direction_increase);
        } else {
            self.envelope_clock_active = false;
            self.envelope_clock_should_lock = false;
        }
        self.envelope_clock_locked = false;

        self.lfsr = 0;
        self.current_lfsr_sample_is_on = false;
    }

    pub fn get_length_counter(&self) -> u16 { self.length_counter }

    pub fn extra_length_clock(&mut self, trigger_is_set_in_nrx4: bool) {
        if self.length_enabled_internal && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 && !trigger_is_set_in_nrx4 && !self.pulsed_on_trigger {
                self.enabled = false;
            }
        }
    }

    pub(super) fn get_envelope_volume(&self) -> u8 { self.envelope_volume }
    pub(super) fn set_envelope_volume(&mut self, vol: u8) { self.envelope_volume = vol & 0x0F; }
    pub(super) fn force_disable_channel(&mut self) { self.enabled = false; }

    pub fn clock_length(&mut self) {
        if self.length_enabled_internal && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 && !self.pulsed_on_trigger {
                self.enabled = false;
            }
        }
    }

    pub fn clock_envelope(&mut self) {
        if !self.envelope_clock_active || !self.nr42.dac_power() {
            return;
        }
        self.envelope_period_timer = self.envelope_period_timer.saturating_sub(1);

        if self.envelope_period_timer == 0 {
            let period_reg_val = self.nr42.envelope_period_val();
            self.envelope_period_timer = if period_reg_val == 0 { 8 } else { period_reg_val };

            if self.envelope_clock_locked {
                return;
            }

            let current_volume = self.envelope_volume;
            let is_increase = self.nr42.envelope_direction_is_increase();

            if is_increase {
                if current_volume < 15 {
                    self.envelope_volume += 1;
                }
            } else {
                if current_volume > 0 {
                    self.envelope_volume -= 1;
                }
            }
            if self.envelope_volume == 0 || self.envelope_volume == 15 {
                if period_reg_val != 0 {
                     self.envelope_clock_locked = true;
                }
            }
        }
    }

    pub fn tick(&mut self, system_mode: SystemMode, lf_div_for_alignment: u8, current_frame_sequencer_step: u8) {
        if self.dmg_delayed_start_timer > 0 {
            self.dmg_delayed_start_timer -= 1;
            if self.dmg_delayed_start_timer == 0 {
                self.perform_trigger_core(system_mode, lf_div_for_alignment, current_frame_sequencer_step);
            }
            return;
        }

        if !self.enabled { return; }

        self.sample_countdown = self.sample_countdown.saturating_sub(1);
        if self.sample_countdown == 0 {
            let r = self.nr43.clock_divider_val();
            let s = self.nr43.clock_shift();
            let divisor_val: u16 = if r == 0 { 8 } else { (r as u16) * 16 };
            self.sample_countdown = divisor_val << s;

            self.step_lfsr();
            self.pulsed_on_trigger = false;
            self.countdown_was_reloaded_by_tick = true;
        }
    }

    pub fn get_output_volume(&self) -> u8 {
        if !self.enabled || !self.nr42.dac_power() { return 0; }
        if self.current_lfsr_sample_is_on { self.envelope_volume } else { 0 }
    }

    pub(super) fn step_lfsr(&mut self) { // Made pub(super)
        let lfsr_val = self.lfsr;
        let xor_res = (lfsr_val & 1) ^ ((lfsr_val >> 1) & 1);
        self.lfsr = (lfsr_val >> 1) | (xor_res << 14);
        if self.lfsr_narrow_mode {
            self.lfsr = (self.lfsr & !(1 << 6)) | (xor_res << 6);
        }
        self.current_lfsr_sample_is_on = (self.lfsr & 1) == 0;
    }

    pub fn reload_length_on_enable(&mut self, current_frame_sequencer_step: u8) {
        let length_data = self.nr41.initial_length_timer_val();
        let is_max_length_condition_len = length_data == 0;
        let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };

        let fs_condition_met = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
        if fs_condition_met && self.nr44.is_length_enabled() && is_max_length_condition_len {
            actual_load_val_len = 63;
        }
        self.length_counter = actual_load_val_len;
    }
}
