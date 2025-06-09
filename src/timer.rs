// src/timer.rs
use crate::interrupts::TIMER_IRQ_BIT;

const DIV_THRESHOLD: u32 = 256; // System clock cycles for DIV increment (4.19MHz / 16384Hz)

pub struct Timer {
    div: u8, // FF04 - Divider Register
    tima: u8, // FF05 - Timer Counter
    tma: u8,  // FF06 - Timer Modulo
    tac: u8,  // FF07 - Timer Control

    div_clock_cycles: u32, // Internal counter for DIV
    tima_clock_cycles: u32, // Internal counter for TIMA
    tima_overflow_occurred: bool, // New field for delayed interrupt
}

impl Timer {
    pub fn new() -> Self {
        Timer {
            div: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            div_clock_cycles: 0,
            tima_clock_cycles: 0,
            tima_overflow_occurred: false, // Initialize to false
        }
    }

    pub fn tick(&mut self, cycles: u32, interrupt_flag: &mut u8) {
        // Handle delayed interrupt from previous overflow
        if self.tima_overflow_occurred {
            *interrupt_flag |= 1 << TIMER_IRQ_BIT;
            self.tima_overflow_occurred = false;
        }

        // DIV Logic
        self.div_clock_cycles += cycles;
        while self.div_clock_cycles >= DIV_THRESHOLD {
            self.div = self.div.wrapping_add(1);
            self.div_clock_cycles -= DIV_THRESHOLD;
        }

        // TIMA Logic
        if self.is_timer_enabled() {
            self.tima_clock_cycles += cycles;
            let tima_threshold = self.get_tima_threshold();

            while self.tima_clock_cycles >= tima_threshold {
                // It's possible for TIMA to increment multiple times if many cycles are passed.
                // Each increment needs to be processed for potential overflow.
                self.tima_clock_cycles -= tima_threshold; // Consume cycles for one potential increment

                let (new_tima, overflowed) = self.tima.overflowing_add(1);
                self.tima = new_tima;

                if overflowed {
                    self.tima = self.tma; // Reload TIMA from TMA
                    // Instead of setting interrupt flag immediately, signal it for the next tick call.
                    self.tima_overflow_occurred = true;
                }
            }
        }

    }

    fn is_timer_enabled(&self) -> bool {
        (self.tac & 0b0000_0100) != 0
    }

    fn get_tima_threshold(&self) -> u32 {
        match self.tac & 0b0000_0011 { // Clock select bits
            0b00 => 1024, // 4096 Hz   (4194304 / 4096)
            0b01 => 16,   // 262144 Hz (4194304 / 262144)
            0b10 => 64,   // 65536 Hz  (4194304 / 65536)
            0b11 => 256,  // 16384 Hz  (4194304 / 16384)
            _ => unreachable!(), // Should not happen due to masking
        }
    }

    pub fn read_byte(&self, address: u16) -> u8 {
        match address {
            0xFF04 => self.div,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac,
            _ => {
                eprintln!("Warning: Read from unhandled timer address: {:#06X}", address);
                0xFF
            }
        }
    }

    pub fn write_byte(&mut self, address: u16, value: u8) {
        match address {
            0xFF04 => {
                self.div = 0; // Writing any value to DIV resets it to 0
                //ध्ययन Sameboy behavior (timer.c lines 103-112) suggests that on DIV write,
                // if the current tima_clock_cycles would have caused TIMA to increment
                // (i.e. >= threshold), TIMA *does* increment once before tima_clock_cycles is reset.
                // This is a subtle glitch. For now, we keep the simpler reset of tima_clock_cycles.
                // A more accurate implementation might need to check and apply this glitch.
                self.div_clock_cycles = 0; // Reset internal counter as well
                self.tima_clock_cycles = 0; // Also reset TIMA's clock accumulator
            }
            0xFF05 => self.tima = value,
            0xFF06 => self.tma = value,
            0xFF07 => {
                // If the timer is being disabled, or the frequency is changing,
                // it's a good idea to reset the TIMA clock accumulator.
                // This avoids unexpected immediate increments.
                if (self.tac & 0b111) != (value & 0b111) { // If enable or freq bits change
                    self.tima_clock_cycles = 0;
                }
                self.tac = value & 0b0000_0111; // Only bits 0-2 are used for TAC
            }
            _ => {
                eprintln!("Warning: Write to unhandled timer address: {:#06X} with value {:#04X}", address, value);
            }
        }
    }

    pub fn get_div_counter(&self) -> u16 {
        // Sameboy's div_counter is a u16 that increments every T-cycle for DIV register update.
        // Our self.div is u8 and increments every 256 T-cycles.
        // To emulate Sameboy's usage for APU glitch, we need the raw 16-bit counter.
        // self.div_clock_cycles counts T-cycles towards the next self.div increment.
        // So, (self.div as u16) << 8 would be the upper byte of a 16-bit DIV,
        // and div_clock_cycles / (DIV_THRESHOLD / 256) or similar might approximate the lower byte.
        // However, Sameboy's `div_divider` in APU is a separate 8-bit counter for APU events,
        // and the `div` register for the skip glitch is the main CPU DIV.
        // For the skip glitch, Sameboy reads `gb->div_timer`, which is a 16-bit free-running counter
        // that increments every T-cycle and whose upper 8 bits become FF04.
        // Our `div_clock_cycles` accumulates T-cycles for one DIV step (256 T-cycles).
        // So, `(self.div as u16) * 256 + self.div_clock_cycles` might be closer to a raw counter.
        // Let's assume for now the glitch relies on the upper bits primarily.
        // A more direct equivalent to Sameboy's `gb->div_timer` would be needed.
        // For now, returning a value based on `self.div` and `self.div_clock_cycles`.
        // DIV_THRESHOLD is 256 T-cycles. div_clock_cycles is current count within that.
        // So, (self.div as u16) << 8 | (self.div_clock_cycles / (DIV_THRESHOLD/256)) ? No, this is too complex.
        // Sameboy's div_timer is essentially the system T-cycle counter.
        // We don't have that directly in Timer struct.
        // The skip glitch needs bit 12 or 13 of this raw T-cycle counter.
        // Our `div` is `(raw_t_cycle_counter >> 8) & 0xFF`.
        // So `(self.div as u16) << 8` gives us the upper byte of the raw counter.
        // `self.div_clock_cycles` is `raw_t_cycle_counter % 256`.
        // So, the raw 16-bit DIV is effectively `(self.div as u16) << 8 | (self.div_clock_cycles % 256) as u8` but div_clock_cycles can be > 255 before reset
        // Let's construct the 16-bit DIV based on how it's formed:
        // DIV register (FF04) is the upper 8 bits of a 14-bit or 16-bit internal counter.
        // Pandocs: "This register is incremented at rate of 16384Hz (= CPU Clock / 256). So it acts as a 16-bit counter...
        // Writing any value to this register resets it to $00."
        // The div_clock_cycles counts up to DIV_THRESHOLD (256 system cycles).
        // A 16-bit counter `internal_div_counter` would be `self.div` as MSB and some LSB.
        // `div_clock_cycles` is the remainder for the current `div` value's 256 cycle period.
        // So, `(self.div as u16) << 8 | self.div_clock_cycles_within_current_div_step_lsb`.
        // This is tricky. For now, let's return a proxy that might work for the glitch.
        // The glitch needs bit 12 (normal speed) or 13 (double speed) of the *actual* 16-bit DIV counter.
        // If our `self.div` is the upper 8 bits, then `(self.div as u16) << 8` is part of it.
        // The lower bits are `self.div_clock_cycles`.
        // So, `internal_16bit_div = (self.div as u16) << 8 | (self.div_clock_cycles & 0xFF)` (approx, as div_clock_cycles is reset)
        // This is not directly what Sameboy uses. Sameboy has `gb->div_timer` which is a direct T-cycle counter.
        // We don't have a global T-cycle counter easily accessible here.
        // The Bus has `master_t_cycle_count` but that's for APU.
        // For now, this will be an approximation.
        // The important bits are bit 12 (0x1000) and 13 (0x2000).
        // These would correspond to certain values of `self.div` if `div_clock_cycles` is low.
        // e.g. bit 12 of 16-bit DIV is bit 4 of `self.div`. bit 13 is bit 5 of `self.div`.
        // This is likely what Sameboy's check simplifies to for the APU.
        // `(gb->div_timer & 0x1000)` or `(gb->div_timer & 0x2000)`
        // Let's return a value that allows checking these bits based on current self.div.
        // This means the APU will effectively check (self.timer.div & (1 << 4)) or (self.timer.div & (1 << 5)).
        (self.div as u16) << 8 // This provides the upper 8 bits. The APU can check its relevant bits.
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Imports Timer, TIMER_IRQ_BIT, DIV_THRESHOLD etc.

    fn assert_timer_regs(timer: &Timer, div: u8, tima: u8, tma: u8, tac: u8, msg: &str) {
        assert_eq!(timer.div, div, "{} - DIV mismatch", msg);
        assert_eq!(timer.tima, tima, "{} - TIMA mismatch", msg);
        assert_eq!(timer.tma, tma, "{} - TMA mismatch", msg);
        assert_eq!(timer.tac, tac, "{} - TAC mismatch", msg);
    }

    #[test]
    fn initial_state() {
        let timer = Timer::new();
        assert_timer_regs(&timer, 0, 0, 0, 0, "Initial state");
        assert_eq!(timer.div_clock_cycles, 0, "Initial div_clock_cycles");
        assert_eq!(timer.tima_clock_cycles, 0, "Initial tima_clock_cycles");
    }

    #[test]
    fn div_increment() {
        let mut timer = Timer::new();
        let mut if_reg = 0;

        // Tick just below threshold
        timer.tick(DIV_THRESHOLD - 1, &mut if_reg);
        assert_eq!(timer.div, 0, "DIV should not increment before threshold");
        assert_eq!(timer.div_clock_cycles, DIV_THRESHOLD - 1, "div_clock_cycles after partial tick");

        // Tick to reach threshold
        timer.tick(1, &mut if_reg);
        assert_eq!(timer.div, 1, "DIV should increment at threshold");
        assert_eq!(timer.div_clock_cycles, 0, "div_clock_cycles should reset after increment");

        // Tick multiple times threshold
        timer.tick(DIV_THRESHOLD * 3 + 50, &mut if_reg); // 3 full increments + 50 cycles
        assert_eq!(timer.div, 1 + 3, "DIV should increment multiple times");
        assert_eq!(timer.div_clock_cycles, 50, "div_clock_cycles after multiple increments");

        // Test wrapping
        timer.div = 0xFE;
        timer.div_clock_cycles = 0;
        timer.tick(DIV_THRESHOLD, &mut if_reg); // FE -> FF
        assert_eq!(timer.div, 0xFF, "DIV FF");
        timer.tick(DIV_THRESHOLD, &mut if_reg); // FF -> 00
        assert_eq!(timer.div, 0x00, "DIV wrap to 00");
    }

    #[test]
    fn div_write_resets() {
        let mut timer = Timer::new();
        let mut if_reg = 0;
        timer.tick(DIV_THRESHOLD * 5 + 10, &mut if_reg); // DIV = 5, div_clock_cycles = 10
        assert_eq!(timer.read_byte(0xFF04), 5, "DIV before reset");
        assert_ne!(timer.div_clock_cycles, 0, "div_clock_cycles should be non-zero before DIV write");

        timer.write_byte(0xFF04, 0xAB); // Write any value to DIV
        assert_eq!(timer.read_byte(0xFF04), 0, "DIV should be 0 after write");
        assert_eq!(timer.div, 0, "Internal div should be 0 after write");
        assert_eq!(timer.div_clock_cycles, 0, "div_clock_cycles should be 0 after DIV write");
    }

    #[test]
    fn tima_disabled() {
        let mut timer = Timer::new();
        let mut if_reg = 0;
        timer.write_byte(0xFF07, 0b000); // Timer disabled, freq 0

        timer.tick(2000, &mut if_reg); // Tick many cycles
        assert_eq!(timer.tima, 0, "TIMA should not increment when timer disabled");
        assert_eq!(timer.tima_clock_cycles, 0, "tima_clock_cycles should not accumulate when disabled");
    }

    fn test_tima_frequency(freq_bits: u8, threshold: u32) {
        let mut timer = Timer::new();
        let mut if_reg = 0;
        timer.write_byte(0xFF07, 0b100 | freq_bits); // Timer enabled, specific frequency
        assert_eq!(timer.tac, 0b100 | freq_bits, "TAC should be set correctly");
        assert!(timer.is_timer_enabled(), "Timer should be enabled");
        assert_eq!(timer.get_tima_threshold(), threshold, "Threshold for freq {} incorrect", freq_bits);

        // Tick just below threshold
        timer.tick(threshold - 1, &mut if_reg);
        assert_eq!(timer.tima, 0, "TIMA shouldn't inc before threshold (freq {})", freq_bits);
        assert_eq!(timer.tima_clock_cycles, threshold -1, "tima_clock_cycles incorrect (freq {})", freq_bits);

        // Tick to reach threshold
        timer.tick(1, &mut if_reg);
        assert_eq!(timer.tima, 1, "TIMA should inc at threshold (freq {})", freq_bits);
        assert_eq!(timer.tima_clock_cycles, 0, "tima_clock_cycles should reset (freq {})", freq_bits);

        // Tick multiple times threshold
        timer.tima = 1; // reset tima for simpler check
        timer.tima_clock_cycles = 0;
        timer.tick(threshold * 2 + (threshold / 2), &mut if_reg); // 2 full increments + partial
        assert_eq!(timer.tima, 1 + 2, "TIMA multi-inc (freq {})", freq_bits);
        assert_eq!(timer.tima_clock_cycles, threshold / 2, "tima_clock_cycles multi-inc (freq {})", freq_bits);
    }

    #[test]
    fn tima_freq_00() { test_tima_frequency(0b00, 1024); } // 4096 Hz
    #[test]
    fn tima_freq_01() { test_tima_frequency(0b01, 16); }   // 262144 Hz
    #[test]
    fn tima_freq_10() { test_tima_frequency(0b10, 64); }   // 65536 Hz
    #[test]
    fn tima_freq_11() { test_tima_frequency(0b11, 256); }  // 16384 Hz

    #[test]
    fn tac_write_masking_and_reset_tima_clock() {
        let mut timer = Timer::new();
        let mut if_reg = 0;

        // Enable timer and set some frequency, let tima_clock_cycles accumulate
        timer.write_byte(0xFF07, 0b0000_0101); // Enable, Freq 1 (16 cycle threshold)
        timer.tick(10, &mut if_reg);
        assert_eq!(timer.tima_clock_cycles, 10);
        assert_eq!(timer.tac, 0b101);

        // Write to TAC with unused bits set, and change frequency
        // This should mask unused bits and reset tima_clock_cycles because frequency changed
        timer.write_byte(0xFF07, 0b1111_1110); // Request enable, Freq 2 (64 cycle threshold)
        assert_eq!(timer.tac, 0b110, "TAC should mask unused bits (0b11111110 -> 0b110)");
        assert_eq!(timer.tima_clock_cycles, 0, "tima_clock_cycles should reset when TAC freq changes");

        // Tick again, tima_clock_cycles should start from 0 for new freq
        timer.tick(5, &mut if_reg);
        assert_eq!(timer.tima_clock_cycles, 5);

        // Disable timer, should also reset tima_clock_cycles
        timer.tick(30, &mut if_reg); // accumulate some more
        assert_ne!(timer.tima_clock_cycles, 0);
        timer.write_byte(0xFF07, 0b0000_0010); // Disable, Freq 2 (but doesn't matter)
        assert_eq!(timer.tac, 0b010, "TAC should be 0b010 (disabled)");
        assert!(!timer.is_timer_enabled());
        assert_eq!(timer.tima_clock_cycles, 0, "tima_clock_cycles should reset when timer is disabled via TAC");
    }

    #[test]
    fn tima_overflow_and_interrupt() {
        let mut timer = Timer::new();
        let mut if_reg = 0;

        timer.write_byte(0xFF06, 0xAB); // TMA = 0xAB
        timer.write_byte(0xFF07, 0b101); // Enabled, Freq 1 (16 T-cycle threshold)
        timer.tima = 0xFE; // TIMA close to overflow

        // Tick to increment TIMA from FE to FF
        timer.tick(16, &mut if_reg);
        assert_eq!(timer.tima, 0xFF, "TIMA should be FF");
        assert_eq!(if_reg & (1 << TIMER_IRQ_BIT), 0, "Interrupt flag should not be set yet");

        // Tick to increment TIMA from FF to 00 (overflow)
        timer.tick(16, &mut if_reg); // TIMA overflows, tima_overflow_occurred becomes true
        assert_eq!(timer.tima, 0xAB, "TIMA should be reset to TMA (0xAB) after overflow");
        assert_eq!(if_reg & (1 << TIMER_IRQ_BIT), 0, "Interrupt flag should NOT be set yet (delay)");
        assert!(timer.tima_overflow_occurred, "tima_overflow_occurred should be true after overflow");
        assert_eq!(timer.tima_clock_cycles, 0, "tima_clock_cycles should reset on overflow tick");

        // Next tick call to process the pending overflow and set the interrupt flag
        timer.tick(1, &mut if_reg); // Minimal tick to process the flag
        assert_ne!(if_reg & (1 << TIMER_IRQ_BIT), 0, "Timer interrupt flag should NOW be set");
        assert!(!timer.tima_overflow_occurred, "tima_overflow_occurred should be false after processing");


        // Reset interrupt flag for next test
        if_reg = 0;
        timer.tima = 0xFF; // Setup for another overflow
        timer.write_byte(0xFF06, 0x00); // TMA = 0x00

        timer.tick(16, &mut if_reg); // TIMA overflows, tima_overflow_occurred becomes true
        assert_eq!(timer.tima, 0x00, "TIMA should be reset to TMA (0x00) after overflow");
        assert_eq!(if_reg & (1 << TIMER_IRQ_BIT), 0, "Interrupt flag should NOT be set yet (TMA=0x00, delay)");
        assert!(timer.tima_overflow_occurred, "tima_overflow_occurred should be true after overflow (TMA=0x00)");

        timer.tick(1, &mut if_reg); // Minimal tick to process the flag
        assert_ne!(if_reg & (1 << TIMER_IRQ_BIT), 0, "Timer interrupt flag should NOW be set (TMA=0x00)");
        assert!(!timer.tima_overflow_occurred, "tima_overflow_occurred should be false after processing (TMA=0x00)");
    }

    #[test]
    fn tma_read_write() {
        let mut timer = Timer::new();
        assert_eq!(timer.read_byte(0xFF06), 0, "Initial TMA should be 0");
        timer.write_byte(0xFF06, 0xDC);
        assert_eq!(timer.read_byte(0xFF06), 0xDC, "TMA should be 0xDC after write");
        assert_eq!(timer.tma, 0xDC, "Internal tma should be 0xDC");
    }

    #[test]
    fn tima_read_write() {
        let mut timer = Timer::new();
        assert_eq!(timer.read_byte(0xFF05), 0, "Initial TIMA should be 0");
        timer.write_byte(0xFF05, 0x7F);
        assert_eq!(timer.read_byte(0xFF05), 0x7F, "TIMA should be 0x7F after write");
        assert_eq!(timer.tima, 0x7F, "Internal tima should be 0x7F");
    }

    #[test]
    fn tac_read_write() {
        let mut timer = Timer::new();
        assert_eq!(timer.read_byte(0xFF07), 0, "Initial TAC should be 0");
        timer.write_byte(0xFF07, 0b111); // Enable, Freq 3
        assert_eq!(timer.read_byte(0xFF07), 0b111, "TAC should be 0b111 after write");
        assert_eq!(timer.tac, 0b111, "Internal tac should be 0b111");

        timer.write_byte(0xFF07, 0b1111_1101); // Enable, Freq 1, with upper bits set
        assert_eq!(timer.read_byte(0xFF07), 0b101, "TAC should mask upper bits (0b11111101 -> 0b101)");
        assert_eq!(timer.tac, 0b101, "Internal tac should be 0b101 after masked write");
    }

    #[test]
    fn tima_reset_by_div_write() {
        let mut timer = Timer::new();
        let mut if_reg = 0;

        // Enable timer, Freq 0 (1024 cycle threshold for TIMA)
        timer.write_byte(0xFF07, 0b0000_0100);
        assert_eq!(timer.tac, 0b100);
        let tima_thresh = timer.get_tima_threshold(); // Should be 1024
        assert_eq!(tima_thresh, 1024);

        // Tick for less than half the threshold
        timer.tick(tima_thresh / 3, &mut if_reg);
        assert_eq!(timer.tima, 0, "TIMA should not have incremented yet");
        assert_eq!(timer.tima_clock_cycles, tima_thresh / 3, "TIMA clock cycles should have accumulated");
        assert_ne!(timer.tima_clock_cycles, 0, "Sanity check: tima_clock_cycles should not be 0 before DIV write");

        // Write to DIV
        timer.write_byte(0xFF04, 0xAB); // Value doesn't matter, resets DIV and its clock

        assert_eq!(timer.div, 0, "DIV should be reset");
        assert_eq!(timer.div_clock_cycles, 0, "DIV clock cycles should be reset");
        assert_eq!(timer.tima_clock_cycles, 0, "TIMA clock cycles SHOULD BE RESET by DIV write");

        // Tick just under the threshold. TIMA should not increment.
        timer.tick(tima_thresh - 1, &mut if_reg);
        assert_eq!(timer.tima, 0, "TIMA should not increment after DIV write and partial tick");
        assert_eq!(timer.tima_clock_cycles, tima_thresh - 1);

        // Tick by 1 more cycle. TIMA should now increment.
        timer.tick(1, &mut if_reg);
        assert_eq!(timer.tima, 1, "TIMA should increment after DIV write and full threshold tick");
        assert_eq!(timer.tima_clock_cycles, 0);

        // Further check: ensure DIV still works
        timer.tick(DIV_THRESHOLD * 2, &mut if_reg);
        assert_eq!(timer.div, 6, "DIV should still be working");
    }
}
