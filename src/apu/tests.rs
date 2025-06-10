// src/apu/tests.rs
#[cfg(test)]
mod tests {
    use crate::apu::{Apu, NR11_ADDR, NR12_ADDR, NR14_ADDR, NR52_ADDR};
    use crate::apu::{NR30_ADDR, NR31_ADDR, NR32_ADDR, NR33_ADDR, NR34_ADDR};
    use crate::models::GameBoyModel; // Import GameBoyModel

    const WAVE_START: u16 = 0xFF30;

    fn create_apu_and_power_on() -> Apu {
        let mut apu = Apu::new(GameBoyModel::DMG); // Pass a default model
        apu.write_byte(NR52_ADDR, 0x80); // Power on APU
        apu
    }

    #[test]
    fn test_ch1_trigger_and_initial_status() {
        let mut apu = create_apu_and_power_on();
        apu.write_byte(NR11_ADDR, 0b10000001);
        apu.write_byte(NR12_ADDR, 0xF3);
        apu.write_byte(NR14_ADDR, 0b11000000);
        for _ in 0..(8192 * 2) {
            apu.tick(1);
        }
        let nr52_val_after_trigger = apu.read_byte(NR52_ADDR);
        assert_ne!(
            nr52_val_after_trigger & 0x01,
            0x00,
            "CH1 should be on after trigger and some ticks"
        );
    }

    // Placeholder for more tests
    #[test]
    fn basic_channel2_trigger() {
        let mut apu = create_apu_and_power_on();
        apu.write_byte(0xFF16, 0b10000001);
        apu.write_byte(0xFF17, 0xF3);
        apu.write_byte(0xFF19, 0b11000000);
        for _ in 0..(8192 * 2) {
            apu.tick(1);
        }
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

        // Tick APU for frequency_timer to hit zero (initial value is (2049-2047)*2 = 4 for period 0x7FF)
        // This allows wave_form_just_read to become true for DMG model.
        apu.tick(4); // Enough ticks for the first sample to be processed and wave_form_just_read to be set.

        // CH3 active, sample_index should have advanced.
        // The write should now be accepted on DMG because wave_form_just_read is true.
        // The read should also be accepted.
        // The current_wave_ram_byte_index() will depend on how many samples advanced.
        // After 4 ticks, sample_index becomes 1 (0->1). current_wave_ram_byte_index() is (1/2) = 0.

        apu.write_byte(WAVE_START + 1, 0xAA); // Redirected to wave_ram[0]
        assert_eq!(
            apu.read_byte(WAVE_START + 0),
            0xAA,
            "Read after write to redirected WaveRAM[0] failed"
        );

        // Advance two more *samples*. Each sample takes `frequency_timer` ticks.
        // Initial freq_timer was 4. It reloads to (2048-2047)*2 = 2.
        // So 2 ticks per sample now.
        // To advance sample_index from 1 to 2 (1 sample): apu.tick(2)
        // To advance sample_index from 2 to 3 (1 sample): apu.tick(2)
        // Total 4 ticks to advance two samples.
        apu.tick(2); // sample_index becomes 2. current_idx = 1. wave_form_just_read = true.
        apu.tick(2); // sample_index becomes 3. current_idx = 1. wave_form_just_read = true.

        apu.write_byte(WAVE_START + 2, 0xBB); // Redirected to wave_ram[1]
        assert_eq!(
            apu.read_byte(WAVE_START + 0x01),
            0xBB,
            "Read after write to redirected WaveRAM[1] failed"
        );
    }
}
