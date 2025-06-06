// src/apu/tests.rs
#[cfg(test)]
mod tests {
    use crate::apu::{Apu, NR11_ADDR, NR12_ADDR, NR14_ADDR, NR52_ADDR};

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

}
