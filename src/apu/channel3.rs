// src/apu/channel3.rs
use super::{Nr30, Nr31, Nr32, Nr33, Nr34};

pub struct Channel3 {
    // Registers
    pub nr30: Nr30,
    pub nr31: Nr31,
    pub nr32: Nr32,
    pub nr33: Nr33,
    pub nr34: Nr34,

    // Internal state
    enabled: bool,
    length_counter: u16,
    frequency_timer: u16,
    sample_index: u8, // 0-31, current nibble index into Wave RAM
    current_sample_buffer: u8, // stores the current nibble being played
    // wave_ram_read_allowed_this_cycle: bool, // For more precise timing, can ignore for now
}

impl Channel3 {
    pub fn new() -> Self {
        Self {
            nr30: Nr30::new(),
            nr31: Nr31::new(),
            nr32: Nr32::new(),
            nr33: Nr33::new(),
            nr34: Nr34::new(),
            enabled: false,
            length_counter: 0,
            frequency_timer: 0,
            sample_index: 0,
            current_sample_buffer: 0,
            // wave_ram_read_allowed_this_cycle: true,
        }
    }

    pub fn is_active_for_wave_ram_access(&self) -> bool {
        // This is a simplified check. True HW behavior is more complex
        // when CPU and APU access Wave RAM concurrently while channel is running.
        self.enabled && self.nr30.dac_on()
    }

    // trigger, clock_length, tick, get_output_sample methods will be added here
    pub fn trigger(&mut self) {
        // Per Pan Docs, CH3 is enabled only if DAC is on (NR30 bit 7)
        self.enabled = self.nr30.dac_on();

        // Reload length counter (t1 = NR31 value)
        // Actual length is 256 - t1. If t1 is 0 (max length in register), counter gets 256.
        let length_data = self.nr31.sound_length_val();
        self.length_counter = 256 - length_data as u16; // if length_data is 0, this becomes 256.

        // Reload frequency timer
        // Period P = (2048 - FreqRegVal)
        // Timer reload value = P * 2 (Wave channel's timer is clocked differently)
        let freq_lsb = self.nr33.freq_lo_val() as u16;
        let freq_msb = self.nr34.frequency_msb_val() as u16;
        let period_val = (freq_msb << 8) | freq_lsb;
        self.frequency_timer = (2048 - period_val) * 2;

        // Reset sample index to the beginning of the wave table.
        self.sample_index = 0;

        // Pan Docs: "Triggering the wave channel does not immediately start playing wave RAM;
        // instead, the last sample ever read (which is reset to 0 when the APU is off)
        // is output until the channel next reads a sample."
        // The current_sample_buffer is NOT reset here, it holds the last value or 0 if APU was off.
        // The first actual sample read by tick() will then populate it.

        // If DAC is off, ensure channel is disabled regardless of trigger
        if !self.nr30.dac_on() {
            self.enabled = false;
        }
    }

    pub fn clock_length(&mut self) { // Called at 256Hz
        if self.nr34.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

    // Note: wave_ram is passed from the main Apu struct where it's stored.
    pub fn tick(&mut self, wave_ram: &[u8; 16]) {
        if !self.enabled { // Overall APU power should gate this at a higher level too
            return;
        }

        self.frequency_timer -= 1;

        if self.frequency_timer == 0 {
            let freq_lsb = self.nr33.freq_lo_val() as u16;
            let freq_msb = self.nr34.frequency_msb_val() as u16;
            let period_val = (freq_msb << 8) | freq_lsb;
            self.frequency_timer = (2048 - period_val) * 2; // Reload

            // Advance sample index and fetch next nibble if channel is truly active
            if self.nr30.dac_on() && self.enabled { // Check dac_on again, as it can be turned off while enabled=true
                self.sample_index = (self.sample_index + 1) % 32;
                let byte_index = (self.sample_index / 2) as usize;

                let sample_byte = wave_ram[byte_index];

                self.current_sample_buffer = if self.sample_index % 2 == 0 {
                    (sample_byte >> 4) & 0x0F // High nibble first
                } else {
                    sample_byte & 0x0F          // Low nibble second
                };
            } else {
                // If DAC turned off while channel was enabled, behavior might be to play last sample or silence.
                // For now, if dac is off, no new samples are fetched. get_output_sample will return 0.
            }
        }
    }

    pub fn get_output_sample(&self) -> u8 {
        if !self.enabled || !self.nr30.dac_on() {
            return 0;
        }

        let output_nibble = self.current_sample_buffer;

        match self.nr32.output_level_val() {
            0b00 => 0,                               // Mute
            0b01 => output_nibble,                   // 100%
            0b10 => output_nibble >> 1,              // 50%
            0b11 => output_nibble >> 2,              // 25%
            _ => 0, // Should not happen
        }
    }
}
