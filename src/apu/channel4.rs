// src/apu/channel4.rs
use super::{Nr41, Nr42, Nr43, Nr44};

pub struct Channel4 {
    pub nr41: Nr41,
    pub nr42: Nr42,
    pub nr43: Nr43,
    pub nr44: Nr44,
    pub enabled: bool,
    length_counter: u16,
    // frequency_timer: u32, // Replaced by lfsr_step_countdown
    envelope_volume: u8,
    envelope_period_timer: u8,
    envelope_running: bool,
    lfsr: u16,
    initial_delay_countdown: u8,

    // New fields for SameBoy-aligned clocking
    lfsr_clock_divider_val: u8,  // Calculated from NR43_raw_divisor: 2,4,8,12,16,20,24,28
    lfsr_step_countdown: u32,    // Counts down APU clock ticks (was frequency_timer)
    lfsr_shift_amount: u8,       // NR43 s bits
    div_apu_counter: u32,        // Free-running counter, reset on trigger, clocked by lfsr_step_countdown
    noise_alignment_buffer: u8,  // Stores alignment effect from trigger time, TODO: full integration
}

impl Channel4 {
    pub fn new() -> Self {
        let mut new_ch4 = Self {
            nr41: Nr41::new(), nr42: Nr42::new(), nr43: Nr43::new(), nr44: Nr44::new(),
            enabled: false, length_counter: 0,
            envelope_volume: 0, envelope_period_timer: 0, envelope_running: false,
            lfsr: 0xFFFF, // Will be set to 0 on trigger
            initial_delay_countdown: 0,

            lfsr_clock_divider_val: 2, // Default (r=0 -> 2)
            lfsr_step_countdown: 2,    // Default
            lfsr_shift_amount: 0,      // Default
            div_apu_counter: 0,        // Default
            noise_alignment_buffer: 0, // Default
        };
        new_ch4.set_lfsr_clock_divider(new_ch4.nr43.clock_divider_val()); // Initialize based on default NR43
        new_ch4.lfsr_shift_amount = new_ch4.nr43.clock_shift();
        new_ch4
    }
    // Note: Channel4 doesn't have a power_on_reset method in the snippet,
    // initial_delay_countdown will be reset if the whole Apu/Channel4 is new'd.

    pub fn trigger(&mut self, current_frame_sequencer_step: u8, lf_div: u8, length_enabled_from_nrx4: bool) {
        let was_enabled_before_this_trigger = self.enabled;

        if self.nr42.dac_power() { self.enabled = true; }
        else { self.enabled = false; return; }

        if was_enabled_before_this_trigger {
            self.initial_delay_countdown = 4u8.saturating_sub(lf_div);
        } else {
            self.initial_delay_countdown = 6u8.saturating_sub(lf_div);
        }

        if self.length_counter == 0 {
            let length_data = self.nr41.initial_length_timer_val();
            let is_max_length_condition_len = length_data == 0;
            let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };
            let next_fs_step_will_not_clock_length = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
            // Use the length_enabled_from_nrx4 passed in
            if next_fs_step_will_not_clock_length && length_enabled_from_nrx4 && is_max_length_condition_len {
                actual_load_val_len = 63;
            }
            self.length_counter = actual_load_val_len;
        }

        // Update clocking parameters from NR43
        self.set_lfsr_clock_divider(self.nr43.clock_divider_val());
        self.lfsr_shift_amount = self.nr43.clock_shift();

        // Initialize lfsr_step_countdown (simplified for now)
        // SameBoy: noise_channel->counter_countdown = noise_channel->divisor + noise_channel->alignment + delayed_start_extra_countdown_value;
        // delayed_start_extra_countdown_value is lf_div for DMG (0/1), 0 for CGB.
        // alignment is (cycles_late + 1) & 3.
        // For now: base divisor + lf_div (assuming CGB, so lf_div part is 0, but structure for DMG)
        let delayed_start_val = if false /* TODO: is_dmg() */ { lf_div } else { 0 };
        self.lfsr_step_countdown = (self.lfsr_clock_divider_val as u32).saturating_add(delayed_start_val as u32);
        // self.noise_alignment_buffer could store the `alignment` part if complex trigger timing is added later.

        self.div_apu_counter = 0; // Reset free-running counter

        self.envelope_volume = self.nr42.initial_volume_val();
        let env_period_raw = self.nr42.envelope_period_val();
        let mut envelope_timer_load_val = if env_period_raw == 0 { 8 } else { env_period_raw };
        if current_frame_sequencer_step == 6 {
            envelope_timer_load_val += 1;
        }
        self.envelope_period_timer = envelope_timer_load_val;
        self.envelope_running = self.nr42.dac_power() && env_period_raw != 0;
        self.lfsr = 0; // Reset LFSR to 0 per SameBoy
        if !self.nr42.dac_power() { self.enabled = false; }
    }

    // New method to set lfsr_clock_divider_val from raw NR43 divider bits
    pub(super) fn set_lfsr_clock_divider(&mut self, raw_r_bits: u8) {
        // SameBoy logic for 'divisor': if r == 0, actual divisor is 2, otherwise r * 4.
        // These are for CGB. DMG is r * 8.
        // Assuming CGB for now:
        let r = raw_r_bits & 0x07;
        self.lfsr_clock_divider_val = if r == 0 { 2 } else { r * 4 };
        // For DMG, it would be: if r == 0 { 8 } else { r * 8 } essentially (but raw NR43 r=0 is 0x08 for timer)
        // Let's stick to the effective values SameBoy uses for counter_countdown reloads for CGB:
        // Divisor values for counter_countdown (from table, for CGB): 2, 4, 8, 12, 16, 20, 24, 28
        // These correspond to r = 0..7
        // if r=0, effective_divisor_for_countdown_reload = 2
        // if r=1, effective_divisor_for_countdown_reload = 4
        // if r=2, effective_divisor_for_countdown_reload = 8
        // if r=3, effective_divisor_for_countdown_reload = 12
        // if r=4, effective_divisor_for_countdown_reload = 16
        // if r=5, effective_divisor_for_countdown_reload = 20
        // if r=6, effective_divisor_for_countdown_reload = 24
        // if r=7, effective_divisor_for_countdown_reload = 28
        // This can be written as: if r == 0 { 2 } else { r * 4 }
        // This seems to be what SameBoy's `noise_channel->divisor` field stores.
    }

    // Getter for lfsr_shift_amount (used in apu.rs for NR43 write)
    pub(super) fn get_lfsr_shift_amount(&self) -> u8 {
        self.lfsr_shift_amount
    }
    // Setter for lfsr_shift_amount (used in apu.rs for NR43 write)
    pub(super) fn set_lfsr_shift_amount(&mut self, shift_amount: u8) {
        self.lfsr_shift_amount = shift_amount;
    }


    // fn update_frequency_timer(&mut self) { // This method is being replaced
    //     let r = self.nr43.clock_divider_val();
    //     let s = self.nr43.clock_shift();
    //     let divisor_val: u32 = if r == 0 { 8 } else { (r as u32) * 16 };
    //     self.frequency_timer = divisor_val << s;
    // }

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

        if self.lfsr_step_countdown > 0 {
            self.lfsr_step_countdown -= 1;
        }

        if self.lfsr_step_countdown == 0 {
            // Reload countdown (simplified for now, add alignment buffer later)
            self.lfsr_step_countdown = self.lfsr_clock_divider_val as u32;
            // TODO: Add self.noise_alignment_buffer logic here if/when fully implemented

            let old_div_apu_bit = (self.div_apu_counter >> self.lfsr_shift_amount) & 1;
            self.div_apu_counter = self.div_apu_counter.wrapping_add(1);
            let new_div_apu_bit = (self.div_apu_counter >> self.lfsr_shift_amount) & 1;

            // LFSR is clocked on the rising edge of (div_apu_counter >> shift_amount)
            // This also implicitly handles shift_amount >= 14 making the bit always 0, so no rising edge.
            if new_div_apu_bit == 1 && old_div_apu_bit == 0 {
                // LFSR stepping logic (already corrected in previous step)
                let feedback_bit = ((self.lfsr & 0x0001) ^ ((self.lfsr >> 1) & 0x0001)) ^ 1;
                self.lfsr >>= 1;
                self.lfsr = (self.lfsr & !(1 << 14)) | (feedback_bit << 14);
                if self.nr43.lfsr_width_is_7bit() { // nr43 field still needed for this
                    self.lfsr = (self.lfsr & !(1 << 6)) | (feedback_bit << 6);
                }
            }
        }
    }

    pub fn get_output_volume(&mut self) -> u8 { // Made &mut self due to initial_delay_countdown
        if self.initial_delay_countdown > 0 {
            self.initial_delay_countdown -= 1;
            return 0;
        }
        if !self.enabled || !self.nr42.dac_power() { return 0; }
        // 3. Output based on inverted bit 0 of LFSR
        if (self.lfsr & 0x0001) != 0 { self.envelope_volume } else { 0 }
    }

    // In src/apu/channel4.rs, within impl Channel4
    // pub fn reload_length_on_enable(&mut self, current_frame_sequencer_step: u8) { // Now unused
    //     let length_data = self.nr41.initial_length_timer_val(); // 0-63
    //     let is_max_length_condition_len = length_data == 0;
    //     let mut actual_load_val_len = if is_max_length_condition_len { 64 } else { 64 - length_data as u16 };

    //     let fs_condition_met = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
    //     // self.nr44.is_length_enabled() should be true
    //     if fs_condition_met && self.nr44.is_length_enabled() && is_max_length_condition_len {
    //         actual_load_val_len = 63;
    //     }
    //     self.length_counter = actual_load_val_len;
    // }
}
