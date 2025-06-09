// src/apu/channel1.rs
use super::{Nr10, Nr11, Nr12, Nr13, Nr14};
use crate::bus::SystemMode; // Moved to top

pub struct Channel1 {
    pub nr10: Nr10,
    pub nr11: Nr11,
    pub nr12: Nr12,
    pub nr13: Nr13,
    pub nr14: Nr14,
    pub enabled: bool,
    length_counter: u16,
    frequency_timer: u16,
    duty_step: u8,
    pub(super) envelope_volume: u8, // Made pub(super)
    pub(super) envelope_period_timer: u8, // Made pub(super)
    envelope_running: bool, // Keep as is, managed by pub(super) is_envelope_running
    sweep_period_timer: u8,
    pub(super) sweep_shadow_frequency: u16, // Made pub(super)
    sweep_enabled: bool,
    sweep_calculated_overflow_this_step: bool,
    has_been_triggered_since_power_on: bool,
    pub(super) force_output_zero_for_next_sample: bool, // Made pub(super)
    pub(super) subtraction_sweep_calculated_since_trigger: bool, // Made pub(super)

    // New fields for Sameboy sweep model
    pub(super) sweep_calculate_countdown: u8, // Made pub(super)
    pub(super) sweep_calculate_countdown_reload_timer: u8, // Made pub(super)
    #[doc(hidden)] // Hiding from docs for now as it's an internal detail
    pub(super) pulsed_on_trigger: bool, // Added pub(super)
    pub(super) sweep_instant_calculation_done: bool, // Made pub(super)
    pub(super) unshifted_sweep: bool, // Made pub(super)
    pub(super) sweep_length_addend: u16, // Made pub(super)
    pub(super) channel1_completed_addend: u16, // Made pub(super)
    pub(super) channel_1_restart_hold: u8, // Made pub(super)
    pub(super) sweep_neg_calculation_occurred_on_trigger: bool, // Made pub(super)

    // Fields for NRx2 envelope glitch logic
    pub envelope_clock_active: bool,
    pub envelope_clock_should_lock: bool,
    pub envelope_clock_locked: bool,

    // Missing field from previous plan
    pub(super) length_enabled_internal: bool, // Added and made pub(super)
    // pulsed_on_trigger: bool, // Already added above with pub(super)
}

impl Channel1 {
    pub fn new() -> Self {
        Self {
            nr10: Nr10::new(), nr11: Nr11::new(), nr12: Nr12::new(), nr13: Nr13::new(), nr14: Nr14::new(),
            enabled: false, length_counter: 0, frequency_timer: 0, duty_step: 0,
            envelope_volume: 0, envelope_period_timer: 0, envelope_running: false,
            sweep_period_timer: 0, sweep_shadow_frequency: 0, sweep_enabled: false,
            sweep_calculated_overflow_this_step: false,
            has_been_triggered_since_power_on: false, force_output_zero_for_next_sample: false,
            subtraction_sweep_calculated_since_trigger: false,
            pulsed_on_trigger: false, // Initialize added field

            sweep_calculate_countdown: 0,
            sweep_calculate_countdown_reload_timer: 0,
            sweep_instant_calculation_done: false,
            unshifted_sweep: false,
            sweep_length_addend: 0,
            channel1_completed_addend: 0,
            channel_1_restart_hold: 0,
            sweep_neg_calculation_occurred_on_trigger: false,

            envelope_clock_active: false,
            envelope_clock_should_lock: false,
            envelope_clock_locked: false,
            length_enabled_internal: false, // Initialize added field
            // pulsed_on_trigger initialized above
        }
    }

    pub fn power_on_reset(&mut self) {
        self.has_been_triggered_since_power_on = false;
        self.force_output_zero_for_next_sample = false;
        self.subtraction_sweep_calculated_since_trigger = false;
        self.pulsed_on_trigger = false; // Reset added field
        self.sweep_calculate_countdown = 0;
        self.sweep_calculate_countdown_reload_timer = 0;
        self.sweep_instant_calculation_done = false;
        self.unshifted_sweep = false;
        self.sweep_length_addend = 0;
        self.channel1_completed_addend = 0;
        self.channel_1_restart_hold = 0;
        self.sweep_neg_calculation_occurred_on_trigger = false;
        self.envelope_clock_active = false;
        self.envelope_clock_should_lock = false;
        self.envelope_clock_locked = false;
        self.length_enabled_internal = false;
        // pulsed_on_trigger reset above
    }

    #[allow(unused_variables)]
    pub(super) fn trigger_sweep_calculation(&mut self, system_mode: crate::bus::SystemMode, lf_div_is_odd: bool) {
        let sweep_shift = self.nr10.sweep_shift_val();
        if sweep_shift != 0 {
            self.sweep_length_addend = self.sweep_shadow_frequency >> sweep_shift;
        }

        self.sweep_calculate_countdown = self.nr10.sweep_period();
        if self.sweep_calculate_countdown == 0 && system_mode != SystemMode::DMG {
            self.sweep_calculate_countdown = 8;
        }
        self.sweep_calculate_countdown_reload_timer = self.sweep_calculate_countdown;
        self.unshifted_sweep = sweep_shift == 0;
        self.sweep_instant_calculation_done = false;
    }

    #[allow(unused_variables)]
    pub(super) fn sweep_calculation_done(&mut self, system_mode: crate::bus::SystemMode, lf_div_is_odd: bool) { // Made pub(super)
        self.sweep_instant_calculation_done = true;
        let sweep_shift = self.nr10.sweep_shift_val();

        if !self.unshifted_sweep {
            let new_frequency;
            if !self.nr10.sweep_direction_is_increase() {
                new_frequency = self.sweep_shadow_frequency.wrapping_sub(self.sweep_length_addend);
                self.channel1_completed_addend = 0xFFFF;
                self.subtraction_sweep_calculated_since_trigger = true;
            } else {
                new_frequency = self.sweep_shadow_frequency.wrapping_add(self.sweep_length_addend);
                self.channel1_completed_addend = 0;
            }

            if new_frequency > 0x7FF {
                self.enabled = false;
                self.sweep_calculated_overflow_this_step = true;
                if !self.nr10.sweep_direction_is_increase() {
                    self.sweep_neg_calculation_occurred_on_trigger = true;
                }
            } else {
                self.sweep_shadow_frequency = new_frequency;
                self.nr13.write((new_frequency & 0xFF) as u8);
                self.nr14.write_frequency_msb(((new_frequency >> 8) & 0x07) as u8);
                self.sweep_calculated_overflow_this_step = false;
            }
        }
    }

    pub fn trigger(&mut self, current_frame_sequencer_step: u8) {
        self.subtraction_sweep_calculated_since_trigger = false;
        self.sweep_neg_calculation_occurred_on_trigger = false;

        if !self.has_been_triggered_since_power_on {
            self.force_output_zero_for_next_sample = true;
            self.has_been_triggered_since_power_on = true;
        }

        if self.nr12.dac_power() { self.enabled = true; } else { self.enabled = false; return; }

        if self.length_counter == 0 {
            let length_data = self.nr11.initial_length_timer_val();
            let is_max_length_condition_len = length_data == 0;
            let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };
            let next_fs_step_will_not_clock_length = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
            // self.length_enabled_internal should be used here if NR14's bit is what it means
            // For trigger, it's NR14.length_enable that matters for this specific glitch.
            // The length_enabled_internal is for APU write to NR14.
            let length_is_enabled_on_trigger = self.nr14.is_length_enabled();
            if next_fs_step_will_not_clock_length && length_is_enabled_on_trigger && is_max_length_condition_len {
                actual_load_val_len = 63;
            }
            self.length_counter = actual_load_val_len;
        }

        let old_low_two_bits = self.frequency_timer & 0b11;
        let freq_lsb = self.nr13.freq_lo_val() as u16;
        let freq_msb = self.nr14.frequency_msb_val() as u16;
        let period_val = (freq_msb << 8) | freq_lsb;
        self.frequency_timer = (2048 - period_val) * 4;
        self.frequency_timer = (self.frequency_timer & !0b11) | old_low_two_bits;
        self.duty_step = 0;

        self.envelope_volume = self.nr12.initial_volume_val();
        let period_reg_val = self.nr12.envelope_period_val();
        self.envelope_period_timer = if period_reg_val == 0 { 8 } else { period_reg_val };

        let is_new_period_zero = period_reg_val == 0;
        let is_direction_increase = self.nr12.envelope_direction_is_increase();

        if !is_new_period_zero {
            self.envelope_clock_active = true;
            self.envelope_clock_should_lock = (self.envelope_volume == 0xF && is_direction_increase) ||
                                              (self.envelope_volume == 0x0 && !is_direction_increase);
        } else {
            self.envelope_clock_active = false;
            self.envelope_clock_should_lock = false;
        }
        self.envelope_clock_locked = false;
        self.envelope_running = self.envelope_clock_active && self.nr12.dac_power();

        self.sweep_shadow_frequency = period_val;
        self.sweep_enabled = self.nr10.sweep_period() != 0 || self.nr10.sweep_shift_val() != 0;
        self.sweep_calculated_overflow_this_step = false;

        self.channel_1_restart_hold = 8;
        self.sweep_instant_calculation_done = false;
        self.unshifted_sweep = false;

        if self.sweep_enabled && self.nr10.sweep_shift_val() > 0 {
            let new_freq = self.calculate_sweep_frequency();
            if new_freq > 2047 {
                self.enabled = false;
                self.sweep_calculated_overflow_this_step = true;
                if !self.nr10.sweep_direction_is_increase() {
                    self.sweep_neg_calculation_occurred_on_trigger = true;
                }
            }
        }
    }

    fn calculate_sweep_frequency(&self) -> u16 {
        let delta = self.sweep_shadow_frequency >> self.nr10.sweep_shift_val();
        if self.nr10.sweep_direction_is_increase() { self.sweep_shadow_frequency.saturating_add(delta) }
        else { self.sweep_shadow_frequency.saturating_sub(delta) }
    }

    pub fn get_length_counter(&self) -> u16 { self.length_counter }

    pub fn extra_length_clock(&mut self, trigger_is_set_in_nrx4: bool) {
        if self.length_enabled_internal && self.length_counter > 0 { // Check internal flag
            self.length_counter -= 1;
            if self.length_counter == 0 && !trigger_is_set_in_nrx4 && !self.pulsed_on_trigger { // pulsed_on_trigger might be relevant
                self.enabled = false;
            }
        }
    }

    pub fn clock_length(&mut self) {
        if self.nr14.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 { self.enabled = false; }
        }
    }

    pub fn clock_envelope(&mut self) {
        if !self.envelope_clock_active || !self.nr12.dac_power() {
            return;
        }
        self.envelope_period_timer = self.envelope_period_timer.saturating_sub(1);

        if self.envelope_period_timer == 0 {
            let period_reg_val = self.nr12.envelope_period_val();
            self.envelope_period_timer = if period_reg_val == 0 { 8 } else { period_reg_val };

            if self.envelope_clock_locked {
                return;
            }

            let current_volume = self.envelope_volume;
            let is_increase = self.nr12.envelope_direction_is_increase();

            if is_increase { if current_volume < 15 { self.envelope_volume += 1; } }
            else { if current_volume > 0 { self.envelope_volume -= 1; } }

            if self.envelope_volume == 0 || self.envelope_volume == 15 {
                if period_reg_val != 0 { self.envelope_clock_locked = true; }
            }
        }
    }

    pub fn clock_sweep_fs_tick(&mut self, system_mode: SystemMode, lf_div_is_odd: bool) {
        if !self.sweep_enabled || !self.nr12.dac_power() {
            return;
        }

        if self.sweep_calculate_countdown > 0 {
            self.sweep_calculate_countdown -= 1;
        }

        if self.sweep_calculate_countdown == 0 {
            self.sweep_calculate_countdown = self.sweep_calculate_countdown_reload_timer;
            if self.sweep_calculate_countdown == 0 && system_mode != SystemMode::DMG {
                self.sweep_calculate_countdown = 8;
            }

            if self.nr10.sweep_period() > 0 {
                self.trigger_sweep_calculation(system_mode, lf_div_is_odd);
                if !self.sweep_instant_calculation_done {
                    self.sweep_calculation_done(system_mode, lf_div_is_odd);
                }
            }
        }
    }

    pub(super) fn has_subtraction_sweep_calculated(&self) -> bool { self.subtraction_sweep_calculated_since_trigger }
    pub(super) fn disable_for_sweep_bug(&mut self) { self.enabled = false; }
    pub(super) fn is_envelope_running(&self) -> bool { self.envelope_running }
    pub(super) fn get_envelope_volume(&self) -> u8 { self.envelope_volume } // Already pub(super)
    pub(super) fn set_envelope_volume(&mut self, vol: u8) { self.envelope_volume = vol & 0x0F; }
    pub(super) fn force_disable_channel(&mut self) { self.enabled = false; }

    pub fn tick(&mut self) {
        if !self.enabled { return; }
        self.frequency_timer = self.frequency_timer.saturating_sub(1);
        if self.frequency_timer == 0 {
            let freq_lsb = self.nr13.freq_lo_val() as u16;
            let freq_msb = self.nr14.frequency_msb_val() as u16;
            let period_val = (freq_msb << 8) | freq_lsb;
            self.frequency_timer = (2048 - period_val) * 4;
            if self.has_been_triggered_since_power_on {
                self.duty_step = (self.duty_step + 1) % 8;
            }
        }
    }

    pub fn get_output_volume(&mut self) -> u8 {
        if self.force_output_zero_for_next_sample { self.force_output_zero_for_next_sample = false; return 0; }
        if !self.enabled || !self.nr12.dac_power() || self.sweep_calculated_overflow_this_step { return 0; }
        let wave_duty = self.nr11.wave_pattern_duty_val();
        let wave_output = match wave_duty {
            0b00 => [0,0,0,0,0,0,0,1][self.duty_step as usize],
            0b01 => [1,0,0,0,0,0,0,1][self.duty_step as usize],
            0b10 => [1,0,0,0,0,1,1,1][self.duty_step as usize],
            0b11 => [0,1,1,1,1,1,1,0][self.duty_step as usize],
            _ => 0,
        };
        if wave_output == 1 { self.envelope_volume } else { 0 }
    }

    pub fn reload_length_on_enable(&mut self, current_frame_sequencer_step: u8) {
        let length_data = self.nr11.initial_length_timer_val();
        let is_max_length_condition_len = length_data == 0;
        let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };
        let fs_condition_met = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
        if fs_condition_met && self.nr14.is_length_enabled() && is_max_length_condition_len {
            actual_load_val_len = 63;
        }
        self.length_counter = actual_load_val_len;
    }
}
// Removed duplicate: use crate::bus::SystemMode;
