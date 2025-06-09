// src/apu/channel3.rs
use super::{Nr30, Nr31, Nr32, Nr33, Nr34};
use crate::bus::SystemMode; // Added import

pub struct Channel3 {
    // NR34 related fields based on Sameboy model
    pub(super) length_enabled_internal: bool, // Made pub(super)
    pub(super) pulsed_on_trigger: bool, // True if channel was (re)triggered // Made pub(super)
    pub(super) wave_form_just_read: bool, // For DMG Wave RAM read/write glitch // Made pub(super)

    // Existing fields
    pub nr30: Nr30,
    pub nr31: Nr31,
    pub nr32: Nr32,
    pub nr33: Nr33,
    pub nr34: Nr34,
    pub enabled: bool,
    length_counter: u16,
    pub(super) sample_countdown: u16, // Renamed from frequency_timer, made pub(super)
    pub(super) current_sample_index: u8, // Renamed from sample_index, made pub(super)
    current_sample_buffer: u8,
}

impl Channel3 {
    pub fn new() -> Self {
        Self {
            length_enabled_internal: false,
            pulsed_on_trigger: false,
            wave_form_just_read: false,

            nr30: Nr30::new(), nr31: Nr31::new(), nr32: Nr32::new(), nr33: Nr33::new(), nr34: Nr34::new(),
            enabled: false, length_counter: 0, sample_countdown: 0, current_sample_index: 0, current_sample_buffer: 0,
        }
    }

    #[allow(unused_variables)]
    pub fn trigger(&mut self, wave_ram_for_corruption: &[u8;16], current_frame_sequencer_step: u8, system_mode: SystemMode, lf_div_raw: u8) {
        self.pulsed_on_trigger = true;

        if self.nr30.dac_on() { self.enabled = true; } else { self.enabled = false; return; }

        if self.length_counter == 0 {
            self.length_counter = 256;
            self.length_enabled_internal = false;
        }

        self.current_sample_index = 0;

        let freq_lsb = self.nr33.freq_lo_val() as u16;
        let freq_msb = self.nr34.frequency_msb_val() as u16;
        let period_val = (freq_msb << 8) | freq_lsb;
        self.sample_countdown = ((2048 - period_val) * 2).saturating_add(6);

        if self.enabled {
            let byte_index = (self.current_sample_index / 2) as usize;
            if byte_index < wave_ram_for_corruption.len() {
                 let sample_byte = wave_ram_for_corruption[byte_index];
                 self.current_sample_buffer = if self.current_sample_index % 2 == 0 {
                    (sample_byte >> 4) & 0x0F
                } else {
                    sample_byte & 0x0F
                };
            }
            self.wave_form_just_read = true;
        }
    }

    pub fn get_length_counter(&self) -> u16 { self.length_counter }
    pub(super) fn sample_countdown(&self) -> u16 { self.sample_countdown }
    pub(super) fn current_sample_index(&self) -> u8 { self.current_sample_index }
    pub(super) fn wave_form_just_read_get(&self) -> bool { self.wave_form_just_read } // Added getter

    pub fn extra_length_clock(&mut self, trigger_is_set_in_nrx4: bool) {
        if self.length_enabled_internal && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 && !trigger_is_set_in_nrx4 && !self.pulsed_on_trigger {
                self.enabled = false;
            }
        }
    }

    pub fn clock_length(&mut self) {
        if self.length_enabled_internal && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 && !self.pulsed_on_trigger {
                self.enabled = false;
            }
        }
    }

    pub fn tick(&mut self, wave_ram: &[u8; 16]) {
        self.wave_form_just_read = false;
        if !self.enabled { return; }

        self.sample_countdown = self.sample_countdown.saturating_sub(1);
        if self.sample_countdown == 0 {
            let freq_lsb = self.nr33.freq_lo_val() as u16;
            let freq_msb = self.nr34.frequency_msb_val() as u16;
            let period_val = (freq_msb << 8) | freq_lsb;
            self.sample_countdown = (2048 - period_val) * 2;

            if self.nr30.dac_on() && self.enabled {
                self.current_sample_index = (self.current_sample_index + 1) & 0x1F;
                let byte_index = (self.current_sample_index / 2) as usize;
                if byte_index < wave_ram.len() {
                    let sample_byte = wave_ram[byte_index];
                    self.current_sample_buffer = if self.current_sample_index % 2 == 0 {
                        (sample_byte >> 4) & 0x0F
                    } else {
                        sample_byte & 0x0F
                    };
                    self.wave_form_just_read = true;
                } else {
                    self.current_sample_buffer = 0;
                }
            }
             self.pulsed_on_trigger = false;
        }
    }

    pub fn get_output_sample(&self) -> u8 {
        if !self.enabled || !self.nr30.dac_on() { return 0; }
        let output_nibble = self.current_sample_buffer;
        let shifted_nibble = output_nibble >> self.nr32.get_volume_shift();
        shifted_nibble
    }

    pub fn current_wave_ram_byte_index(&self) -> usize {
        (self.current_sample_index / 2) as usize // Changed self.sample_index
    }

    pub fn reload_length_on_enable(&mut self, current_frame_sequencer_step: u8) {
        let length_data = self.nr31.sound_length_val();
        let is_max_length_condition_len = length_data == 0;
        let mut actual_load_val_len = if is_max_length_condition_len { 256 } else { 256 - length_data as u16 };

        let fs_condition_met = matches!(current_frame_sequencer_step, 0 | 2 | 4 | 6);
        if fs_condition_met && self.nr34.is_length_enabled() && is_max_length_condition_len {
            actual_load_val_len = 255;
        }
        self.length_counter = actual_load_val_len;
    }
}
