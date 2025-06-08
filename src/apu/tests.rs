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
