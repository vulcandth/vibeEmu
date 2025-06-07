// src/apu/tests.rs
#[cfg(test)]
mod tests {
    use crate::apu::{
        Apu, NR10_ADDR, NR11_ADDR, NR12_ADDR, NR13_ADDR, NR14_ADDR, NR21_ADDR, NR22_ADDR, NR23_ADDR,
        NR24_ADDR, NR30_ADDR, NR31_ADDR, NR32_ADDR, NR33_ADDR, NR34_ADDR, NR41_ADDR, NR42_ADDR,
        NR43_ADDR, NR44_ADDR, NR50_ADDR, NR51_ADDR, NR52_ADDR,
        CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK,
    };

    fn create_apu_and_power_on() -> Apu {
        let mut apu = Apu::new(44100); // Use a fixed sample rate for tests
        // Power on APU
        apu.write_byte(NR52_ADDR, 0x80);
        // Initialize NR50 and NR51 to enable outputs for testing
        apu.write_byte(NR50_ADDR, 0x77); // Max volume for SO1/SO2
        apu.write_byte(NR51_ADDR, 0xFF); // All channels to SO1/SO2
        apu
    }

    #[test]
    fn test_ch1_trigger_and_status_check() {
        let mut apu = create_apu_and_power_on();
        // Minimal setup for CH1 to make sound and be enabled
        apu.write_byte(NR11_ADDR, 0b10000001); // Duty 50%, Length 1
        apu.write_byte(NR12_ADDR, 0xF0);       // Initial vol 15, No envelope
        // No sweep NR10 default is fine
        apu.write_byte(NR13_ADDR, 0x00);       // Freq low
        apu.write_byte(NR14_ADDR, 0b10000000); // Trigger, No length disable, Freq high

        // Tick for a while to let channel status update
        // One frame sequencer cycle should be enough for status update
        for _ in 0..(CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK * 8) {
            apu.tick(1);
        }

        let nr52_val = apu.read_byte(NR52_ADDR);
        assert_ne!(nr52_val & 0x01, 0x00, "CH1 status bit should be set in NR52");

        // Check if it produces some output (very basic check)
        let (so1, so2) = apu.get_mixed_audio_samples();
        assert!(so1 != 0.0 || so2 != 0.0, "CH1 should produce non-zero output");
    }

    #[test]
    fn test_ch2_trigger_and_status_check() {
        let mut apu = create_apu_and_power_on();
        apu.write_byte(NR21_ADDR, 0b10000001); // Duty 50%, Length 1
        apu.write_byte(NR22_ADDR, 0xF0);       // Initial vol 15, No envelope
        apu.write_byte(NR23_ADDR, 0x00);
        apu.write_byte(NR24_ADDR, 0b10000000); // Trigger

        for _ in 0..(CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK * 8) {
            apu.tick(1);
        }

        let nr52_val = apu.read_byte(NR52_ADDR);
        assert_ne!(nr52_val & 0x02, 0x00, "CH2 status bit should be set in NR52");
        let (so1, so2) = apu.get_mixed_audio_samples();
        assert!(so1 != 0.0 || so2 != 0.0, "CH2 should produce non-zero output");
    }

    #[test]
    fn test_ch3_trigger_and_status_check() {
        let mut apu = create_apu_and_power_on();
        // Wave RAM is pre-filled by Apu::new()
        apu.write_byte(NR30_ADDR, 0x80);       // DAC On
        apu.write_byte(NR31_ADDR, 0x01);       // Length 1
        apu.write_byte(NR32_ADDR, 0b00100000); // Volume 100% (code 01)
        apu.write_byte(NR33_ADDR, 0x00);
        apu.write_byte(NR34_ADDR, 0b10000000); // Trigger

        for _ in 0..(CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK * 8) {
            apu.tick(1);
        }

        let nr52_val = apu.read_byte(NR52_ADDR);
        assert_ne!(nr52_val & 0x04, 0x00, "CH3 status bit should be set in NR52");
        let (so1, so2) = apu.get_mixed_audio_samples();
        assert!(so1 != 0.0 || so2 != 0.0, "CH3 should produce non-zero output with pre-filled wave RAM");
    }

    #[test]
    fn test_ch4_trigger_and_status_check() {
        let mut apu = create_apu_and_power_on();
        apu.write_byte(NR41_ADDR, 0x01);       // Length 1
        apu.write_byte(NR42_ADDR, 0xF0);       // Initial vol 15, No envelope
        apu.write_byte(NR43_ADDR, 0x20);       // LFSR params
        apu.write_byte(NR44_ADDR, 0b10000000); // Trigger

        for _ in 0..(CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK * 8) {
            apu.tick(1);
        }

        let nr52_val = apu.read_byte(NR52_ADDR);
        assert_ne!(nr52_val & 0x08, 0x00, "CH4 status bit should be set in NR52");
        let (so1, so2) = apu.get_mixed_audio_samples();
        assert!(so1 != 0.0 || so2 != 0.0, "CH4 should produce non-zero output");
    }

    // --- Tests for power_on_reset behavior ---
    #[test]
    fn test_ch3_initial_state_after_apu_power_on() {
        let mut apu = Apu::new(44100); // Use a fixed sample rate for tests
        // CH3 is initially disabled, length counter 0 etc. by Apu::new() due to reset_power_on_channel_flags
        // Trigger CH3 to give it some non-default state IF Apu::new() didn't already reset it via power_on_reset
        apu.write_byte(NR52_ADDR, 0x80); // Power on to allow register writes
        apu.write_byte(NR30_ADDR, 0x80);
        apu.write_byte(NR31_ADDR, 0xFF); // Max length
        apu.write_byte(NR34_ADDR, 0x80); // Trigger

        // Tick for a short duration to ensure it's 'on' if it were to enable
        for _ in 0..CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK {
             apu.tick(1);
        }
        // At this point, if power_on_reset wasn't called initially, CH3 might be on.
        // If power_on_reset WAS called by Apu::new(), these writes might make it on.
        // The crucial part is the state AFTER power cycle.

        // Now, power APU off and on again
        apu.write_byte(NR52_ADDR, 0x00); // Power off (should call full_apu_reset_on_power_off)
        apu.write_byte(NR52_ADDR, 0x80); // Power back on (should call reset_power_on_channel_flags)

        // Tick for a bit to allow reset logic to run during power on sequence
        for _ in 0..10 {
            apu.tick(1);
        }

        assert!(!apu.channel3.enabled, "CH3 should be disabled after APU power cycle and reset");
        assert_eq!(apu.channel3.get_length_counter(), 0, "CH3 length counter should be 0 after APU power cycle and reset");
    }

    #[test]
    fn test_ch4_initial_state_after_apu_power_on() {
        let mut apu = Apu::new(44100); // Use a fixed sample rate for tests
        // Setup CH4 to a non-default state
        apu.write_byte(NR52_ADDR, 0x80);
        apu.write_byte(NR41_ADDR, 0x3F); // Max Length
        apu.write_byte(NR42_ADDR, 0xF0); // Max Volume, no envelope
        apu.write_byte(NR43_ADDR, 0x20);
        apu.write_byte(NR44_ADDR, 0x80); // Trigger

        for _ in 0..CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK {
            apu.tick(1);
        }

        // Power APU off and on
        apu.write_byte(NR52_ADDR, 0x00);
        apu.write_byte(NR52_ADDR, 0x80);

        for _ in 0..10 {
            apu.tick(1);
        }

        assert!(!apu.channel4.enabled, "CH4 should be disabled after APU power cycle and reset");
        assert_eq!(apu.channel4.get_length_counter(), 0, "CH4 length counter should be 0 after APU power cycle and reset");
        assert_eq!(apu.channel4.get_envelope_volume(), 0, "CH4 envelope volume should be 0 after APU power cycle and reset");
    }

    // --- Test for HPF Activity ---
    #[test]
    fn test_hpf_activity() {
        let mut apu = create_apu_and_power_on(); // Uses Apu::new(0)

        apu.channel1.nr11.write(0b10000000); // Duty 50% (pattern 10000111), length 64.
        apu.write_byte(NR12_ADDR, 0xF0);    // Max initial volume (15), envelope direction increase (irrelevant), period 0 (no envelope)
        // Set a very low frequency so duty step doesn't change for a long time
        apu.write_byte(NR13_ADDR, 0xFF);    // Freq low part = 0xFF
        apu.write_byte(NR14_ADDR, 0b10000111); // Trigger, Freq high part = 0x07 (period = 2047, slowest)
                                            // Timer load will be (2048-2047)*4 = 4. This is too fast.
                                            // We need freq_timer to be large.
                                            // Let's set period to 0 (0xFF, 0x07 is max period)
                                            // Trigger will set frequency_timer to (2048-0)*4 = 8192
        apu.channel1.nr13.write(0x00); // freq_lo = 0
        apu.channel1.nr14.write(0x80); // freq_hi = 0, trigger. Period val = 0. freq_timer = (2048-0)*4 = 8192

        apu.write_byte(NR51_ADDR, 0x11);
        apu.write_byte(NR50_ADDR, 0x77);

        let mut samples_so1 = Vec::new();

        // First sample will be 0 due to force_output_zero_for_next_sample from CH1's power_on_reset then first trigger
        let (s1, _) = apu.get_mixed_audio_samples(); samples_so1.push(s1);
        for _ in 0..8192 { apu.tick(1); } // Ensure CH1 duty step advances past forced zero if it was just powered up
                                      // And also ensures that has_been_triggered_since_power_on is true.
                                      // And that force_output_zero_for_next_sample is consumed.

        // Re-trigger to ensure we are at duty_step 0 after all init.
        apu.write_byte(NR14_ADDR, 0x80); // Trigger CH1 again.
        // Now, the next sample from get_output_volume should be based on duty_step 0.
        // For pattern 0b10 (default from nr11.write(0x80)), duty_step 0 is '1'.
        // CH1 output: envelope_volume (15) * wave_output (1) = 15
        // DAC conversion: 1.0 - (15.0 / 7.5) = -1.0
        // SO1 mix: -1.0
        // Master vol: -1.0 * (7+1)/8 = -1.0
        // Final division: -1.0 / 4.0 = -0.25

        let (s1_initial_expected_dc, _) = apu.get_mixed_audio_samples();
        samples_so1.push(s1_initial_expected_dc);
        // Tick APU so freq_timer advances, but not enough to change duty_step
        // (freq_timer is 8192, duty_step changes when it hits 0)
        for _ in 0..10 { apu.tick(1); }


        const NUM_FRAMES: usize = 1000; // Number of audio samples to collect for HPF test
        for i in 0..NUM_FRAMES {
            let (s1, _) = apu.get_mixed_audio_samples();
            samples_so1.push(s1);
            // Tick APU by a number of T-cycles. Each apu.tick(1) is one T-cycle.
            // We need to ensure the HPF capacitor updates, and also that CH1's freq_timer
            // very slowly decrements but ideally doesn't hit zero during these NUM_FRAMES
            // to keep the DC output stable.
            // If we tick by, say, 4 T-cycles each time:
            for _ in 0..4 { apu.tick(1); }
            if i < 5 && samples_so1.last().is_some() { // Print first few samples
                 //println!("Sample {}: {:.4}", samples_so1.len() -1, samples_so1.last().unwrap());
            }
        }

        // samples_so1[0] is from before CH1 was properly triggered after power on state.
        // samples_so1[1] is the first sample after proper trigger. This should be our DC value.
        assert!(samples_so1[1].abs() > 0.15 && samples_so1[1].abs() < 0.30,
                "Initial DC output after trigger should be around -0.25 (or 0.25). Got {}", samples_so1[1]);

        let initial_avg: f32 = samples_so1[1..11].iter().sum::<f32>() / 10.0;
        let final_idx_start = samples_so1.len().saturating_sub(10);
        let final_avg: f32 = samples_so1[final_idx_start..].iter().sum::<f32>() / (samples_so1.len() - final_idx_start) as f32;

        println!("HPF Test: Initial sample (idx 1): {:.4}, Initial avg (1-10): {:.4}, Final avg (last 10): {:.4}", samples_so1[1], initial_avg, final_avg);
        assert!(final_avg.abs() < initial_avg.abs() / 2.0 || final_avg.abs() < 0.01,
                "HPF should reduce DC offset over time. Initial_avg: {:.4}, Final_avg: {:.4}", initial_avg, final_avg);
    }
}
