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
    wave_form_just_read: bool,
    pulsed: bool, // For CGB-E specific retrigger behavior
    bugged_read_countdown: u8, // For CGB-E specific retrigger behavior
}

impl Channel3 {
    pub fn new() -> Self {
        Self {
            nr30: Nr30::new(), nr31: Nr31::new(), nr32: Nr32::new(), nr33: Nr33::new(), nr34: Nr34::new(),
            enabled: false, length_counter: 0, frequency_timer: 0, sample_index: 0, current_sample_buffer: 0,
            wave_form_just_read: false,
            pulsed: false,
            bugged_read_countdown: 0,
        }
    }

    pub fn trigger(&mut self, wave_ram: &[u8;16], current_frame_sequencer_step: u8, length_enabled_from_nrx4: bool) {
        self.enabled = self.nr30.dac_on();
        self.wave_form_just_read = false;
        self.pulsed = true; // Set on trigger per SameBoy

        if self.length_counter == 0 {
            let length_data = self.nr31.sound_length_val();
            let is_max_length_condition = length_data == 0;
            let mut actual_load_val = if is_max_length_condition { 256 } else { 256 - (length_data as u16) };
            let next_fs_step_will_not_clock_length = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
            // Use the length_enabled_from_nrx4 passed in
            if next_fs_step_will_not_clock_length && length_enabled_from_nrx4 && is_max_length_condition {
                actual_load_val = 255;
            }
            self.length_counter = actual_load_val;
        }

        let freq_lsb = self.nr33.freq_lo_val() as u16;
        let freq_msb = self.nr34.frequency_msb_val() as u16;
        let period_val = (freq_msb << 8) | freq_lsb;
        // SameBoy: (period_val ^ 0x7FF) + 3. Our timer is *2 due to T-cycles vs M-cycles.
        // ( (period_val ^ 0x7FF) + C ) * 2. Let C=2.
        // period_val ^ 0x7FF is (2047 - period_val) because period_val <= 2047.
        // So, (2047 - period_val + 2) * 2 = (2049 - period_val) * 2.
        // Ensure period_val doesn't exceed 2049 for positive result, though max is 2047.
        if period_val <= 2049 { // Max period_val is 2047, so this is always true.
             self.frequency_timer = (2049u16.saturating_sub(period_val)) * 2;
        } else { // Should not happen with valid period_val
             self.frequency_timer = 0;
        }
        self.sample_index = 0; // Reset sample index

        // If frequency timer is immediately 0 (e.g. period_val is 2049 or more, or 2047 with C=2),
        // pre-load the first nibble from wave_ram[0].
        // This aligns with SameBoy loading sample_byte if sample_countdown is 0 on trigger.
        if self.frequency_timer == 0 && self.enabled {
            let sample_byte = wave_ram[0];
            self.current_sample_buffer = (sample_byte >> 4) & 0x0F; // First nibble
            self.wave_form_just_read = true; // Considered an immediate read
            // Note: tick() would advance sample_index to 1 then load if this wasn't here.
            // This pre-load means the first played sample is indeed from index 0's first nibble.
            // Then tick() will advance to index 1.
        }

        if !self.nr30.dac_on() { self.enabled = false; } // Ensure channel disabled if DAC is off
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
        if !self.enabled {
            self.wave_form_just_read = false; // Clear if channel is not enabled
            return;
        }

        self.frequency_timer = self.frequency_timer.saturating_sub(1);

        let mut did_read_sample_this_tick = false;

        if self.frequency_timer == 0 {
            let freq_lsb = self.nr33.freq_lo_val() as u16;
            let freq_msb = self.nr34.frequency_msb_val() as u16;
            let period_val = (freq_msb << 8) | freq_lsb;
            self.frequency_timer = (2048 - period_val) * 2;
            // self.enabled implies self.nr30.dac_on() due to trigger logic, but double check for safety
            if self.enabled && self.nr30.dac_on() {
                self.sample_index = (self.sample_index + 1) % 32;
                let byte_index = (self.sample_index / 2) as usize;
                let sample_byte = wave_ram[byte_index];
                self.current_sample_buffer = if self.sample_index % 2 == 0 {
                    (sample_byte >> 4) & 0x0F
                } else {
                    sample_byte & 0x0F
                };
                did_read_sample_this_tick = true;
            }
        }
        self.wave_form_just_read = did_read_sample_this_tick;
    }

    pub fn get_output_sample(&self) -> u8 {
        if !self.enabled || !self.nr30.dac_on() { return 0; }
        let output_nibble = self.current_sample_buffer;
        let shifted_nibble = output_nibble >> self.nr32.get_volume_shift();
        shifted_nibble
    }

    /// Returns the wave RAM byte index that the channel is currently reading
    /// when active. This emulates CGB behavior where CPU accesses to wave RAM
    /// while CH3 is active are redirected to the byte currently being read.
    pub fn current_wave_ram_byte_index(&self) -> usize {
        (self.sample_index / 2) as usize
    }

    #[allow(dead_code)] // Will be used once model checks are in place
    pub(super) fn get_wave_form_just_read(&self) -> bool {
        self.wave_form_just_read
    }

    pub(super) fn set_wave_form_just_read(&mut self, val: bool) {
        self.wave_form_just_read = val;
    }

    pub(super) fn get_frequency_timer(&self) -> u16 {
        self.frequency_timer
    }

    #[allow(dead_code)] // Will be used once model checks are in place for pulsed logic
    pub(super) fn set_pulsed(&mut self, val: bool) {
        self.pulsed = val;
    }

    pub(super) fn is_active(&self) -> bool {
        self.enabled && self.nr30.dac_on()
    }

    /// Reloads the current_sample_buffer based on the current sample_index and wave_ram.
    /// Used when DAC is disabled at a specific timing per SameBoy.
    pub(super) fn reload_current_sample_buffer(&mut self, wave_ram: &[u8;16]) {
        // This should only be called if channel was enabled and DAC is being turned off.
        // The sample_index should be valid.
        let byte_index = (self.sample_index / 2) as usize;
        if byte_index < wave_ram.len() { // Ensure index is within bounds
            let sample_byte = wave_ram[byte_index];
            self.current_sample_buffer = if self.sample_index % 2 == 0 {
                (sample_byte >> 4) & 0x0F
            } else {
                sample_byte & 0x0F
            };
        }
        // wave_form_just_read is not explicitly set here as this is a corrective action,
        // not a standard tick-based sample read.
    }

    // In src/apu/channel3.rs, within impl Channel3
    // pub fn reload_length_on_enable(&mut self, current_frame_sequencer_step: u8) { // Now unused
    //     let length_data = self.nr31.sound_length_val(); // 0-255
    //     let is_max_length_condition_len = length_data == 0;
    //     // Max length for channel 3 is 256
    //     let mut actual_load_val_len = if is_max_length_condition_len { 256 } else { 256 - length_data as u16 };

    //     let fs_condition_met = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
    //     // self.nr34.is_length_enabled() should be true
    //     if fs_condition_met && self.nr34.is_length_enabled() && is_max_length_condition_len {
    //         actual_load_val_len = 255;
    //     }
    //     self.length_counter = actual_load_val_len;
    // }
}
