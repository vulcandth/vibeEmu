// src/apu.rs

pub mod channel1; // Declare the channel1 submodule
pub mod channel2; // Declare the channel2 submodule
pub mod channel3; // Declare the channel3 submodule
pub mod channel4; // Declare the channel4 submodule
use self::channel1::Channel1; // Import the Channel1 struct
use self::channel2::Channel2; // Import the Channel2 struct
use self::channel3::Channel3; // Import the Channel3 struct
use self.channel4::Channel4; // Import the Channel4 struct
use log::debug; // Assuming log crate is available

// APU Clocking Constants
const CPU_CLOCK_HZ: u32 = 4194304;
const FRAME_SEQUENCER_FREQUENCY_HZ: u32 = 512;
const CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK: u32 = CPU_CLOCK_HZ / FRAME_SEQUENCER_FREQUENCY_HZ;

// Register Address Constants
const NR10_ADDR: u16 = 0xFF10; // Channel 1 Sweep register
const NR11_ADDR: u16 = 0xFF11; // Channel 1 Sound length/Wave pattern duty
const NR12_ADDR: u16 = 0xFF12; // Channel 1 Volume Envelope
const NR13_ADDR: u16 = 0xFF13; // Channel 1 Frequency lo
const NR14_ADDR: u16 = 0xFF14; // Channel 1 Frequency hi

const NR21_ADDR: u16 = 0xFF16; // Channel 2 Sound length/Wave pattern duty
const NR22_ADDR: u16 = 0xFF17; // Channel 2 Volume Envelope
const NR23_ADDR: u16 = 0xFF18; // Channel 2 Frequency lo
const NR24_ADDR: u16 = 0xFF19; // Channel 2 Frequency hi

const NR30_ADDR: u16 = 0xFF1A; // Channel 3 Sound on/off
const NR31_ADDR: u16 = 0xFF1B; // Channel 3 Sound length
const NR32_ADDR: u16 = 0xFF1C; // Channel 3 Select output level
const NR33_ADDR: u16 = 0xFF1D; // Channel 3 Frequency lo
const NR34_ADDR: u16 = 0xFF1E; // Channel 3 Frequency hi

const NR41_ADDR: u16 = 0xFF20; // Channel 4 Sound length
const NR42_ADDR: u16 = 0xFF21; // Channel 4 Volume Envelope
const NR43_ADDR: u16 = 0xFF22; // Channel 4 Polynomial counter
const NR44_ADDR: u16 = 0xFF23; // Channel 4 Counter/consecutive; Initial

const NR50_ADDR: u16 = 0xFF24; // Channel control / ON-OFF / Volume
const NR51_ADDR: u16 = 0xFF25; // Selection of Sound output terminal
const NR52_ADDR: u16 = 0xFF26; // Sound on/off

const WAVE_PATTERN_RAM_START_ADDR: u16 = 0xFF30;
const WAVE_PATTERN_RAM_END_ADDR: u16 = 0xFF3F;

// Channel 1 Registers (NR1x)
#[derive(Debug, Clone, Copy)]
pub(super) struct Nr10 { // Made pub(super)
    sweep_time: u8,       // Bits 6-4
    sweep_direction: u8,  // Bit 3 (0: Addition, 1: Subtraction)
    sweep_shift: u8,      // Bits 2-0
}

impl Nr10 {
    pub(super) fn new() -> Self {
        Self {
            sweep_time: 0,
            sweep_direction: 0,
            sweep_shift: 0,
        }
    }

    pub(super) fn read(&self) -> u8 {
        0x80 | (self.sweep_time << 4) | (self.sweep_direction << 3) | self.sweep_shift
    }

    pub(super) fn write(&mut self, value: u8) {
        self.sweep_time = (value >> 4) & 0x07;
        self.sweep_direction = (value >> 3) & 0x01;
        self.sweep_shift = value & 0x07;
    }

    pub(super) fn sweep_period(&self) -> u8 {
        self.sweep_time
    }

    pub(super) fn sweep_shift_val(&self) -> u8 {
        self.sweep_shift
    }

    pub(super) fn sweep_direction_is_increase(&self) -> bool {
        self.sweep_direction == 0
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr11 { // Made pub(super)
    wave_pattern_duty: u8, // Bits 7-6
    sound_length_data: u8, // Bits 5-0
}

impl Nr11 {
    pub(super) fn new() -> Self {
        Self {
            wave_pattern_duty: 0b10, // 50%
            sound_length_data: 0x3F, // Max length
        }
    }

    pub(super) fn read(&self) -> u8 {
        (self.wave_pattern_duty << 6) | self.sound_length_data
    }

    pub(super) fn write(&mut self, value: u8) {
        self.wave_pattern_duty = (value >> 6) & 0x03;
        self.sound_length_data = value & 0x3F;
    }

    pub(super) fn initial_length_timer_val(&self) -> u8 {
        self.sound_length_data
    }

    pub(super) fn wave_pattern_duty_val(&self) -> u8 {
        self.wave_pattern_duty
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr12 { // Made pub(super)
    initial_volume: u8,   // Bits 7-4
    envelope_direction: u8, // Bit 3
    envelope_period: u8,  // Bits 2-0
}

impl Nr12 {
    pub(super) fn new() -> Self {
        Self {
            initial_volume: 0,
            envelope_direction: 0,
            envelope_period: 0,
        }
    }

    pub(super) fn read(&self) -> u8 {
        (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period
    }

    pub(super) fn write(&mut self, value: u8) {
        self.initial_volume = (value >> 4) & 0x0F;
        self.envelope_direction = (value >> 3) & 0x01;
        self.envelope_period = value & 0x07;
    }

    pub(super) fn initial_volume_val(&self) -> u8 {
        self.initial_volume
    }

    pub(super) fn dac_power(&self) -> bool {
        ((self.initial_volume << 4) | (self.envelope_direction << 3)) & 0xF8 != 0
    }

    pub(super) fn envelope_period_val(&self) -> u8 {
        self.envelope_period
    }

    pub(super) fn envelope_direction_is_increase(&self) -> bool {
        self.envelope_direction == 1
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr13 { // Made pub(super)
    freq_lo: u8,
}

impl Nr13 {
    pub(super) fn new() -> Self {
        Self { freq_lo: 0x00 }
    }

    pub(super) fn read(&self) -> u8 { // This is the read from CPU perspective
        0xFF
    }

    pub(super) fn write(&mut self, value: u8) {
        self.freq_lo = value;
    }

    pub(super) fn freq_lo_val(&self) -> u8 { // For internal APU use
        self.freq_lo
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr14 { // Made pub(super)
    trigger: bool,          // Bit 7 (Write-Only)
    length_enable: bool,    // Bit 6 (R/W)
    freq_hi: u8,            // Bits 2-0 (Write-Only)
}

impl Nr14 {
    pub(super) fn new() -> Self {
        Self {
            trigger: false,
            length_enable: false,
            freq_hi: 0,
        }
    }

    pub(super) fn read(&self) -> u8 {
        (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF
    }

    pub(super) fn write(&mut self, value: u8) {
        self.trigger = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        self.freq_hi = value & 0x07;
    }

    pub(super) fn is_length_enabled(&self) -> bool {
        self.length_enable
    }

    pub(super) fn frequency_msb_val(&self) -> u8 {
        self.freq_hi
    }

    pub(super) fn write_frequency_msb(&mut self, val: u8) {
        self.freq_hi = val & 0x07;
    }

    pub(super) fn is_triggered(&self) -> bool {
        self.trigger
    }

    pub(super) fn clear_trigger_flag(&mut self) {
        self.trigger = false;
    }
}

// Nr2x, Nr3x, Nr4x structs are below, unchanged for now by this specific set of helpers
// Nr2x register structs with helpers
#[derive(Debug, Clone, Copy)]
pub(super) struct Nr21 {
    wave_pattern_duty: u8,
    sound_length_data: u8,
}

impl Nr21 {
    pub(super) fn new() -> Self {
        Self {
            wave_pattern_duty: 0b00, // Default 12.5%
            sound_length_data: 0x3F, // Max length
        }
    }

    pub(super) fn read(&self) -> u8 {
        (self.wave_pattern_duty << 6) | self.sound_length_data
    }

    pub(super) fn write(&mut self, value: u8) {
        self.wave_pattern_duty = (value >> 6) & 0x03;
        self.sound_length_data = value & 0x3F;
    }

    pub(super) fn initial_length_timer_val(&self) -> u8 {
        self.sound_length_data
    }

    pub(super) fn wave_pattern_duty_val(&self) -> u8 {
        self.wave_pattern_duty
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr22 {
    initial_volume: u8,
    envelope_direction: u8,
    envelope_period: u8,
}

impl Nr22 {
    pub(super) fn new() -> Self {
        Self {
            initial_volume: 0,
            envelope_direction: 0,
            envelope_period: 0,
        }
    }

    pub(super) fn read(&self) -> u8 {
        (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period
    }

    pub(super) fn write(&mut self, value: u8) {
        self.initial_volume = (value >> 4) & 0x0F;
        self.envelope_direction = (value >> 3) & 0x01;
        self.envelope_period = value & 0x07;
    }

    pub(super) fn initial_volume_val(&self) -> u8 {
        self.initial_volume
    }

    pub(super) fn dac_power(&self) -> bool {
        ((self.initial_volume << 4) | (self.envelope_direction << 3)) & 0xF8 != 0
    }

    pub(super) fn envelope_period_val(&self) -> u8 {
        self.envelope_period
    }

    pub(super) fn envelope_direction_is_increase(&self) -> bool {
        self.envelope_direction == 1
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr23 {
    freq_lo: u8,
}

impl Nr23 {
    pub(super) fn new() -> Self {
        Self { freq_lo: 0x00 }
    }

    pub(super) fn read(&self) -> u8 { // CPU read
        0xFF
    }

    pub(super) fn write(&mut self, value: u8) {
        self.freq_lo = value;
    }

    pub(super) fn freq_lo_val(&self) -> u8 { // Internal APU use
        self.freq_lo
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr24 {
    trigger: bool,
    length_enable: bool,
    freq_hi: u8,
}

impl Nr24 {
    pub(super) fn new() -> Self {
        Self {
            trigger: false,
            length_enable: false,
            freq_hi: 0,
        }
    }

    pub(super) fn read(&self) -> u8 {
        (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF
    }

    pub(super) fn write(&mut self, value: u8) {
        self.trigger = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        self.freq_hi = value & 0x07;
    }

    pub(super) fn is_length_enabled(&self) -> bool {
        self.length_enable
    }

    pub(super) fn frequency_msb_val(&self) -> u8 {
        self.freq_hi
    }

    pub(super) fn is_triggered(&self) -> bool {
        self.trigger
    }

    pub(super) fn clear_trigger_flag(&mut self) {
        self.trigger = false;
    }
}

// End of NR2x structs

// Channel 3 Registers (NR3x)
#[derive(Debug, Clone, Copy)]
pub(super) struct Nr30 { // Made pub(super)
    sound_on: bool, // Bit 7, effectively DAC power
}

impl Nr30 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self { sound_on: false }
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super)
        (if self.sound_on { 0x80 } else { 0x00 }) | 0x7F // Bit 7 is data, 6-0 unused (read as 1)
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.sound_on = (value >> 7) & 0x01 != 0;
    }

    pub(super) fn dac_on(&self) -> bool {
        self.sound_on
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr31 { // Made pub(super)
    sound_length: u8, // Bits 7-0
}

impl Nr31 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self { sound_length: 0x00 } // Default to max length (256 steps from 0)
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super) - CPU read
        0xFF // Write-only
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.sound_length = value;
    }

    pub(super) fn sound_length_val(&self) -> u8 { // For internal APU use
        self.sound_length
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr32 { // Made pub(super)
    output_level: u8, // Bits 6-5
}

impl Nr32 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self { output_level: 0b00 } // Mute
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super)
        (self.output_level << 5) | 0x9F // Bits 6-5 data, others unused (read as 1)
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.output_level = (value >> 5) & 0x03;
    }

    pub(super) fn output_level_val(&self) -> u8 {
        self.output_level
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr33 { // Made pub(super)
    freq_lo: u8,
}

impl Nr33 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self { freq_lo: 0x00 }
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super) - CPU read
        0xFF // Write-only
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.freq_lo = value;
    }

    pub(super) fn freq_lo_val(&self) -> u8 { // For internal APU use
        self.freq_lo
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr34 { // Made pub(super)
    trigger: bool,       // Bit 7 (Write-Only)
    length_enable: bool, // Bit 6 (R/W)
    freq_hi: u8,         // Bits 2-0 (Write-Only)
}

impl Nr34 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self {
            trigger: false,
            length_enable: false,
            freq_hi: 0,
        }
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super)
        (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF // Bit 6 readable, others (WO/unused) read as 1
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.trigger = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        self.freq_hi = value & 0x07;
    }

    pub(super) fn is_length_enabled(&self) -> bool {
        self.length_enable
    }

    pub(super) fn frequency_msb_val(&self) -> u8 {
        self.freq_hi
    }

    // No write_frequency_msb for CH3 as it's directly written by CPU.

    pub(super) fn is_triggered(&self) -> bool {
        self.trigger
    }

    pub(super) fn clear_trigger_flag(&mut self) {
        self.trigger = false;
    }
}
// End of NR3x Structs and Impls

// Channel 4 Registers (NR4x)
#[derive(Debug, Clone, Copy)]
pub(super) struct Nr41 { // Made pub(super)
    sound_length_data: u8, // Bits 5-0
}

impl Nr41 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self { sound_length_data: 0x3F } // Max length
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super) - CPU read
        0xFF // Write-only (Pan Docs says NR41 W Channel 4 Sound Length)
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.sound_length_data = value & 0x3F;
    }

    pub(super) fn initial_length_timer_val(&self) -> u8 { // For internal APU use
        self.sound_length_data
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr42 { // Made pub(super)
    initial_volume: u8,
    envelope_direction: u8,
    envelope_period: u8,
}

impl Nr42 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self {
            initial_volume: 0,
            envelope_direction: 0,
            envelope_period: 0,
        }
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super)
        (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.initial_volume = (value >> 4) & 0x0F;
        self.envelope_direction = (value >> 3) & 0x01;
        self.envelope_period = value & 0x07;
    }

    pub(super) fn initial_volume_val(&self) -> u8 {
        self.initial_volume
    }

    pub(super) fn dac_power(&self) -> bool {
        ((self.initial_volume << 4) | (self.envelope_direction << 3)) & 0xF8 != 0
    }

    pub(super) fn envelope_period_val(&self) -> u8 {
        self.envelope_period
    }

    pub(super) fn envelope_direction_is_increase(&self) -> bool {
        self.envelope_direction == 1
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr43 { // Made pub(super)
    shift_clock_freq: u8, // Bits 7-4 (s)
    counter_width: u8,    // Bit 3   (0=15 bits, 1=7 bits)
    dividing_ratio: u8,   // Bits 2-0 (r)
}

impl Nr43 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self {
            shift_clock_freq: 0,
            counter_width: 0,
            dividing_ratio: 0,
        }
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super)
        (self.shift_clock_freq << 4) | (self.counter_width << 3) | self.dividing_ratio
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.shift_clock_freq = (value >> 4) & 0x0F;
        self.counter_width = (value >> 3) & 0x01;
        self.dividing_ratio = value & 0x07;
    }

    pub(super) fn clock_shift(&self) -> u8 {
        self.shift_clock_freq
    }

    pub(super) fn lfsr_width_is_7bit(&self) -> bool {
        self.counter_width == 1
    }

    pub(super) fn clock_divider_val(&self) -> u8 { // This is 'r'
        self.dividing_ratio
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr44 { // Made pub(super)
    trigger: bool,       // Bit 7 (Write-Only)
    length_enable: bool, // Bit 6 (R/W)
}

impl Nr44 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self {
            trigger: false,
            length_enable: false,
        }
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super)
        (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF // Bit 6 readable, others (WO/unused) read as 1
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.trigger = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
    }

    pub(super) fn is_length_enabled(&self) -> bool {
        self.length_enable
    }

    pub(super) fn is_triggered(&self) -> bool {
        self.trigger
    }

    pub(super) fn clear_trigger_flag(&mut self) {
        self.trigger = false;
    }
}
// End of NR4x Structs and Impls


#[derive(Debug, Clone, Copy)]
struct Nr52 {
    // FF26 - NR52 - Sound on/off (R/W)
    // Bit 7 - All sound on/off (0=Stop all sound circuits, 1=Enable all sound circuits) (R/W)
    // Bits 6-4 - Unused
    // Bit 3 - Sound 4 on/off status (Read Only)
    // Bit 2 - Sound 3 on/off status (Read Only)
    // Bit 1 - Sound 2 on/off status (Read Only)
    // Bit 0 - Sound 1 on/off status (Read Only)
    all_sound_on: bool,   // Bit 7 (R/W)
    ch4_status: bool,     // Bit 3 (Read-Only)
    ch3_status: bool,     // Bit 2 (Read-Only)
    ch2_status: bool,     // Bit 1 (Read-Only)
    ch1_status: bool,     // Bit 0 (Read-Only)
}

impl Nr52 {
    fn new() -> Self {
        Self {
            all_sound_on: false,
            ch4_status: false,
            ch3_status: false,
            ch2_status: false,
            ch1_status: false,
        }
    }

    // Helper method to check APU power status
    fn is_apu_enabled(&self) -> bool {
        self.all_sound_on
    }

    fn read(&self) -> u8 {
        // Bits 6-4 (Unused) read as 1.
        // Bits 3-0 are status bits (read-only).
        let status_byte = (if self.ch4_status { 0x08 } else { 0x00 })
            | (if self.ch3_status { 0x04 } else { 0x00 })
            | (if self.ch2_status { 0x02 } else { 0x00 })
            | (if self.ch1_status { 0x01 } else { 0x00 });
        (if self.all_sound_on { 0x80 } else { 0x00 }) | 0x70 | status_byte
        // Simpler: (if self.all_sound_on { 0x80 } else { 0x00 }) | 0x70 | (self.ch4_status as u8 * 8) ...
        // The 0x70 already includes unused bits as 1s, and status bits as 0s.
        // So, we just need to OR the actual status bits.
        // (if self.all_sound_on { 0x80 } else { 0x00 }) | // Bit 7
        // 0x70 | // Unused bits 1, status bits 0 template
        // ((self.ch4_status as u8) << 3) |
        // ((self.ch3_status as u8) << 2) |
        // ((self.ch2_status as u8) << 1) |
        // (self.ch1_status as u8)
        // This is getting complex. Let's do it bit by bit for clarity for read:
        // Bit 7: self.all_sound_on
        // Bit 6: 1 (Unused)
        // Bit 5: 1 (Unused)
        // Bit 4: 1 (Unused)
        // Bit 3: self.ch4_status
        // Bit 2: self.ch3_status
        // Bit 1: self.ch2_status
        // Bit 0: self.ch1_status
        (if self.all_sound_on { 0x80 } else { 0x00 }) |
        0x70 | // This correctly sets bits 6,5,4 to 1 and 3,2,1,0 to 0.
        (if self.ch4_status { 0x08 } else { 0x00 }) |
        (if self.ch3_status { 0x04 } else { 0x00 }) |
        (if self.ch2_status { 0x02 } else { 0x00 }) |
        (if self.ch1_status { 0x01 } else { 0x00 })

    }

    fn write(&mut self, value: u8) {
        // Only bit 7 is writable. Bits 3-0 are Read-Only status flags.
        // Bits 6-4 are unused.
    // let previous_power_state = self.all_sound_on; // Not needed with current Apu::write_byte logic
        self.all_sound_on = (value >> 7) & 0x01 != 0;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr51 {
    // FF25 - NR51 - Selection of Sound output terminal (R/W)
    // Bit 7 - Output sound 4 to SO2 terminal (0=No, 1=Yes)
    // Bit 6 - Output sound 3 to SO2 terminal (0=No, 1=Yes)
    // Bit 5 - Output sound 2 to SO2 terminal (0=No, 1=Yes)
    // Bit 4 - Output sound 1 to SO2 terminal (0=No, 1=Yes)
    // Bit 3 - Output sound 4 to SO1 terminal (0=No, 1=Yes)
    // Bit 2 - Output sound 3 to SO1 terminal (0=No, 1=Yes)
    // Bit 1 - Output sound 2 to SO1 terminal (0=No, 1=Yes)
    // Bit 0 - Output sound 1 to SO1 terminal (0=No, 1=Yes)
    ch4_to_so2: bool, // Bit 7
    ch3_to_so2: bool, // Bit 6
    ch2_to_so2: bool, // Bit 5
    ch1_to_so2: bool, // Bit 4
    ch4_to_so1: bool, // Bit 3
    ch3_to_so1: bool, // Bit 2
    ch2_to_so1: bool, // Bit 1
    ch1_to_so1: bool, // Bit 0
}

impl Nr51 {
    fn new() -> Self {
        // Default: 0xF3
        // CH4_SO2 (B7)=1, CH3_SO2 (B6)=1, CH2_SO2 (B5)=1, CH1_SO2 (B4)=1
        // CH4_SO1 (B3)=0, CH3_SO1 (B2)=0, CH2_SO1 (B1)=1, CH1_SO1 (B0)=1
        // 0b11110011 = 0xF3
        Self {
            ch4_to_so2: true,
            ch3_to_so2: true,
            ch2_to_so2: true,
            ch1_to_so2: true,
            ch4_to_so1: false,
            ch3_to_so1: false,
            ch2_to_so1: true,
            ch1_to_so1: true,
        }
    }

    fn read(&self) -> u8 {
        (if self.ch4_to_so2 { 0x80 } else { 0x00 })
            | (if self.ch3_to_so2 { 0x40 } else { 0x00 })
            | (if self.ch2_to_so2 { 0x20 } else { 0x00 })
            | (if self.ch1_to_so2 { 0x10 } else { 0x00 })
            | (if self.ch4_to_so1 { 0x08 } else { 0x00 })
            | (if self.ch3_to_so1 { 0x04 } else { 0x00 })
            | (if self.ch2_to_so1 { 0x02 } else { 0x00 })
            | (if self.ch1_to_so1 { 0x01 } else { 0x00 })
    }

    fn write(&mut self, value: u8) {
        self.ch4_to_so2 = (value >> 7) & 0x01 != 0;
        self.ch3_to_so2 = (value >> 6) & 0x01 != 0;
        self.ch2_to_so2 = (value >> 5) & 0x01 != 0;
        self.ch1_to_so2 = (value >> 4) & 0x01 != 0;
        self.ch4_to_so1 = (value >> 3) & 0x01 != 0;
        self.ch3_to_so1 = (value >> 2) & 0x01 != 0;
        self.ch2_to_so1 = (value >> 1) & 0x01 != 0;
        self.ch1_to_so1 = value & 0x01 != 0;
    }
}

// Sound Control Registers (NR5x)
#[derive(Debug, Clone, Copy)]
pub(super) struct Nr50 { // Made pub(super)
    vin_so2_enable: bool, // Bit 7
    so2_volume: u8,       // Bits 6-4
    vin_so1_enable: bool, // Bit 3
    so1_volume: u8,       // Bits 2-0
}

impl Nr50 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self {
            vin_so2_enable: false,
            so2_volume: 7,
            vin_so1_enable: false,
            so1_volume: 7,
        }
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super)
        (if self.vin_so2_enable { 0x80 } else { 0x00 })
            | (self.so2_volume << 4)
            | (if self.vin_so1_enable { 0x08 } else { 0x00 })
            | self.so1_volume
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.vin_so2_enable = (value >> 7) & 0x01 != 0;
        self.so2_volume = (value >> 4) & 0x07;
        self.vin_so1_enable = (value >> 3) & 0x01 != 0;
        self.so1_volume = value & 0x07;
    }

    pub(super) fn so1_output_level(&self) -> u8 {
        self.so1_volume
    }

    pub(super) fn so2_output_level(&self) -> u8 {
        self.so2_volume
    }
    // Vin enable helpers could be added if needed for advanced mixing
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr51 { // Made pub(super)
    ch4_to_so2: bool, // Bit 7
    ch3_to_so2: bool, // Bit 6
    ch2_to_so2: bool, // Bit 5
    ch1_to_so2: bool, // Bit 4
    ch4_to_so1: bool, // Bit 3
    ch3_to_so1: bool, // Bit 2
    ch2_to_so1: bool, // Bit 1
    ch1_to_so1: bool, // Bit 0
}

impl Nr51 {
    pub(super) fn new() -> Self { // Made pub(super)
        Self {
            ch4_to_so2: true, ch3_to_so2: true, ch2_to_so2: true, ch1_to_so2: true,
            ch4_to_so1: false, ch3_to_so1: false, ch2_to_so1: true, ch1_to_so1: true,
        }
    }

    pub(super) fn read(&self) -> u8 { // Made pub(super)
        (if self.ch4_to_so2 { 0x80 } else { 0x00 }) | (if self.ch3_to_so2 { 0x40 } else { 0x00 }) |
        (if self.ch2_to_so2 { 0x20 } else { 0x00 }) | (if self.ch1_to_so2 { 0x10 } else { 0x00 }) |
        (if self.ch4_to_so1 { 0x08 } else { 0x00 }) | (if self.ch3_to_so1 { 0x04 } else { 0x00 }) |
        (if self.ch2_to_so1 { 0x02 } else { 0x00 }) | (if self.ch1_to_so1 { 0x01 } else { 0x00 })
    }

    pub(super) fn write(&mut self, value: u8) { // Made pub(super)
        self.ch4_to_so2 = (value >> 7) & 0x01 != 0;
        self.ch3_to_so2 = (value >> 6) & 0x01 != 0;
        self.ch2_to_so2 = (value >> 5) & 0x01 != 0;
        self.ch1_to_so2 = (value >> 4) & 0x01 != 0;
        self.ch4_to_so1 = (value >> 3) & 0x01 != 0;
        self.ch3_to_so1 = (value >> 2) & 0x01 != 0;
        self.ch2_to_so1 = (value >> 1) & 0x01 != 0;
        self.ch1_to_so1 = value & 0x01 != 0;
    }

    // SO1 = Left, SO2 = Right
    pub(super) fn is_ch1_to_so1(&self) -> bool { self.ch1_to_so1 }
    pub(super) fn is_ch2_to_so1(&self) -> bool { self.ch2_to_so1 }
    pub(super) fn is_ch3_to_so1(&self) -> bool { self.ch3_to_so1 }
    pub(super) fn is_ch4_to_so1(&self) -> bool { self.ch4_to_so1 }
    pub(super) fn is_ch1_to_so2(&self) -> bool { self.ch1_to_so2 }
    pub(super) fn is_ch2_to_so2(&self) -> bool { self.ch2_to_so2 }
    pub(super) fn is_ch3_to_so2(&self) -> bool { self.ch3_to_so2 }
    pub(super) fn is_ch4_to_so2(&self) -> bool { self.ch4_to_so2 }
}


#[derive(Debug, Clone, Copy)]
struct Nr44 {
    // FF23 - NR44 - Channel 4 Counter/consecutive; Initial (R/W)
    // Bit 7   - Initial (Trigger) (Write Only) (1=Restart Sound)
    // Bit 6   - Counter/consecutive selection (R/W) (1=Stop output when length in NR41 expires)
    // Bits 5-0 - Unused
    trigger: bool,       // Bit 7 (Write-Only)
    length_enable: bool, // Bit 6 (R/W)
}

impl Nr44 {
    fn new() -> Self {
        // Default: Register value is not explicitly stated beyond "Only Bit 6 can be read."
        // Assume trigger=false, length_enable=false for internal state.
        // Pan Docs default for FF23 is 0xBF. This means bit 7 and 5-0 are 1s on read, bit 6 is 0.
        // So length_enable (bit 6) is 0. Trigger (bit 7) is WO.
        Self {
            trigger: false,
            length_enable: false,
        }
    }

    fn read(&self) -> u8 {
        // Only Bit 6 is readable. Unused bits (5-0) and WO bit 7 read as 1.
        // So, mask is 0xBF (10111111) if length_enable is 0.
        // Or 0xFF (11111111) if length_enable is 1.
        (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF
    }

    fn write(&mut self, value: u8) {
        self.trigger = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        // Bits 5-0 are not writable
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr43 {
    // FF22 - NR43 - Channel 4 Polynomial Counter (R/W)
    // Bit 7-4 - Shift Clock Frequency (s)
    // Bit 3   - Counter Step/Width (0=15 bits, 1=7 bits)
    // Bit 2-0 - Dividing Ratio of Frequencies (r)
    shift_clock_freq: u8, // Bits 7-4
    counter_width: u8,    // Bit 3 (0 for 15-bit, 1 for 7-bit)
    dividing_ratio: u8,   // Bits 2-0
}

impl Nr43 {
    fn new() -> Self {
        // Default: 0x00 (Shift clock 0, step 15-bits, ratio 0)
        Self {
            shift_clock_freq: 0,
            counter_width: 0,
            dividing_ratio: 0,
        }
    }

    fn read(&self) -> u8 {
        (self.shift_clock_freq << 4) | (self.counter_width << 3) | self.dividing_ratio
    }

    fn write(&mut self, value: u8) {
        self.shift_clock_freq = (value >> 4) & 0x0F;
        self.counter_width = (value >> 3) & 0x01;
        self.dividing_ratio = value & 0x07;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr42 {
    // FF21 - NR42 - Channel 4 Volume Envelope (R/W)
    // Bit 7-4 - Initial Volume of envelope (0-F) (0=No Sound)
    // Bit 3   - Envelope Direction (0=Decrease, 1=Increase)
    // Bit 2-0 - Number of envelope sweep (n) (0-7) (0=Stop)
    initial_volume: u8,   // Bits 7-4
    envelope_direction: u8, // Bit 3
    envelope_period: u8,  // Bits 2-0
}

impl Nr42 {
    fn new() -> Self {
        // Default: 0x00 (Initial Volume 0, Direction Decrease, Sweep 0)
        Self {
            initial_volume: 0,
            envelope_direction: 0,
            envelope_period: 0,
        }
    }

    fn read(&self) -> u8 {
        (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period
    }

    fn write(&mut self, value: u8) {
        self.initial_volume = (value >> 4) & 0x0F;
        self.envelope_direction = (value >> 3) & 0x01;
        self.envelope_period = value & 0x07;
    }
}

// Channel 4 Registers (NR4x)
#[derive(Debug, Clone, Copy)]
struct Nr41 {
    // FF20 - NR41 - Channel 4 Sound length (W)
    // Bits 5-0 - Sound Length Data (t1) (0-63)
    // Bits 7-6 - Unused
    sound_length_data: u8, // Bits 5-0
}

impl Nr41 {
    fn new() -> Self {
        // Default: Register value 0xFF (Unused bits 7-6 are 1, sound_length_data is 0x3F).
        // Sound Length itself is Write-Only.
        Self {
            sound_length_data: 0x3F, // Max length
        }
    }

    fn read(&self) -> u8 {
        // The sound length data is write-only. Unused bits (7-6) read as 1.
        // So, reading this register returns 0xFF if length bits are also 1 (which they are not necessarily).
        // Or C0 if length bits are 0.
        // Pan Docs states default value for register FF20 is $FF.
        // "NR41 W Channel 4 Sound Length" - gbhw
        // This implies reads should yield 0xFF.
        0xFF
    }

    fn write(&mut self, value: u8) {
        // Only bits 5-0 are writable
        self.sound_length_data = value & 0x3F;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr34 {
    // FF1E - NR34 - Channel 3 Frequency hi data (R/W)
    // Bit 7   - Initial (Trigger) (Write Only) (1=Restart Sound)
    // Bit 6   - Counter/consecutive selection (R/W) (1=Stop output when length in NR31 expires)
    // Bits 5-3 - Unused
    // Bit 2-0 - Higher 3 bits of 11-bit frequency (Write Only)
    trigger: bool,          // Bit 7 (Write-Only)
    length_enable: bool,    // Bit 6 (R/W)
    freq_hi: u8,            // Bits 2-0 (Write-Only)
}

impl Nr34 {
    fn new() -> Self {
        // Default: Unspecified. "Only Bit 6 can be read."
        // Writable fields are 0 initially.
        Self {
            trigger: false,
            length_enable: false,
            freq_hi: 0,
        }
    }

    fn read(&self) -> u8 {
        // Only Bit 6 is readable. Unused/Write-only bits read as 1.
        (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF
    }

    fn write(&mut self, value: u8) {
        self.trigger = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        self.freq_hi = value & 0x07;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr33 {
    // FF1D - NR33 - Channel 3 Frequency lo data (Write Only)
    // Bits 7-0 - Lower 8 bits of an 11-bit frequency.
    freq_lo: u8,
}

impl Nr33 {
    fn new() -> Self {
        // Default: Undefined as it's Write Only. Using 0x00.
        Self { freq_lo: 0x00 }
    }

    fn read(&self) -> u8 {
        // Write-only, typically returns 0xFF on read attempt.
        0xFF
    }

    fn write(&mut self, value: u8) {
        self.freq_lo = value;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr32 {
    // FF1C - NR32 - Channel 3 Select output level (R/W)
    // Bits 6-5 - Select output level (00=Mute, 01=100%, 10=50%, 11=25%)
    // Bits 7, 4-0 - Unused
    output_level: u8, // Bits 6-5
}

impl Nr32 {
    fn new() -> Self {
        // Default: 0x9F (Output level 00 (Mute), Unused bits read as 1)
        // field `output_level` stores just the 2 bits, so 0.
        Self { output_level: 0b00 }
    }

    fn read(&self) -> u8 {
        // Bits 6-5 are data, others are unused and read as 1.
        // Mask for unused bits is 0x9F (10011111)
        (self.output_level << 5) | 0x9F
    }

    fn write(&mut self, value: u8) {
        // Only bits 6-5 are writable.
        self.output_level = (value >> 5) & 0x03;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr31 {
    // FF1B - NR31 - Channel 3 Sound Length (W)
    // Bits 7-0 - Sound length data (t1) (0-255)
    // Actual length is (256-t1) cycles.
    sound_length: u8, // Bits 7-0
}

impl Nr31 {
    fn new() -> Self {
        // Default: Write Only. Power-up value is often 0x00 or 0xFF.
        // Let's use 0x00 for internal state. Max length.
        Self { sound_length: 0x00 }
    }

    fn read(&self) -> u8 {
        // Write-only, returns 0xFF on read.
        0xFF
    }

    fn write(&mut self, value: u8) {
        self.sound_length = value;
    }
}

// Channel 3 Registers (NR3x)
#[derive(Debug, Clone, Copy)]
struct Nr30 {
    // FF1A - NR30 - Channel 3 Sound on/off (R/W)
    // Bit 7 - Sound on/off (0=Playback off; 1=Playback on) (R/W - but see Pan Docs notes)
    // Bits 6-0 - Unused
    sound_on: bool, // Bit 7 // This is effectively the DAC power for Channel 3
}

impl Nr30 {
    fn new() -> Self {
        Self { sound_on: false }
    }

    // Helper method to check if Channel 3 DAC is enabled
    fn dac_on(&self) -> bool {
        self.sound_on
    }

    fn read(&self) -> u8 {
        // Bit 7 is the data, bits 6-0 are unused and read as 1.
        (if self.sound_on { 0x80 } else { 0x00 }) | 0x7F
    }

    fn write(&mut self, value: u8) {
        // Only bit 7 is writable.
        self.sound_on = (value >> 7) & 0x01 != 0;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr24 {
    // FF19 - NR24 - Channel 2 Frequency hi data (R/W)
    // Bit 7   - Initial (Trigger) (Write Only) (1=Restart Sound)
    // Bit 6   - Counter/consecutive selection (R/W) (1=Stop output when length in NR21 expires)
    // Bits 5-3 - Unused
    // Bit 2-0 - Higher 3 bits of 11-bit frequency (Write Only)
    trigger: bool,          // Bit 7 (Write-Only)
    length_enable: bool,    // Bit 6 (R/W)
    freq_hi: u8,            // Bits 2-0 (Write-Only)
}

impl Nr24 {
    fn new() -> Self {
        // Default: Unspecified. "Only Bit 6 can be read."
        // Writable fields are 0 initially.
        Self {
            trigger: false,
            length_enable: false,
            freq_hi: 0,
        }
    }

    fn read(&self) -> u8 {
        // Only Bit 6 is readable. Unused/Write-only bits read as 1.
        (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF
    }

    fn write(&mut self, value: u8) {
        self.trigger = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        self.freq_hi = value & 0x07;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr23 {
    // FF18 - NR23 - Channel 2 Frequency lo data (Write Only)
    // Bits 7-0 - Lower 8 bits of an 11-bit frequency.
    freq_lo: u8,
}

impl Nr23 {
    fn new() -> Self {
        // Default: Undefined as it's Write Only. Using 0x00.
        Self { freq_lo: 0x00 }
    }

    fn read(&self) -> u8 {
        // Write-only, typically returns 0xFF on read attempt.
        0xFF
    }

    fn write(&mut self, value: u8) {
        self.freq_lo = value;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr22 {
    // FF17 - NR22 - Channel 2 Volume Envelope (R/W)
    // Bit 7-4 - Initial Volume of envelope (0-F) (0=No Sound)
    // Bit 3   - Envelope Direction (0=Decrease, 1=Increase)
    // Bit 2-0 - Number of envelope sweep (n) (0-7) (0=Stop)
    initial_volume: u8,   // Bits 7-4
    envelope_direction: u8, // Bit 3
    envelope_period: u8,  // Bits 2-0
}

impl Nr22 {
    fn new() -> Self {
        // Default: 0x00 (Initial Volume 0, Direction Decrease, Sweep 0)
        Self {
            initial_volume: 0,
            envelope_direction: 0,
            envelope_period: 0,
        }
    }

    fn read(&self) -> u8 {
        (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period
    }

    fn write(&mut self, value: u8) {
        self.initial_volume = (value >> 4) & 0x0F;
        self.envelope_direction = (value >> 3) & 0x01;
        self.envelope_period = value & 0x07;
    }
}

// Channel 2 Registers (NR2x)
#[derive(Debug, Clone, Copy)]
struct Nr21 {
    // FF16 - NR21 - Channel 2 Sound length/Wave pattern duty (R/W)
    // Bit 7-6 - Wave Pattern Duty (00: 12.5%, 01: 25%, 10: 50%, 11: 75%)
    // Bit 5-0 - Sound Length Data (t1) (0-63)
    wave_pattern_duty: u8, // Bits 7-6
    sound_length_data: u8, // Bits 5-0
}

impl Nr21 {
    fn new() -> Self {
        // Default: 0x3F (Duty 12.5% (00), Length 0x3F (63))
        // 0b00111111 = 0x3F
        // Pan Docs usually implies that unspecified bits during power-up are 0.
        // However, some sources indicate 0x3F for NR21.
        Self {
            wave_pattern_duty: 0b00,
            sound_length_data: 0x3F,
        }
    }

    fn read(&self) -> u8 {
        // Register is R/W. Individual bit description can be confusing.
        // Assume all bits are readable to reflect internal state.
        (self.wave_pattern_duty << 6) | self.sound_length_data
    }

    fn write(&mut self, value: u8) {
        self.wave_pattern_duty = (value >> 6) & 0x03;
        self.sound_length_data = value & 0x3F;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr14 {
    // FF14 - NR14 - Channel 1 Frequency hi data (R/W)
    // Bit 7   - Initial (Trigger) (Write Only) (1=Restart Sound)
    // Bit 6   - Counter/consecutive selection (R/W) (1=Stop output when length in NR11 expires)
    // Bits 5-3 - Unused
    // Bit 2-0 - Higher 3 bits of 11-bit frequency (Write Only)
    trigger: bool,          // Bit 7 (Write-Only)
    length_enable: bool,    // Bit 6 (R/W)
    freq_hi: u8,            // Bits 2-0 (Write-Only)
}

impl Nr14 {
    fn new() -> Self {
        // Default: Unspecified. "Only Bit 6 can be read."
        // Let's assume all writable fields are 0 initially.
        // trigger = false (0)
        // length_enable = false (0) (Sound is continuous)
        // freq_hi = 0
        Self {
            trigger: false,
            length_enable: false,
            freq_hi: 0,
        }
    }

    fn read(&self) -> u8 {
        // Only Bit 6 is readable. Unused bits read as 1.
        // So, mask is 0b10111111 (BF) if bit 6 is 0, or 0b11111111 (FF) if bit 6 is 1.
        // (self.length_enable as u8 << 6) | 0b10111111
        // Or, more simply, (self.length_enable as u8 << 6) | (all other bits that are not Bit 6 are 1s = 0xFF ^ (1 << 6))
        // (self.length_enable as u8 << 6) | (0xFF ^ 0x40) = (self.length_enable as u8 << 6) | 0xBF
        (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF // sets bit 6 based on length_enable, other readable bits (unused) are 1. Write-only bits (7, 2-0) read as 1s.
    }

    fn write(&mut self, value: u8) {
        self.trigger = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        self.freq_hi = value & 0x07;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr13 {
    // FF13 - NR13 - Channel 1 Frequency lo data (Write Only)
    // Bits 7-0 - Lower 8 bits of an 11-bit frequency.
    freq_lo: u8,
}

impl Nr13 {
    fn new() -> Self {
        // Default: Undefined as it's Write Only. Using 0x00.
        // Pan Docs: "Write Only".
        // The value is combined with 3 bits from NR14 to form an 11-bit frequency.
        Self { freq_lo: 0x00 }
    }

    fn read(&self) -> u8 {
        // This register is write-only. Some sources say reads return 0xFF, others 0x00 or last written.
        // For emulation, typically last written value is fine, or a fixed value like 0xFF.
        // Let's return last written value for now. Or 0xFF as per some docs.
        // Given Pan Docs says "Write Only", reading likely has undefined behavior or returns a fixed bus value.
        // For simplicity in our struct, we can store and return the value,
        // but if it were a direct hardware model, it might be 0xFF or open bus.
        // Returning 0xFF is a common placeholder for write-only registers.
        0xFF // Or self.freq_lo if we decide to make it readable for software state.
             // Most emulators treat it as readable (last written value).
             // Let's stick to storing and returning for now.
        // Actually, let's align with Pan Docs more strictly for "Write Only" if possible.
        // However, most emulators make these readable. For now, let's make it return 0xFF.
        // This means the internal state `freq_lo` is only for holding the written value.
        // The problem states "Each struct should have fields corresponding to the bits".
        // So, we need `freq_lo`. The read behavior can be modeled as 0xFF.
    }

    fn write(&mut self, value: u8) {
        self.freq_lo = value;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr12 {
    // FF12 - NR12 - Channel 1 Volume Envelope (R/W)
    // Bit 7-4 - Initial Volume of envelope (0-F) (0=No Sound)
    // Bit 3   - Envelope Direction (0=Decrease, 1=Increase)
    // Bit 2-0 - Number of envelope sweep (n) (0-7) (0=Stop)
    initial_volume: u8,   // Bits 7-4
    envelope_direction: u8, // Bit 3
    envelope_period: u8,  // Bits 2-0
}

impl Nr12 {
    fn new() -> Self {
        // Default: 0x00 (Initial Volume 0, Direction Decrease, Sweep 0)
        Self {
            initial_volume: 0,
            envelope_direction: 0,
            envelope_period: 0,
        }
    }

    fn read(&self) -> u8 {
        (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period
    }

    fn write(&mut self, value: u8) {
        self.initial_volume = (value >> 4) & 0x0F;
        self.envelope_direction = (value >> 3) & 0x01;
        self.envelope_period = value & 0x07;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr11 {
    // FF11 - NR11 - Channel 1 Sound length/Wave pattern duty (R/W)
    // Bit 7-6 - Wave Pattern Duty (00: 12.5%, 01: 25%, 10: 50%, 11: 75%)
    // Bit 5-0 - Sound Length Data (t1) (0-63)
    wave_pattern_duty: u8, // Bits 7-6
    sound_length_data: u8, // Bits 5-0
}

impl Nr11 {
    fn new() -> Self {
        // Default: 0xBF (Duty 50% (10), Length 0x3F (63))
        // 0b10111111 = 0xBF
        Self {
            wave_pattern_duty: 0b10,
            sound_length_data: 0x3F,
        }
    }

    fn read(&self) -> u8 {
        // Bits 7-6 are R/W, 5-0 are Write Only according to some docs, but readable in others.
        // For now, assume readable.
        (self.wave_pattern_duty << 6) | self.sound_length_data
    }

    fn write(&mut self, value: u8) {
        self.wave_pattern_duty = (value >> 6) & 0x03;
        self.sound_length_data = value & 0x3F;
    }
}

pub struct Apu {
    // Channel 1: Square wave with sweep
    channel1: Channel1,

    // Channel 2: Square wave
    channel2: Channel2,

    // Channel 3: Wave output
    channel3: Channel3,
    wave_ram: [u8; 16], // Wave Pattern RAM (FF30-FF3F) - Stays in Apu struct

    // Channel 4: Noise
    channel4: Channel4,

    // Sound Control Registers
    nr50: Nr50,
    nr51: Nr51,
    nr52: Nr52,

    // APU global state
    frame_sequencer_counter: u32,
    frame_sequencer_step: u8,
    hpf_capacitor_left: f32,
    hpf_capacitor_right: f32,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            channel1: Channel1::new(),
            channel2: Channel2::new(),
            channel3: Channel3::new(),
            wave_ram: [
                0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF,
                0xCF, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00
            ],
            channel4: Channel4::new(),
            nr50: Nr50::new(),
            nr51: Nr51::new(),
            nr52: Nr52::new(),
            frame_sequencer_counter: 0,
            frame_sequencer_step: 0,
            hpf_capacitor_left: 0.0,
            hpf_capacitor_right: 0.0,
        }
    }

    fn reset_registers_and_wave_ram(&mut self) {
        self.channel1 = Channel1::new();
        self.channel2 = Channel2::new();
        self.channel3 = Channel3::new();
        self.wave_ram = [
            0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF,
            0xCF, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00
        ];
        self.channel4 = Channel4::new();
        self.nr50 = Nr50::new();
        self.nr51 = Nr51::new();
        // NR52 is not reset here.
        // Frame sequencer and HPF caps also likely reset or affected by power off.
        self.frame_sequencer_counter = 0;
        self.frame_sequencer_step = 0;
        self.hpf_capacitor_left = 0.0;
        self.hpf_capacitor_right = 0.0;
    }

    // This should be pub if called from outside the apu module (e.g. from main loop)
    pub fn tick(&mut self, cpu_t_cycles: u32) {
        // cpu_t_cycles is the number of CPU T-cycles (4.194304 MHz) that have passed.
        // The APU's frame sequencer is clocked by the system clock / 8192.
        // Each channel's internal frequency timer is also clocked by the system clock (or a division).

        self.frame_sequencer_counter += cpu_t_cycles;
        while self.frame_sequencer_counter >= CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK {
            self.frame_sequencer_counter -= CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK;
            self.clock_frame_sequencer();
        }

        // For channel ticks, their internal timers are based on formulas that relate to CPU clock.
        // If a channel's frequency_timer is, say, N, it means N APU clock cycles (often 1MHz or 2MHz).
        // If we pass T-cycles (4MHz), we might need to adjust how channel tick() methods decrement their timers,
        // or pass a scaled number of ticks to them.
        // The current channel tick() methods decrement their frequency_timer by 1. This implies they are
        // being called at the rate their frequency_timer expects.
        // If frequency_timer is set up in terms of CPU T-cycles, then we can loop cpu_t_cycles times.
        // Let's assume for now that channel tick() methods are robust enough or will be adjusted.
        // A simple approach: call tick N times.
        // However, the channel timers are set with values like (2048-X)*4. This is already in terms of
        // a certain clock (likely APU's ~1MHz or ~2MHz clock).
        // If Apu::tick is called for every T-cycle (smallest CPU step), then channel tick should be called.
        // If Apu::tick is called with a batch of T-cycles, then channel ticks should be called that many times.

        for _ in 0..cpu_t_cycles {
            // This assumes channel timers are set relative to the fastest clock (CPU T-cycles)
            // or that their internal logic handles the rate.
            // The current channel frequency timers are defined like (2048-P)*4 or *2.
            // This value usually corresponds to a number of APU clock cycles (e.g., 1MHz or 2MHz).
            // If Apu::tick() is called with `t_cycles` (at 4.19MHz rate), then
            // each channel's timer should also effectively be clocked by this.
            // The most straightforward way is to have each channel's tick() be called for each of these t_cycles.
            self.channel1.tick();
            self.channel2.tick();
            self.channel3.tick(&self.wave_ram);
            self.channel4.tick();
        }
    }

    fn clock_frame_sequencer(&mut self) {
        if !self.nr52.is_apu_enabled() {
            return;
        }

        match self.frame_sequencer_step {
            0 => { // 256 Hz
                self.channel1.clock_length();
                self.channel2.clock_length();
                self.channel3.clock_length();
                self.channel4.clock_length();
            }
            1 => { /* --- Nothing --- */ }
            2 => { // 128 Hz for sweep, 256 Hz for length
                self.channel1.clock_length();
                self.channel2.clock_length();
                self.channel3.clock_length();
                self.channel4.clock_length();
                self.channel1.clock_sweep();
            }
            3 => { /* --- Nothing --- */ }
            4 => { // 256 Hz
                self.channel1.clock_length();
                self.channel2.clock_length();
                self.channel3.clock_length();
                self.channel4.clock_length();
            }
            5 => { /* --- Nothing --- */ }
            6 => { // 128 Hz for sweep, 256 Hz for length
                self.channel1.clock_length();
                self.channel2.clock_length();
                self.channel3.clock_length();
                self.channel4.clock_length();
                self.channel1.clock_sweep();
            }
            7 => { // 64 Hz
                self.channel1.clock_envelope();
                self.channel2.clock_envelope();
                // Channel 3 does not have a volume envelope
                self.channel4.clock_envelope();
            }
            _ => { /* Unreachable */ }
        }
        self.frame_sequencer_step = (self.frame_sequencer_step + 1) % 8;
    }

    // This should be pub if called from outside the apu module
    pub fn get_mixed_audio_samples(&mut self) -> (f32, f32) {
        if !self.nr52.is_apu_enabled() {
            return (0.0, 0.0);
        }

        let ch1_dig = self.channel1.get_output_volume();
        let ch2_dig = self.channel2.get_output_volume();
        let ch3_dig = self.channel3.get_output_sample();
        let ch4_dig = self.channel4.get_output_volume();

        // DAC: digital 0 maps to analog 1.0, digital 15 maps to analog -1.0
        // Formula: 1.0 - (digital_value / 7.5)
        // If DAC is off for a channel, its output is 0.0 for mixing.
        let dac1 = if self.channel1.nr12.dac_power() { 1.0 - (ch1_dig as f32 / 7.5) } else { 0.0 };
        let dac2 = if self.channel2.nr22.dac_power() { 1.0 - (ch2_dig as f32 / 7.5) } else { 0.0 };
        let dac3 = if self.channel3.nr30.dac_on() { 1.0 - (ch3_dig as f32 / 7.5) } else { 0.0 };
        let dac4 = if self.channel4.nr42.dac_power() { 1.0 - (ch4_dig as f32 / 7.5) } else { 0.0 };

        let mut left_sample_sum = 0.0;
        let mut right_sample_sum = 0.0;

        if self.nr51.is_ch1_to_so1() { left_sample_sum += dac1; }
        if self.nr51.is_ch1_to_so2() { right_sample_sum += dac1; }
        if self.nr51.is_ch2_to_so1() { left_sample_sum += dac2; }
        if self.nr51.is_ch2_to_so2() { right_sample_sum += dac2; }
        if self.nr51.is_ch3_to_so1() { left_sample_sum += dac3; }
        if self.nr51.is_ch3_to_so2() { right_sample_sum += dac3; }
        if self.nr51.is_ch4_to_so1() { left_sample_sum += dac4; }
        if self.nr51.is_ch4_to_so2() { right_sample_sum += dac4; }

        // Normalize by number of channels routed to avoid > 4.0 or < -4.0 sum
        // Max output per terminal is 4 channels * 1.0 = 4.0 (or -4.0)
        // This needs to be scaled to the final output range (e.g. -1.0 to 1.0)
        // For now, let's scale by a factor of 0.25 to bring it into -1.0 to 1.0 range roughly
        // left_sample_sum *= 0.25;
        // right_sample_sum *= 0.25;
        // This scaling might be better done after master volume.

        let left_vol_factor = (self.nr50.so1_output_level() as f32) / 7.0; // 0-7 maps to 0/7 to 7/7
        let right_vol_factor = (self.nr50.so2_output_level() as f32) / 7.0; // 0-7 maps to 0/7 to 7/7

        // Note: Pan Docs says SOx volume is (0-7)+1 / 8. But then if volume is 0, output is 1/8th?
        // "Volume is adjusted by multiplying by (SOx volume+1)/8." - this sounds more plausible.
        // Let's use (vol+1)/8. Max is 1.0, min is 1/8.
        // let left_master_vol = (self.nr50.so1_output_level() + 1) as f32 / 8.0;
        // let right_master_vol = (self.nr50.so2_output_level() + 1) as f32 / 8.0;
        // However, many emulators use volume/7 for scaling. Let's stick to the prompt's (vol+1)/8

        let left_master_vol = (self.nr50.so1_output_level().wrapping_add(1)) as f32 / 8.0;
        let right_master_vol = (self.nr50.so2_output_level().wrapping_add(1)) as f32 / 8.0;


        let mut final_left = left_sample_sum * left_master_vol;
        let mut final_right = right_sample_sum * right_master_vol;

        // Scale to prevent clipping if sum of 4 channels is max. Max sum is 4.0.
        // To bring to typical -1.0 to 1.0 audio range, divide by 4.
        final_left /= 4.0;
        final_right /= 4.0;

        // High-Pass Filter (HPF) - DMG model
        // This is a very simplified model. Real HPF is more complex.
        // Pan Docs: output = sample - capacitor; capacitor = sample - output * factor;
        // Factor for DMG: 0.999958 (approx 1 - (1 / (2^14.3))) for 44.1kHz. Or based on RC time constant.
        // For now, let's use a simple one from common emulators or the one in prompt.
        // The prompt's version seems to be a common one.
        let charge_factor_dmg = 0.999958_f32;
        // For CGB, it's different, often disabled or different factor (e.g., 0.998943)
        // We'll assume DMG for now.

        let hpf_out_left = final_left - self.hpf_capacitor_left;
        self.hpf_capacitor_left = final_left - hpf_out_left * charge_factor_dmg;

        let hpf_out_right = final_right - self.hpf_capacitor_right;
        self.hpf_capacitor_right = final_right - hpf_out_right * charge_factor_dmg;

        (hpf_out_left, hpf_out_right)
    }


    pub fn read_byte(&self, addr: u16) -> u8 {
        let result = if !self.nr52.is_apu_enabled() && addr != NR52_ADDR {
            // TODO: Some registers might be readable even when APU is off (e.g. NR52 itself, maybe wave RAM under certain conditions)
            // Pan Docs: "When the sound controller is OFF (FF26 Bit 7 = 0) all APU registers will read $FF" - this is not entirely true.
            // Most common behavior is FF for many, but some specific registers might differ or be partially readable.
            // For now, sticking to the simple rule from the prompt.
            return 0xFF;
        }

        match addr {
            NR10_ADDR => self.channel1.nr10.read(),
            NR11_ADDR => self.channel1.nr11.read(),
            NR12_ADDR => self.channel1.nr12.read(),
            NR13_ADDR => self.channel1.nr13.read(),
            NR14_ADDR => self.channel1.nr14.read(),

            NR21_ADDR => self.channel2.nr21.read(),
            NR22_ADDR => self.channel2.nr22.read(),
            NR23_ADDR => self.channel2.nr23.read(),
            NR24_ADDR => self.channel2.nr24.read(),

            NR30_ADDR => self.channel3.nr30.read(),
            NR31_ADDR => self.channel3.nr31.read(),
            NR32_ADDR => self.channel3.nr32.read(),
            NR33_ADDR => self.channel3.nr33.read(),
            NR34_ADDR => self.channel3.nr34.read(),

            WAVE_PATTERN_RAM_START_ADDR..=WAVE_PATTERN_RAM_END_ADDR => {
                if self.nr52.is_apu_enabled() && self.channel3.nr30.dac_on() { // Condition for CH3 DAC for Wave RAM access
                    self.wave_ram[(addr - WAVE_PATTERN_RAM_START_ADDR) as usize]
                } else {
                    0xFF
                }
            }

            NR41_ADDR => self.channel4.nr41.read(),
            NR42_ADDR => self.channel4.nr42.read(),
            NR43_ADDR => self.channel4.nr43.read(),
            NR44_ADDR => self.channel4.nr44.read(),

            NR50_ADDR => self.nr50.read(), // Uses pub(super) Nr50::read()
            NR51_ADDR => self.nr51.read(), // Uses pub(super) Nr51::read()
            NR52_ADDR => self.nr52.read(),

            _ => {
                // println!("APU read attempt at unmapped address: {:#04X}", addr);
                0xFF // Unmapped APU addresses often return 0xFF
            }
        };
        debug!("APU Read: Addr={:#06X}, Value={:#04X}", addr, result);
        result
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        debug!("APU Write: Addr={:#06X}, Value={:#04X}", addr, value);
        if !self.nr52.is_apu_enabled() && addr != NR52_ADDR {
            // Most writes ignored if APU is off, except to NR52 itself.
            // Special case for NR52 to allow turning APU on.
            if addr == NR52_ADDR {
                let was_apu_enabled = self.nr52.is_apu_enabled();
                self.nr52.write(value); // Allow write to NR52
                let is_apu_enabled_after_write = self.nr52.is_apu_enabled();
                if was_apu_enabled && !is_apu_enabled_after_write {
                    self.reset_registers_and_wave_ram();
                }
            }
            return;
        }

        match addr {
            NR10_ADDR => self.channel1.nr10.write(value),
            NR11_ADDR => self.channel1.nr11.write(value),
            NR12_ADDR => self.channel1.nr12.write(value),
            NR13_ADDR => self.channel1.nr13.write(value),
            NR14_ADDR => self.channel1.nr14.write(value),

            NR21_ADDR => self.channel2.nr21.write(value),
            NR22_ADDR => self.channel2.nr22.write(value),
            NR23_ADDR => self.channel2.nr23.write(value),
            NR24_ADDR => self.channel2.nr24.write(value),

            NR30_ADDR => self.channel3.nr30.write(value),
            NR31_ADDR => self.channel3.nr31.write(value),
            NR32_ADDR => self.channel3.nr32.write(value),
            NR33_ADDR => self.channel3.nr33.write(value),
            NR34_ADDR => self.channel3.nr34.write(value),

            WAVE_PATTERN_RAM_START_ADDR..=WAVE_PATTERN_RAM_END_ADDR => {
                if self.nr52.is_apu_enabled() && self.channel3.nr30.dac_on() { // Condition for CH3 DAC for Wave RAM access
                    self.wave_ram[(addr - WAVE_PATTERN_RAM_START_ADDR) as usize] = value;
                }
            }

            NR41_ADDR => self.channel4.nr41.write(value),
            NR42_ADDR => self.channel4.nr42.write(value),
            NR43_ADDR => self.channel4.nr43.write(value),
            NR44_ADDR => self.channel4.nr44.write(value),

            NR50_ADDR => self.nr50.write(value), // Uses pub(super) Nr50::write()
            NR51_ADDR => self.nr51.write(value), // Uses pub(super) Nr51::write()
            NR52_ADDR => {
                let was_apu_enabled = self.nr52.is_apu_enabled();
                self.nr52.write(value);
                let is_apu_enabled_after_write = self.nr52.is_apu_enabled();

                if was_apu_enabled && !is_apu_enabled_after_write {
                    // APU was just turned off, reset all registers
                    self.reset_registers_and_wave_ram();
                }
            }
            _ => {
                // println!("APU write attempt at unmapped address: {:#04X} with value: {:#02X}", addr, value);
                // Writes to unmapped APU addresses are typically ignored.
            }
        }
    }
}
