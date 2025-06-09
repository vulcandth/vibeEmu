// src/apu/tests.rs
#[cfg(test)]
mod tests {
    use crate::apu::{Apu, NR11_ADDR, NR12_ADDR, NR14_ADDR, NR52_ADDR};
    use crate::apu::{NR30_ADDR, NR31_ADDR, NR32_ADDR, NR33_ADDR, NR34_ADDR};

    const WAVE_START: u16 = 0xFF30;

    fn create_apu_and_power_on() -> Apu {
        let mut apu = Apu::new();
        apu.write_byte(NR52_ADDR, 0x80);
        apu
    }

    #[test]
    fn test_ch1_trigger_and_initial_status() {
        let mut apu = create_apu_and_power_on();
        apu.write_byte(NR11_ADDR, 0b10000001);
        apu.write_byte(NR12_ADDR, 0xF3);
        apu.write_byte(NR14_ADDR, 0b11000000);
        for _ in 0..(8192 * 2) { apu.tick(1); }
        let nr52_val_after_trigger = apu.read_byte(NR52_ADDR);
        assert_ne!(nr52_val_after_trigger & 0x01, 0x00, "CH1 should be on after trigger and some ticks");
    }

    // Placeholder for more tests
    #[test]
    fn basic_channel2_trigger() {
        let mut apu = create_apu_and_power_on();
        apu.write_byte(0xFF16, 0b10000001);
        apu.write_byte(0xFF17, 0xF3);
        apu.write_byte(0xFF19, 0b11000000);
        for _ in 0..(8192 * 2) { apu.tick(1); }
        let nr52_val = apu.read_byte(NR52_ADDR);
        assert_ne!(nr52_val & 0x02, 0x00, "CH2 should be on");
    }

    #[test]
    fn ch3_wave_ram_active_access_redirects() {
        let mut apu = create_apu_and_power_on();

        // Preload wave RAM with known pattern
        for i in 0..16 {
            apu.write_byte(WAVE_START + i, i as u8);
        }

        // Setup channel 3 with high frequency for quick sample increments
        apu.write_byte(NR30_ADDR, 0x80); // DAC on
        apu.write_byte(NR31_ADDR, 0x00);
        apu.write_byte(NR32_ADDR, 0x20);
        apu.write_byte(NR33_ADDR, 0xFF);
        apu.write_byte(NR34_ADDR, 0x87); // trigger, freq hi = 7

        // CH3 active, sample_index = 0 -> CPU access should map to byte 0
        apu.write_byte(WAVE_START + 1, 0xAA);
        assert_eq!(apu.read_byte(WAVE_START + 0), 0xAA);

        // Advance two samples so current byte index becomes 1
        for _ in 0..4 { apu.tick(1); }

        apu.write_byte(WAVE_START + 2, 0xBB);
        assert_eq!(apu.read_byte(WAVE_START + 0x01), 0xBB);
    }

}

#[cfg(test)]
mod wave_ram_access_tests {
    use crate::apu::{Apu, NR30_ADDR, NR34_ADDR, NR52_ADDR, WAVE_PATTERN_RAM_START_ADDR};
    use crate::bus::SystemMode;

    fn setup_apu_with_mode(system_mode: SystemMode) -> Apu {
        let mut apu = Apu::new(system_mode); // Assuming Apu::new takes SystemMode
        apu.write_byte(NR52_ADDR, 0x80, false); // Power on APU (is_double_speed false for simplicity)
        // Pre-fill wave RAM with a known pattern (0x00, 0x11, 0x22, ..., 0xFF)
        for i in 0..16 {
            apu.wave_ram[i] = (i as u8) * 0x11;
        }
        apu
    }

    #[test]
    fn test_wave_ram_disabled_ch3() {
        let mut apu = setup_apu_with_mode(SystemMode::DMG);
        apu.write_byte(NR30_ADDR, 0x00, false); // Ensure CH3 DAC is off (channel disabled)

        // Write directly
        apu.write_byte(WAVE_PATTERN_RAM_START_ADDR + 5, 0xAB, false);
        assert_eq!(apu.wave_ram[5], 0xAB, "Direct write to wave RAM when CH3 disabled failed");

        // Read directly
        assert_eq!(apu.read_byte(WAVE_PATTERN_RAM_START_ADDR + 5), 0xAB, "Direct read from wave RAM when CH3 disabled failed");
        assert_eq!(apu.read_byte(WAVE_PATTERN_RAM_START_ADDR + 0), 0x00, "Direct read [0] failed");
        assert_eq!(apu.read_byte(WAVE_PATTERN_RAM_START_ADDR + 1), 0x11, "Direct read [1] failed");
    }

    #[test]
    fn test_wave_ram_dmg_enabled_ch3() {
        let mut apu = setup_apu_with_mode(SystemMode::DMG);
        // Enable CH3: DAC on, trigger
        apu.write_byte(NR30_ADDR, 0x80, false); // DAC on
        // Trigger with some frequency that doesn't make it advance too fast or too slow
        apu.write_byte(NR33_ADDR, 0x00, false);
        apu.write_byte(NR34_ADDR, 0b10000000, false); // Trigger, No length

        // Initial state after trigger: sample_index = 0, wave_form_just_read = true (due to trigger loading first sample)
        assert!(apu.channel3.wave_form_just_read_get(), "wave_form_just_read should be true after trigger");
        assert_eq!(apu.channel3.current_wave_ram_byte_index(), 0, "current_wave_ram_byte_index should be 0 after trigger");

        // Read while wave_form_just_read = true
        assert_eq!(apu.read_byte(WAVE_PATTERN_RAM_START_ADDR + 5), apu.wave_ram[0], "DMG Read (just_read=true) should redirect to current_sample_index/2");

        // Write while wave_form_just_read = true
        apu.write_byte(WAVE_PATTERN_RAM_START_ADDR + 5, 0xCC, false);
        assert_eq!(apu.wave_ram[0], 0xCC, "DMG Write (just_read=true) should redirect");
        assert_ne!(apu.wave_ram[5], 0xCC, "DMG Write (just_read=true) should not write to address directly");
        apu.wave_ram[0] = 0x00; // Reset for next part

        // Simulate APU ticks to make wave_form_just_read false
        // Channel3 tick should reset wave_form_just_read. One APU tick is enough if it leads to CH3 tick.
        // The sample_countdown is likely high after trigger. Let's force it.
        apu.channel3.sample_countdown = 1; // Force next tick to advance sample
        apu.tick(1 * 4); // One M-cycle of T-cycles
        assert!(!apu.channel3.wave_form_just_read_get(), "wave_form_just_read should be false after a tick that doesn't reload sample");

        // Read while wave_form_just_read = false
        assert_eq!(apu.read_byte(WAVE_PATTERN_RAM_START_ADDR + 5), 0xFF, "DMG Read (just_read=false) should return 0xFF");

        // Write while wave_form_just_read = false (should be blocked)
        let original_val_at_idx = apu.channel3.current_wave_ram_byte_index();
        let original_wave_val = apu.wave_ram[original_val_at_idx];
        apu.write_byte(WAVE_PATTERN_RAM_START_ADDR + 7, 0xDD, false); // Attempt to write to a different address
        assert_eq!(apu.wave_ram[original_val_at_idx], original_wave_val, "DMG Write (just_read=false) should be blocked for current index");
        assert_ne!(apu.wave_ram[7], 0xDD, "DMG Write (just_read=false) should be blocked for target address");
    }

    #[test]
    fn test_wave_ram_cgb_enabled_ch3() {
        let mut apu = setup_apu_with_mode(SystemMode::CGB_D);
        apu.write_byte(NR30_ADDR, 0x80, false);
        apu.write_byte(NR34_ADDR, 0b10000000, false);

        assert_eq!(apu.channel3.current_wave_ram_byte_index(), 0);
        // CGB: Reads and writes always redirect to current_sample_index / 2, regardless of wave_form_just_read

        // Test Read
        assert_eq!(apu.read_byte(WAVE_PATTERN_RAM_START_ADDR + 5), apu.wave_ram[0], "CGB Read should redirect");

        // Test Write
        apu.write_byte(WAVE_PATTERN_RAM_START_ADDR + 5, 0xCC, false);
        assert_eq!(apu.wave_ram[0], 0xCC, "CGB Write should redirect");
        assert_ne!(apu.wave_ram[5], 0xCC, "CGB Write should not write to address directly");
    }

    #[test]
    fn test_wave_ram_agb_enabled_ch3() {
        let mut apu = setup_apu_with_mode(SystemMode::AGB);
        apu.write_byte(NR30_ADDR, 0x80, false, 0);
        apu.write_byte(NR34_ADDR, 0b10000000, false, 0);

        // AGB: Reads always 0xFF, Writes are blocked
        assert_eq!(apu.read_byte(WAVE_PATTERN_RAM_START_ADDR + 0), 0xFF, "AGB Read should be 0xFF");
        assert_eq!(apu.read_byte(WAVE_PATTERN_RAM_START_ADDR + 5), 0xFF, "AGB Read should be 0xFF");

        let original_wave_val_0 = apu.wave_ram[0];
        let original_wave_val_5 = apu.wave_ram[5];
        apu.write_byte(WAVE_PATTERN_RAM_START_ADDR + 5, 0xCC, false, 0);
        assert_eq!(apu.wave_ram[0], original_wave_val_0, "AGB Write should be blocked for current index");
        assert_eq!(apu.wave_ram[5], original_wave_val_5, "AGB Write should be blocked for target address");
    }
}

#[cfg(test)]
mod frame_sequencer_timing_tests {
    use crate::apu::{Apu, NR52_ADDR, CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK};
    use crate::bus::SystemMode;

    #[test]
    fn test_lf_div_generation() {
        let mut apu = Apu::new(SystemMode::DMG); // System mode doesn't affect lf_div directly

        assert_eq!(apu.lf_div, 0, "Initial lf_div should be 0");

        apu.tick(1); // 1 T-cycle
        assert_eq!(apu.lf_div, 0, "lf_div after 1 T-cycle (master_t_count=1, lf_div=(1/2)%2=0)");

        apu.tick(1); // 2 T-cycles total
        assert_eq!(apu.lf_div, 1, "lf_div after 2 T-cycles (master_t_count=2, lf_div=(2/2)%2=1)");

        apu.tick(1); // 3 T-cycles total
        assert_eq!(apu.lf_div, 1, "lf_div after 3 T-cycles (master_t_count=3, lf_div=(3/2)%2=1)");

        apu.tick(1); // 4 T-cycles total
        assert_eq!(apu.lf_div, 0, "lf_div after 4 T-cycles (master_t_count=4, lf_div=(4/2)%2=0)");

        apu.tick(4); // 8 T-cycles total
        assert_eq!(apu.lf_div, 0, "lf_div after 8 T-cycles (master_t_count=8, lf_div=(8/2)%2=0)");

        apu.tick(5); // 13 T-cycles total
        assert_eq!(apu.lf_div, 0, "lf_div after 13 T-cycles (master_t_count=13, lf_div=(13/2)%2=0)");
    }

    #[test]
    fn test_skip_div_event_glitch_normal_speed() {
        let mut apu = Apu::new(SystemMode::DMG); // DMG, normal speed

        // Scenario 1: Glitch should occur (DIV bit 12 is high)
        // Simulate DIV counter having bit 12 set (0x1000)
        let div_counter_glitch = 0x1000;
        apu.write_byte(NR52_ADDR, 0x80, false, div_counter_glitch); // Power ON APU, normal speed

        assert!(apu.skip_next_frame_sequencer_increment, "skip_next_fs_inc should be true for glitch");
        assert_eq!(apu.frame_sequencer_step, 1, "FS step should be 1 for glitch");

        // First tick after power on: should skip clocking events
        apu.tick(CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK);
        assert!(apu.frame_sequencer_clock_is_being_skipped, "FS clock should have been marked as skipped this tick");
        // The actual clock_frame_sequencer where events are clocked is skipped,
        // but the step advancement for the *next* cycle still happens inside the skipped path.
        // So after this tick, frame_sequencer_step should be 2 (1 from glitch + 1 from skipped step processing)
        assert_eq!(apu.frame_sequencer_step, 2, "FS step should be 2 after skipped tick");
        assert!(!apu.skip_next_frame_sequencer_increment, "skip_next_fs_inc should be reset");
        assert!(!apu.frame_sequencer_clock_is_being_skipped, "frame_sequencer_clock_is_being_skipped should be reset for next tick");

        // Second tick: should clock events normally at step 2
        let ch1_len_before = apu.channel1.get_length_counter();
        apu.tick(CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK);
        assert_ne!(ch1_len_before, apu.channel1.get_length_counter(), "Channel 1 length should have been clocked on step 2");
        assert_eq!(apu.frame_sequencer_step, 3, "FS step should be 3 after normal tick");


        // Scenario 2: Glitch should NOT occur (DIV bit 12 is low)
        apu.write_byte(NR52_ADDR, 0x00, false, 0); // Power OFF APU to reset state
        let div_counter_no_glitch = 0x0000;
        apu.write_byte(NR52_ADDR, 0x80, false, div_counter_no_glitch); // Power ON APU

        assert!(!apu.skip_next_frame_sequencer_increment, "skip_next_fs_inc should be false for no glitch");
        assert_eq!(apu.frame_sequencer_step, 0, "FS step should be 0 for no glitch");

        // First tick after power on: should clock events normally at step 0
        let ch1_len_before_no_glitch = apu.channel1.get_length_counter();
        apu.tick(CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK);
        assert!(!apu.frame_sequencer_clock_is_being_skipped, "FS clock should not be skipped");
        assert_ne!(ch1_len_before_no_glitch, apu.channel1.get_length_counter(), "Channel 1 length should have been clocked on step 0 (no glitch)");
        assert_eq!(apu.frame_sequencer_step, 1, "FS step should be 1 after normal tick (no glitch)");
    }

    // TODO: Add test for skip_div_event_glitch_double_speed if is_double_speed can be controlled for APU.
}
