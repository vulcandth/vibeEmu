// src/apu/channel1.rs
use super::{Nr10, Nr11, Nr12, Nr13, Nr14};

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
    envelope_volume: u8,
    envelope_period_timer: u8,
    envelope_running: bool,
    sweep_calculate_countdown: u8, // Renamed from sweep_period_timer
    sweep_shadow_frequency: u16,
    sweep_enabled: bool,
    sweep_calculated_overflow_this_step: bool,
    has_been_triggered_since_power_on: bool,
    force_output_zero_for_next_sample: bool,
    subtraction_sweep_calculated_since_trigger: bool,
    initial_delay_countdown: u8, // For SameBoy's trigger initial sample delay
}

impl Channel1 {
    pub fn new() -> Self {
        Self {
            nr10: Nr10::new(), nr11: Nr11::new(), nr12: Nr12::new(), nr13: Nr13::new(), nr14: Nr14::new(),
            enabled: false, length_counter: 0, frequency_timer: 0, duty_step: 0,
            envelope_volume: 0, envelope_period_timer: 0, envelope_running: false,
            sweep_calculate_countdown: 0, sweep_shadow_frequency: 0, sweep_enabled: false,
            sweep_calculated_overflow_this_step: false,
            has_been_triggered_since_power_on: false, force_output_zero_for_next_sample: false,
            subtraction_sweep_calculated_since_trigger: false,
            initial_delay_countdown: 0,
        }
    }

    pub fn power_on_reset(&mut self) {
        self.has_been_triggered_since_power_on = false;
        self.force_output_zero_for_next_sample = false;
        self.subtraction_sweep_calculated_since_trigger = false;
        self.initial_delay_countdown = 0;
    }

    // lf_div is (main_clock_cycles / 16) & 1, effectively. (0 or 1)
    pub fn trigger(&mut self, current_frame_sequencer_step: u8, lf_div: u8) {
        let was_enabled_before_this_trigger = self.enabled; // Check before self.enabled might be set true

        self.subtraction_sweep_calculated_since_trigger = false;
        if !self.has_been_triggered_since_power_on {
            self.force_output_zero_for_next_sample = true; // This is a separate power-on specific quirk
            self.has_been_triggered_since_power_on = true;
        }

        // Standard DAC power check; if off, channel doesn't enable.
        if self.nr12.dac_power() { self.enabled = true; } else { self.enabled = false; return; }

        // SameBoy's initial sample delay logic
        // delay = (was_active ? 4 : 6) - lf_div;
        // lf_div is 0 or 1. If lf_div is 1, delay is shorter.
        if was_enabled_before_this_trigger {
            self.initial_delay_countdown = 4u8.saturating_sub(lf_div);
        } else {
            self.initial_delay_countdown = 6u8.saturating_sub(lf_div);
        }
        // Ensure countdown is at least 1 if it becomes 0 due to lf_div, though SameBoy uses it as is.
        // E.g. 4-1 = 3 samples delayed. If initial_delay_countdown is 0, no delay.

        if self.length_counter == 0 {
            let length_data = self.nr11.initial_length_timer_val();
            let is_max_length_condition_len = length_data == 0;
            let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };
            let next_fs_step_will_not_clock_length = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
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
        let env_period_raw = self.nr12.envelope_period_val();
        let mut envelope_timer_load_val = if env_period_raw == 0 { 8 } else { env_period_raw };
        if current_frame_sequencer_step == 6 {
            envelope_timer_load_val += 1;
        }
        self.envelope_period_timer = envelope_timer_load_val;
        self.envelope_running = self.nr12.dac_power() && env_period_raw != 0;
        self.sweep_shadow_frequency = period_val;
        let sweep_period_raw = self.nr10.sweep_period();
        // Initialize sweep_calculate_countdown
        self.sweep_calculate_countdown = if sweep_period_raw == 0 { 8 } else { sweep_period_raw };
        self.sweep_enabled = sweep_period_raw != 0 || self.nr10.sweep_shift_val() != 0;
        self.sweep_calculated_overflow_this_step = false; // Reset this flag on trigger
        if self.sweep_enabled && self.nr10.sweep_shift_val() != 0 {
            let new_freq = self.calculate_sweep_frequency();
            if new_freq > 2047 { self.enabled = false; self.sweep_calculated_overflow_this_step = true; }
        }
    }

    fn calculate_sweep_frequency(&self) -> u16 {
        let delta = self.sweep_shadow_frequency >> self.nr10.sweep_shift_val();
        if self.nr10.sweep_direction_is_increase() { self.sweep_shadow_frequency.saturating_add(delta) }
        else { self.sweep_shadow_frequency.saturating_sub(delta) }
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
        if self.nr14.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 { self.enabled = false; }
        }
    }

    pub fn clock_envelope(&mut self) {
        if !self.envelope_running || !self.nr12.dac_power() { return; }
        let env_period_raw = self.nr12.envelope_period_val();
        if env_period_raw == 0 { self.envelope_running = false; return; }
        self.envelope_period_timer -= 1;
        if self.envelope_period_timer == 0 {
            self.envelope_period_timer = env_period_raw;
            let current_volume = self.envelope_volume;
            if self.nr12.envelope_direction_is_increase() {
                if current_volume < 15 { self.envelope_volume += 1; }
            } else {
                if current_volume > 0 { self.envelope_volume -= 1; }
            }
            if self.envelope_volume == 0 || self.envelope_volume == 15 { self.envelope_running = false; }
        }
    }

    pub fn clock_sweep(&mut self) {
        if !self.sweep_enabled || !self.nr12.dac_power() { return; }

        if self.sweep_calculate_countdown > 0 {
            self.sweep_calculate_countdown -= 1;
        }

        if self.sweep_calculate_countdown == 0 {
            let sweep_period_raw = self.nr10.sweep_period();
            self.sweep_calculate_countdown = if sweep_period_raw == 0 { 8 } else { sweep_period_raw };

            if !self.enabled { return; } // Don't run if channel got disabled by other means

            // If the sweep period is zero, the sweep timer is reloaded with 8,
            // but the sweep calculation doesn't occur.
            if sweep_period_raw == 0 {
                return;
            }

            // Only if sweep is enabled and shift is non-zero can frequency change
            // and overflow checks/updates happen.
            // However, the subtraction flag might need to be set even if shift is 0,
            // if a calculation was *attempted* with subtract direction.
            // But sweep calculation only has effect if shift > 0.
            // So, if shift is 0, new_freq is same as shadow_freq.

            // Set subtraction flag before shadow frequency potentially changes
            // This flag is for the NR10 negate bug.
            if self.nr10.sweep_shift_val() != 0 && !self.nr10.sweep_direction_is_increase() {
                self.subtraction_sweep_calculated_since_trigger = true;
            }

            let new_freq = self.calculate_sweep_frequency();

            if new_freq > 2047 {
                self.enabled = false;
                self.sweep_calculated_overflow_this_step = true;
                return;
            }

            // If sweep shift is 0, the frequency doesn't change.
            // No need to update registers or do the second overflow check.
            if self.nr10.sweep_shift_val() == 0 {
                // self.sweep_calculated_overflow_this_step = false; // Do not clear if set by trigger
                return;
            }

            // At this point: shift > 0 and new_freq <= 2047
            self.sweep_shadow_frequency = new_freq;
            self.nr13.write((new_freq & 0xFF) as u8);
            self.nr14.write_frequency_msb(((new_freq >> 8) & 0x07) as u8);

            // A successful calculation and update happened, so clear the flag for this step
            self.sweep_calculated_overflow_this_step = false;

            // Perform the second overflow check using the new shadow frequency
            let final_check_freq = self.calculate_sweep_frequency();
            if final_check_freq > 2047 {
                self.enabled = false;
                // self.sweep_calculated_overflow_this_step = true; // Channel is disabled, output will be 0.
                                                                // No need to set this again explicitly as primary effect is disabling.
            }
        }
    }

    // Getter for sweep_shadow_frequency needed for NR10 negate glitch logic in apu.rs
    pub(super) fn get_sweep_shadow_frequency(&self) -> u16 { self.sweep_shadow_frequency }
    // pub(super) fn has_subtraction_sweep_calculated(&self) -> bool { self.subtraction_sweep_calculated_since_trigger } // Unused
    // pub(super) fn disable_for_sweep_bug(&mut self) { self.enabled = false; } // Unused

    pub(super) fn is_envelope_running(&self) -> bool { self.envelope_running }
    pub(super) fn get_envelope_volume(&self) -> u8 { self.envelope_volume }
    pub(super) fn set_envelope_volume(&mut self, vol: u8) { self.envelope_volume = vol & 0x0F; }
    // pub(super) fn get_envelope_period_timer(&self) -> u8 { self.envelope_period_timer } // Unused
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
        // Initial trigger delay countdown
        if self.initial_delay_countdown > 0 {
            self.initial_delay_countdown -= 1;
            return 0;
        }

        // Power-on specific first sample quirk
        if self.force_output_zero_for_next_sample {
            self.force_output_zero_for_next_sample = false;
            return 0;
        }

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
        // Channel is not explicitly enabled here by just loading length,
        // its status depends on trigger or if it was already enabled.
        // DAC power is a prerequisite for sound output, not for length counter loading.

        let length_data = self.nr11.initial_length_timer_val(); // 0-63
        let is_max_length_condition_len = length_data == 0;
        let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };

        // Apply the "set to 63 instead of 64" obscure behavior.
        // Condition: Next Frame Sequencer step doesn't clock length (current_frame_sequencer_step is 0, 2, 4, or 6),
        // AND length is enabled in NR14, AND NR11's length data was 0 (meaning max length).
        let fs_condition_met = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
        // self.nr14.is_length_enabled() should be true if this path is taken from apu.rs
        if fs_condition_met && self.nr14.is_length_enabled() && is_max_length_condition_len {
            actual_load_val_len = 63;
        }
        self.length_counter = actual_load_val_len;
    }
}
