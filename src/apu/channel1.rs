// src/apu/channel1.rs
// Content will be added in subsequent steps.
use super::{Nr10, Nr11, Nr12, Nr13, Nr14}; // Use super to access items from src/apu.rs

// Define Channel1 struct and its methods here
pub struct Channel1 {
    // Register fields
    pub nr10: Nr10,
    pub nr11: Nr11,
    pub nr12: Nr12,
    pub nr13: Nr13,
    pub nr14: Nr14,

    // Internal state
    enabled: bool,
    length_counter: u16,
    frequency_timer: u16,
    duty_step: u8,

    // Volume Envelope State
    envelope_volume: u8,
    envelope_period_timer: u8,
    envelope_running: bool,

    // Frequency Sweep State
    sweep_period_timer: u8,
    sweep_shadow_frequency: u16,
    sweep_enabled: bool,
    sweep_calculated_overflow_this_step: bool, // True if sweep calculation resulted in overflow
}

// Implement Channel1 methods here
impl Channel1 {
    pub fn new() -> Self {
        Self {
            nr10: Nr10::new(), // pub(super) in apu.rs
            nr11: Nr11::new(),
            nr12: Nr12::new(),
            nr13: Nr13::new(),
            nr14: Nr14::new(),
            enabled: false,
            length_counter: 0,
            frequency_timer: 0, // Will be loaded on trigger
            duty_step: 0,
            envelope_volume: 0, // Will be loaded from NR12 on trigger
            envelope_period_timer: 0, // Will be loaded from NR12 on trigger
            envelope_running: false,
            sweep_period_timer: 0, // Will be loaded from NR10 on trigger
            sweep_shadow_frequency: 0, // Will be loaded from NR13/NR14 on trigger
            sweep_enabled: false,
            sweep_calculated_overflow_this_step: false,
        }
    }

    pub fn trigger(&mut self) {
        if self.nr12.dac_power() {
            self.enabled = true;
        }

        // Reload length counter (t1 = NR11 bits 5-0)
        // Length is 64 - t1. If t1 is 0, length is 64.
        let length_data = self.nr11.initial_length_timer_val();
        self.length_counter = if length_data == 0 { 64 } else { 64 - length_data as u16 };

        // Reload frequency timer
        let freq_lsb = self.nr13.freq_lo_val() as u16;
        let freq_msb = self.nr14.frequency_msb_val() as u16;
        let period_val = (freq_msb << 8) | freq_lsb;
        // Formula: Timer Period = (2048 - CPU_Freq_Register_Value) * 4 APU clock cycles
        // CPU_Freq_Register_Value is what's in NR13/NR14.
        // Game Boy CPU clock is 4194304 Hz. APU clock is half of that, 2097152 Hz.
        // Or, simpler, the sequencer generates clocks at 512Hz for frequency.
        // The value in NR13/14 (call it X) sets a period. The sound frequency is 131072 / (2048 - X) Hz.
        // The timer should count (2048 - X) some scaled value.
        // From Pan Docs: "The timer is clocked by a 512 Hz source (derived from APU Timer)."
        // "Timer period = (2048 - FreqReg) * 4 APU clocks".
        // The actual timer counts down; when it hits zero, it reloads and the duty step advances.
        self.frequency_timer = (2048 - period_val) * 4;


        // Reset envelope
        self.envelope_volume = self.nr12.initial_volume_val();
        let env_period = self.nr12.envelope_period_val();
        self.envelope_period_timer = if env_period == 0 { 8 } else { env_period };
        self.envelope_running = self.nr12.dac_power() && env_period != 0; // Envelope only runs if DAC is on and period > 0

        // Sweep Trigger Logic
        self.sweep_shadow_frequency = period_val;
        let sweep_period = self.nr10.sweep_period();
        self.sweep_period_timer = if sweep_period == 0 { 8 } else { sweep_period };
        // Sweep is enabled if period or shift is non-zero.
        self.sweep_enabled = sweep_period != 0 || self.nr10.sweep_shift_val() != 0;
        self.sweep_calculated_overflow_this_step = false;

        if self.sweep_enabled && self.nr10.sweep_shift_val() != 0 {
            // Perform one frequency calculation and overflow check immediately.
            let new_freq = self.calculate_sweep_frequency();
            if new_freq > 2047 {
                self.enabled = false;
                self.sweep_calculated_overflow_this_step = true;
            }
            // Note: Pan Docs says "If the new frequency is 2048 or greater, channel 1 is disabled."
            // "If the sweep shift is zero, the channel's frequency is not changed but the overflow check is still performed."
            // The new frequency is NOT written back to NR13/NR14 at trigger time, only during sweep clocking.
        }

        // If DAC is off, channel should be immediately disabled
        if !self.nr12.dac_power() {
            self.enabled = false;
        }
    }

    fn calculate_sweep_frequency(&self) -> u16 {
        let delta = self.sweep_shadow_frequency >> self.nr10.sweep_shift_val();
        if self.nr10.sweep_direction_is_increase() {
            self.sweep_shadow_frequency.saturating_add(delta)
        } else {
            self.sweep_shadow_frequency.saturating_sub(delta)
        }
    }

    pub fn clock_length(&mut self) { // Called at 256Hz
        if self.nr14.is_length_enabled() && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

    pub fn clock_envelope(&mut self) { // Called at 64Hz
        if !self.envelope_running || !self.nr12.dac_power() { // Envelope stops if DAC becomes off
            return;
        }

        let env_period = self.nr12.envelope_period_val();
        if env_period == 0 { // Envelope is stopped if period is 0
            self.envelope_running = false; // ensure it's marked as not running
            return;
        }

        self.envelope_period_timer -= 1;
        if self.envelope_period_timer == 0 {
            self.envelope_period_timer = if env_period == 0 { 8 } else { env_period }; // Reload timer

            let current_volume = self.envelope_volume;
            if self.nr12.envelope_direction_is_increase() {
                if current_volume < 15 {
                    self.envelope_volume += 1;
                }
            } else { // Decrease
                if current_volume > 0 {
                    self.envelope_volume -= 1;
                }
            }

            // If volume reaches 0 or 15, the envelope stops.
            if self.envelope_volume == 0 || self.envelope_volume == 15 {
                self.envelope_running = false;
            }
        }
    }

    pub fn clock_sweep(&mut self) { // Called at 128Hz
        if !self.sweep_enabled || !self.nr12.dac_power() { // Sweep stops if DAC becomes off
            return;
        }

        let sweep_period = self.nr10.sweep_period();
        if sweep_period == 0 { // if sweep period is 0, sweep is effectively off for clocking purposes
             return;
        }

        self.sweep_period_timer -= 1;
        if self.sweep_period_timer == 0 {
            self.sweep_period_timer = if sweep_period == 0 { 8 } else { sweep_period }; // Reload timer

            // Only calculate new frequency if period was non-zero (which we checked)
            let new_freq = self.calculate_sweep_frequency();

            if new_freq > 2047 {
                self.enabled = false;
                self.sweep_calculated_overflow_this_step = true;
                return;
            }

            self.sweep_calculated_overflow_this_step = false; // Reset for next step

            // Only update registers if sweep shift is non-zero
            if self.nr10.sweep_shift_val() != 0 {
                self.sweep_shadow_frequency = new_freq;
                // Update NR13 and NR14's frequency bits
                self.nr13.write((new_freq & 0xFF) as u8);
                self.nr14.write_frequency_msb((new_freq >> 8) as u8);

                // Perform the overflow check again with the new frequency (but don't write back again)
                // This second check is mentioned in some docs.
                let final_check_freq = self.calculate_sweep_frequency(); // Calculate based on the *new* shadow_freq
                if final_check_freq > 2047 {
                    self.enabled = false;
                    self.sweep_calculated_overflow_this_step = true;
                }
            }
        }
    }

    pub fn tick(&mut self) { // Called by APU at its own step rate (e.g. every T-cycle of CPU / APU clock divider)
        if !self.enabled { // Should also check overall APU power if this tick is global
            return;
        }

        self.frequency_timer -= 1; // Assuming this ticks at CPU speed / 4 or similar high rate.
                                   // The actual rate depends on how APU::step calls this.
                                   // For now, assume it's called at a rate that makes sense for the timer values.
                                   // A common model is that the frequency_timer counts down (2048-X) * 4 * (CPU_Clocks_Per_APU_Sample_Step)
                                   // Or simply, it's decremented each time the APU decides to clock the channel's frequency generation part.

        if self.frequency_timer == 0 {
            let freq_lsb = self.nr13.freq_lo_val() as u16;
            let freq_msb = self.nr14.frequency_msb_val() as u16;
            let period_val = (freq_msb << 8) | freq_lsb;
            self.frequency_timer = (2048 - period_val) * 4; // Reload

            self.duty_step = (self.duty_step + 1) % 8;
        }
    }

    pub fn get_output_volume(&self) -> u8 {
        if !self.enabled || !self.nr12.dac_power() || self.sweep_calculated_overflow_this_step {
            // If sweep calculation caused overflow in the *current* step, output is muted for this sample.
            // This flag should be ideally reset before the next full sweep calculation.
            // Or, more simply, if enabled is false due to sweep overflow, this check handles it.
            return 0;
        }

        let wave_duty = self.nr11.wave_pattern_duty_val();
        let wave_output = match wave_duty {
            0b00 => [0,0,0,0,0,0,0,1][self.duty_step as usize], // 12.5% duty (_______-)
            0b01 => [1,0,0,0,0,0,0,1][self.duty_step as usize], // 25%   duty (-______-)
            0b10 => [1,0,0,0,0,1,1,1][self.duty_step as usize], // 50%   duty (-____---)
            0b11 => [0,1,1,1,1,1,1,0][self.duty_step as usize], // 75%   duty (_------_) (inverted 25%)
            _ => 0, // Should not happen
        };

        if wave_output == 1 {
            self.envelope_volume
        } else {
            0
        }
    }
}
