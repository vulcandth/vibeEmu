// src/apu/channel4.rs
use super::{Nr41, Nr42, Nr43, Nr44};
use crate::models::GameBoyModel;

pub struct Channel4 {
    pub nr41: Nr41,
    pub nr42: Nr42,
    pub nr43: Nr43,
    pub nr44: Nr44,
    pub enabled: bool,
    length_counter: u16,
    envelope_volume: u8,
    envelope_period_timer: u8,
    envelope_running: bool,
    lfsr: u16,
    initial_delay_countdown: u8,

    lfsr_clock_divider_val: u8,
    lfsr_step_countdown: u32,
    lfsr_shift_amount: u8,
    div_apu_counter: u32,
    // noise_alignment_buffer: u8, // Removed as per warning: field is never read

    pub dmg_delayed_start_countdown: u8,
    force_narrow_lfsr_for_glitch: bool,
}

impl Channel4 {
    pub fn new() -> Self {
        let mut new_ch4 = Self {
            nr41: Nr41::new(),
            nr42: Nr42::new(),
            nr43: Nr43::new(),
            nr44: Nr44::new(),
            enabled: false,
            length_counter: 0,
            envelope_volume: 0,
            envelope_period_timer: 0,
            envelope_running: false,
            lfsr: 0xFFFF,
            initial_delay_countdown: 0,
            lfsr_clock_divider_val: 2,
            lfsr_step_countdown: 2,
            lfsr_shift_amount: 0,
            div_apu_counter: 0,
            // noise_alignment_buffer: 0, // Removed as per warning: field is never read
            dmg_delayed_start_countdown: 0,
            force_narrow_lfsr_for_glitch: false,
        };
        new_ch4.set_lfsr_clock_divider_from_raw(new_ch4.nr43.clock_divider_val());
        new_ch4.lfsr_shift_amount = new_ch4.nr43.clock_shift();
        new_ch4
    }

    pub fn trigger(
        &mut self,
        current_frame_sequencer_step: u8,
        lf_div: u8,
        model: GameBoyModel,
        current_alignment: u32,
        length_enabled_from_nrx4: bool,
    ) {
        let was_enabled_before_this_trigger = self.enabled;

        if self.nr42.dac_power() {
            self.enabled = true;
        } else {
            self.enabled = false;
            return;
        }

        if was_enabled_before_this_trigger {
            self.initial_delay_countdown = 4u8.saturating_sub(lf_div);
        } else {
            self.initial_delay_countdown = 6u8.saturating_sub(lf_div);
        }

        if model.is_dmg_family() && (current_alignment & 3) != 0 {
            self.dmg_delayed_start_countdown = 6;
        } else {
            self.dmg_delayed_start_countdown = 0;
        }

        if self.length_counter == 0 {
            let length_data = self.nr41.initial_length_timer_val();
            let is_max_length_condition_len = length_data == 0;
            let mut actual_load_val_len = if is_max_length_condition_len {
                64
            } else {
                64 - length_data as u16
            };
            let next_fs_step_will_not_clock_length =
                matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
            if next_fs_step_will_not_clock_length
                && length_enabled_from_nrx4
                && is_max_length_condition_len
            {
                actual_load_val_len = 63;
            }
            self.length_counter = actual_load_val_len;
        }

        self.set_lfsr_clock_divider_from_raw(self.nr43.clock_divider_val());
        self.lfsr_shift_amount = self.nr43.clock_shift();

        let r_val = self.nr43.clock_divider_val();
        let divisor: i32 = if r_val == 0 { 8 } else { (r_val as i32) * 16 }; // MUT REMOVED HERE

        let mut countdown: i32 = divisor + 16;

        if divisor == 8 {
            if model.is_cgb_c_or_older() {
                countdown += (lf_div as i32) * 4;
            } else {
                countdown += (1 - lf_div as i32) * 4;
            }
        } else {
            // Placeholder for complex alignment
        }

        self.lfsr_step_countdown = if countdown > 0 { countdown as u32 } else { 4 };

        self.div_apu_counter = 0;
        self.lfsr = 0xFFFF;

        self.envelope_volume = self.nr42.initial_volume_val();
        let env_period_raw = self.nr42.envelope_period_val();
        let mut envelope_timer_load_val = if env_period_raw == 0 {
            8
        } else {
            env_period_raw
        };
        if current_frame_sequencer_step == 6 {
            envelope_timer_load_val += 1;
        }
        self.envelope_period_timer = envelope_timer_load_val;
        self.envelope_running = self.nr42.dac_power() && env_period_raw != 0;
        if !self.nr42.dac_power() {
            self.enabled = false;
        }
    }

    pub(super) fn set_lfsr_clock_divider_from_raw(&mut self, raw_r_bits: u8) {
        let r = raw_r_bits & 0x07;
        self.lfsr_clock_divider_val = match r {
            0 => 2,
            1 => 4,
            2 => 8,
            3 => 12,
            4 => 16,
            5 => 20,
            6 => 24,
            _ => 28,
        };
    }

    pub(super) fn get_lfsr_shift_amount(&self) -> u8 {
        self.lfsr_shift_amount
    }
    pub(super) fn set_lfsr_shift_amount(&mut self, shift_amount: u8) {
        self.lfsr_shift_amount = shift_amount;
    }
    pub(super) fn get_div_apu_counter(&self) -> u32 {
        self.div_apu_counter
    }
    pub(super) fn set_force_narrow_lfsr_for_glitch(&mut self, val: bool) {
        self.force_narrow_lfsr_for_glitch = val;
    }

    pub fn get_length_counter(&self) -> u16 {
        self.length_counter
    }

    pub fn extra_length_clock(&mut self, trigger_is_set_in_nrx4: bool) {
        if self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 && !trigger_is_set_in_nrx4 {
                self.enabled = false;
            }
        }
    }

    pub(super) fn is_envelope_running(&self) -> bool {
        self.envelope_running
    }
    pub(super) fn get_envelope_volume(&self) -> u8 {
        self.envelope_volume
    }
    pub(super) fn set_envelope_volume(&mut self, vol: u8) {
        self.envelope_volume = vol & 0x0F;
    }
    pub(super) fn force_disable_channel(&mut self) {
        self.enabled = false;
    }

    pub fn clock_length(&mut self) {
        if self.nr44.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

    pub fn clock_envelope(&mut self) {
        if !self.envelope_running || !self.nr42.dac_power() {
            return;
        }
        let env_period_raw = self.nr42.envelope_period_val();
        if env_period_raw == 0 {
            self.envelope_running = false;
            return;
        }
        self.envelope_period_timer -= 1;
        if self.envelope_period_timer == 0 {
            self.envelope_period_timer = env_period_raw;
            let current_volume = self.envelope_volume;
            if self.nr42.envelope_direction_is_increase() {
                if current_volume < 15 {
                    self.envelope_volume += 1;
                }
            } else {
                if current_volume > 0 {
                    self.envelope_volume -= 1;
                }
            }
            if self.envelope_volume == 0 || self.envelope_volume == 15 {
                self.envelope_running = false;
            }
        }
    }

    pub(super) fn step_lfsr(&mut self) {
        let use_narrow_mode = if self.force_narrow_lfsr_for_glitch {
            true
        } else {
            self.nr43.lfsr_width_is_7bit()
        };
        let feedback_bit = ((self.lfsr & 0x0001) ^ ((self.lfsr >> 1) & 0x0001)) ^ 1;
        self.lfsr >>= 1;
        self.lfsr = (self.lfsr & !(1 << 14)) | (feedback_bit << 14);
        if use_narrow_mode {
            self.lfsr = (self.lfsr & !(1 << 6)) | (feedback_bit << 6);
        }
    }

    pub fn tick(&mut self) {
        if !self.enabled {
            return;
        }

        if self.lfsr_step_countdown > 0 {
            self.lfsr_step_countdown -= 1;
        }

        if self.lfsr_step_countdown == 0 {
            self.lfsr_step_countdown = (self.lfsr_clock_divider_val as u32) * 4;

            let old_div_apu_bit = (self.div_apu_counter >> self.lfsr_shift_amount) & 1;
            self.div_apu_counter = self.div_apu_counter.wrapping_add(1);
            let new_div_apu_bit = (self.div_apu_counter >> self.lfsr_shift_amount) & 1;

            if new_div_apu_bit == 1 && old_div_apu_bit == 0 {
                self.step_lfsr();
            }
        }
    }

    pub fn get_output_volume(&mut self) -> u8 {
        if self.initial_delay_countdown > 0 {
            self.initial_delay_countdown -= 1;
            return 0;
        }
        if !self.enabled || !self.nr42.dac_power() {
            return 0;
        }
        if (self.lfsr & 0x0001) != 0 {
            0
        } else {
            self.envelope_volume
        }
    }
}
