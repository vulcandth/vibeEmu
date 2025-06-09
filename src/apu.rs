// src/apu.rs

pub mod channel1;
pub mod channel2;
pub mod channel3;
pub mod channel4;
use self::channel1::Channel1;
use self::channel2::Channel2;
use self::channel3::Channel3;
use self::channel4::Channel4;
use log::debug;

pub const CPU_CLOCK_HZ: u32 = 4194304;
const FRAME_SEQUENCER_FREQUENCY_HZ: u32 = 512;
const CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK: u32 = CPU_CLOCK_HZ / FRAME_SEQUENCER_FREQUENCY_HZ;

const NR10_ADDR: u16 = 0xFF10;
const NR11_ADDR: u16 = 0xFF11;
const NR12_ADDR: u16 = 0xFF12;
const NR13_ADDR: u16 = 0xFF13;
const NR14_ADDR: u16 = 0xFF14;

const NR21_ADDR: u16 = 0xFF16;
const NR22_ADDR: u16 = 0xFF17;
const NR23_ADDR: u16 = 0xFF18;
const NR24_ADDR: u16 = 0xFF19;

const NR30_ADDR: u16 = 0xFF1A;
const NR31_ADDR: u16 = 0xFF1B;
const NR32_ADDR: u16 = 0xFF1C;
const NR33_ADDR: u16 = 0xFF1D;
const NR34_ADDR: u16 = 0xFF1E;

const NR41_ADDR: u16 = 0xFF20;
const NR42_ADDR: u16 = 0xFF21;
const NR43_ADDR: u16 = 0xFF22;
const NR44_ADDR: u16 = 0xFF23;

const NR50_ADDR: u16 = 0xFF24;
const NR51_ADDR: u16 = 0xFF25;
const NR52_ADDR: u16 = 0xFF26;

const WAVE_PATTERN_RAM_START_ADDR: u16 = 0xFF30;
const WAVE_PATTERN_RAM_END_ADDR: u16 = 0xFF3F;

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr10 {
    sweep_time: u8,
    sweep_direction: u8,
    sweep_shift: u8,
}

impl Nr10 {
    pub(super) fn new() -> Self { Self { sweep_time: 0, sweep_direction: 0, sweep_shift: 0 } }
    pub(super) fn read(&self) -> u8 { 0x80 | (self.sweep_time << 4) | (self.sweep_direction << 3) | self.sweep_shift }
    pub(super) fn write(&mut self, value: u8) {
        self.sweep_time = (value >> 4) & 0x07;
        self.sweep_direction = (value >> 3) & 0x01;
        self.sweep_shift = value & 0x07;
    }
    pub(super) fn sweep_period(&self) -> u8 { self.sweep_time }
    pub(super) fn sweep_shift_val(&self) -> u8 { self.sweep_shift }
    pub(super) fn sweep_direction_is_increase(&self) -> bool { self.sweep_direction == 0 }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr11 {
    wave_pattern_duty: u8,
    sound_length_data: u8,
}

impl Nr11 {
    pub(super) fn new() -> Self { Self { wave_pattern_duty: 0b00, sound_length_data: 0x00 } }
    pub(super) fn read(&self) -> u8 { (self.wave_pattern_duty << 6) | 0x3F }
    pub(super) fn write(&mut self, value: u8) {
        self.wave_pattern_duty = (value >> 6) & 0x03;
        self.sound_length_data = value & 0x3F;
    }
    pub(super) fn initial_length_timer_val(&self) -> u8 { self.sound_length_data }
    pub(super) fn wave_pattern_duty_val(&self) -> u8 { self.wave_pattern_duty }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr12 {
    initial_volume: u8,
    envelope_direction: u8,
    envelope_period: u8,
}

impl Nr12 {
    pub(super) fn new() -> Self { Self { initial_volume: 0, envelope_direction: 0, envelope_period: 0 } }
    pub(super) fn read(&self) -> u8 { (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period }
    pub(super) fn write(&mut self, value: u8) {
        self.initial_volume = (value >> 4) & 0x0F;
        self.envelope_direction = (value >> 3) & 0x01;
        self.envelope_period = value & 0x07;
    }
    pub(super) fn initial_volume_val(&self) -> u8 { self.initial_volume }
    pub(super) fn dac_power(&self) -> bool { (self.read() & 0xF8) != 0 }
    pub(super) fn envelope_period_val(&self) -> u8 { self.envelope_period }
    pub(super) fn envelope_direction_is_increase(&self) -> bool { self.envelope_direction == 1 }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr13 { freq_lo: u8 }
impl Nr13 {
    pub(super) fn new() -> Self { Self { freq_lo: 0x00 } }
    pub(super) fn read(&self) -> u8 { 0xFF }
    pub(super) fn write(&mut self, value: u8) { self.freq_lo = value; }
    pub(super) fn freq_lo_val(&self) -> u8 { self.freq_lo }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr14 {
    trigger_bit_in_write: bool,
    length_enable: bool,
    freq_hi: u8,
}
impl Nr14 {
    pub(super) fn new() -> Self { Self { trigger_bit_in_write: false, length_enable: false, freq_hi: 0 } }
    pub(super) fn read(&self) -> u8 { (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF }
    pub(super) fn write(&mut self, value: u8) {
        self.trigger_bit_in_write = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        self.freq_hi = value & 0x07;
    }
    pub(super) fn is_length_enabled(&self) -> bool { self.length_enable }
    pub(super) fn frequency_msb_val(&self) -> u8 { self.freq_hi }
    pub(super) fn write_frequency_msb(&mut self, val: u8) { self.freq_hi = val & 0x07; }
    pub(super) fn consume_trigger_flag(&mut self) -> bool {
        let triggered = self.trigger_bit_in_write;
        self.trigger_bit_in_write = false;
        triggered
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr21 { wave_pattern_duty: u8, sound_length_data: u8 }
impl Nr21 {
    pub(super) fn new() -> Self { Self { wave_pattern_duty: 0b00, sound_length_data: 0x00 } }
    pub(super) fn read(&self) -> u8 { (self.wave_pattern_duty << 6) | 0x3F }
    pub(super) fn write(&mut self, value: u8) {
        self.wave_pattern_duty = (value >> 6) & 0x03;
        self.sound_length_data = value & 0x3F;
    }
    pub(super) fn initial_length_timer_val(&self) -> u8 { self.sound_length_data }
    pub(super) fn wave_pattern_duty_val(&self) -> u8 { self.wave_pattern_duty }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr22 { initial_volume: u8, envelope_direction: u8, envelope_period: u8 }
impl Nr22 {
    pub(super) fn new() -> Self { Self { initial_volume: 0, envelope_direction: 0, envelope_period: 0 } }
    pub(super) fn read(&self) -> u8 { (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period }
    pub(super) fn write(&mut self, value: u8) {
        self.initial_volume = (value >> 4) & 0x0F;
        self.envelope_direction = (value >> 3) & 0x01;
        self.envelope_period = value & 0x07;
    }
    pub(super) fn initial_volume_val(&self) -> u8 { self.initial_volume }
    pub(super) fn dac_power(&self) -> bool { (self.read() & 0xF8) != 0 }
    pub(super) fn envelope_period_val(&self) -> u8 { self.envelope_period }
    pub(super) fn envelope_direction_is_increase(&self) -> bool { self.envelope_direction == 1 }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr23 { freq_lo: u8 }
impl Nr23 {
    pub(super) fn new() -> Self { Self { freq_lo: 0x00 } }
    pub(super) fn read(&self) -> u8 { 0xFF }
    pub(super) fn write(&mut self, value: u8) { self.freq_lo = value; }
    pub(super) fn freq_lo_val(&self) -> u8 { self.freq_lo }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr24 { trigger_bit_in_write: bool, length_enable: bool, freq_hi: u8 }
impl Nr24 {
    pub(super) fn new() -> Self { Self { trigger_bit_in_write: false, length_enable: false, freq_hi: 0 } }
    pub(super) fn read(&self) -> u8 { (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF }
    pub(super) fn write(&mut self, value: u8) {
        self.trigger_bit_in_write = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        self.freq_hi = value & 0x07;
    }
    pub(super) fn is_length_enabled(&self) -> bool { self.length_enable }
    pub(super) fn frequency_msb_val(&self) -> u8 { self.freq_hi }
    pub(super) fn consume_trigger_flag(&mut self) -> bool {
        let triggered = self.trigger_bit_in_write;
        self.trigger_bit_in_write = false;
        triggered
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr30 { sound_on: bool }
impl Nr30 {
    pub(super) fn new() -> Self { Self { sound_on: false } }
    pub(super) fn read(&self) -> u8 { (if self.sound_on { 0x80 } else { 0x00 }) | 0x7F }
    pub(super) fn write(&mut self, value: u8) { self.sound_on = (value >> 7) & 0x01 != 0; }
    pub(super) fn dac_on(&self) -> bool { self.sound_on }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr31 { sound_length: u8 }
impl Nr31 {
    pub(super) fn new() -> Self { Self { sound_length: 0x00 } }
    pub(super) fn read(&self) -> u8 { 0xFF }
    pub(super) fn write(&mut self, value: u8) { self.sound_length = value; }
    pub(super) fn sound_length_val(&self) -> u8 { self.sound_length }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr32 { output_level: u8 }
impl Nr32 {
    pub(super) fn new() -> Self { Self { output_level: 0b00 } }
    pub(super) fn read(&self) -> u8 { (self.output_level << 5) | 0x9F }
    pub(super) fn write(&mut self, value: u8) { self.output_level = (value >> 5) & 0x03; }
    pub(super) fn get_volume_shift(&self) -> u8 {
        match self.output_level { 0b01 => 0, 0b10 => 1, 0b11 => 2, _ => 4 }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr33 { freq_lo: u8 }
impl Nr33 {
    pub(super) fn new() -> Self { Self { freq_lo: 0x00 } }
    pub(super) fn read(&self) -> u8 { 0xFF }
    pub(super) fn write(&mut self, value: u8) { self.freq_lo = value; }
    pub(super) fn freq_lo_val(&self) -> u8 { self.freq_lo }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr34 { trigger_bit_in_write: bool, length_enable: bool, freq_hi: u8 }
impl Nr34 {
    pub(super) fn new() -> Self { Self { trigger_bit_in_write: false, length_enable: false, freq_hi: 0 } }
    pub(super) fn read(&self) -> u8 { (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF }
    pub(super) fn write(&mut self, value: u8) {
        self.trigger_bit_in_write = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
        self.freq_hi = value & 0x07;
    }
    pub(super) fn is_length_enabled(&self) -> bool { self.length_enable }
    pub(super) fn frequency_msb_val(&self) -> u8 { self.freq_hi }
    pub(super) fn consume_trigger_flag(&mut self) -> bool {
        let triggered = self.trigger_bit_in_write;
        self.trigger_bit_in_write = false;
        triggered
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr41 { sound_length_data: u8 }
impl Nr41 {
    pub(super) fn new() -> Self { Self { sound_length_data: 0x00 } }
    pub(super) fn read(&self) -> u8 { 0xFF }
    pub(super) fn write(&mut self, value: u8) { self.sound_length_data = value & 0x3F; }
    pub(super) fn initial_length_timer_val(&self) -> u8 { self.sound_length_data }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr42 { initial_volume: u8, envelope_direction: u8, envelope_period: u8 }
impl Nr42 {
    pub(super) fn new() -> Self { Self { initial_volume: 0, envelope_direction: 0, envelope_period: 0 } }
    pub(super) fn read(&self) -> u8 { (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period }
    pub(super) fn write(&mut self, value: u8) {
        self.initial_volume = (value >> 4) & 0x0F;
        self.envelope_direction = (value >> 3) & 0x01;
        self.envelope_period = value & 0x07;
    }
    pub(super) fn initial_volume_val(&self) -> u8 { self.initial_volume }
    pub(super) fn dac_power(&self) -> bool { (self.read() & 0xF8) != 0 }
    pub(super) fn envelope_period_val(&self) -> u8 { self.envelope_period }
    pub(super) fn envelope_direction_is_increase(&self) -> bool { self.envelope_direction == 1 }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr43 { shift_clock_freq: u8, counter_width: u8, dividing_ratio: u8 }
impl Nr43 {
    pub(super) fn new() -> Self { Self { shift_clock_freq: 0, counter_width: 0, dividing_ratio: 0 } }
    pub(super) fn read(&self) -> u8 { (self.shift_clock_freq << 4) | (self.counter_width << 3) | self.dividing_ratio }
    pub(super) fn write(&mut self, value: u8) {
        self.shift_clock_freq = (value >> 4) & 0x0F;
        self.counter_width = (value >> 3) & 0x01;
        self.dividing_ratio = value & 0x07;
    }
    pub(super) fn clock_shift(&self) -> u8 { self.shift_clock_freq }
    pub(super) fn lfsr_width_is_7bit(&self) -> bool { self.counter_width == 1 }
    pub(super) fn clock_divider_val(&self) -> u8 { self.dividing_ratio }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr44 { trigger_bit_in_write: bool, length_enable: bool }
impl Nr44 {
    pub(super) fn new() -> Self { Self { trigger_bit_in_write: false, length_enable: false } }
    pub(super) fn read(&self) -> u8 { (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF }
    pub(super) fn write(&mut self, value: u8) {
        self.trigger_bit_in_write = (value >> 7) & 0x01 != 0;
        self.length_enable = (value >> 6) & 0x01 != 0;
    }
    pub(super) fn is_length_enabled(&self) -> bool { self.length_enable }
    pub(super) fn consume_trigger_flag(&mut self) -> bool {
        let triggered = self.trigger_bit_in_write;
        self.trigger_bit_in_write = false;
        triggered
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Nr52 {
    all_sound_on: bool,
    ch4_status: bool,
    ch3_status: bool,
    ch2_status: bool,
    ch1_status: bool,
}
impl Nr52 {
    fn new() -> Self { Self::default() }
    fn is_apu_enabled(&self) -> bool { self.all_sound_on }
    fn read(&self) -> u8 {
        (if self.all_sound_on { 0x80 } else { 0x00 }) | // Power bit
        0x70 | // Unused bits read as 1
        (if self.ch4_status { 0x08 } else { 0x00 }) | // Channel 4 status
        (if self.ch3_status { 0x04 } else { 0x00 }) | // Channel 3 status
        (if self.ch2_status { 0x02 } else { 0x00 }) | // Channel 2 status
        (if self.ch1_status { 0x01 } else { 0x00 })   // Channel 1 status
    }
    fn write(&mut self, value: u8) { self.all_sound_on = (value >> 7) & 0x01 != 0; }
    fn update_status_bits(&mut self, ch1_on: bool, ch2_on: bool, ch3_on: bool, ch4_on: bool) {
        self.ch1_status = ch1_on; self.ch2_status = ch2_on;
        self.ch3_status = ch3_on; self.ch4_status = ch4_on;
    }
}

#[derive(Debug, Clone, Copy)]
struct Nr51 {
    ch4_to_so2: bool, ch3_to_so2: bool, ch2_to_so2: bool, ch1_to_so2: bool,
    ch4_to_so1: bool, ch3_to_so1: bool, ch2_to_so1: bool, ch1_to_so1: bool,
}
impl Nr51 {
    fn new() -> Self {
        Self {
            ch4_to_so2: true, ch3_to_so2: true, ch2_to_so2: true, ch1_to_so2: true,
            ch4_to_so1: true, ch3_to_so1: true, ch2_to_so1: true, ch1_to_so1: true,
        }
    }
    fn read(&self) -> u8 {
        (if self.ch4_to_so2 { 0x80 } else { 0x00 }) | (if self.ch3_to_so2 { 0x40 } else { 0x00 }) |
        (if self.ch2_to_so2 { 0x20 } else { 0x00 }) | (if self.ch1_to_so2 { 0x10 } else { 0x00 }) |
        (if self.ch4_to_so1 { 0x08 } else { 0x00 }) | (if self.ch3_to_so1 { 0x04 } else { 0x00 }) |
        (if self.ch2_to_so1 { 0x02 } else { 0x00 }) | (if self.ch1_to_so1 { 0x01 } else { 0x00 })
    }
    fn write(&mut self, value: u8) {
        self.ch1_to_so1 = value & 0x01 != 0; self.ch2_to_so1 = (value >> 1) & 0x01 != 0;
        self.ch3_to_so1 = (value >> 2) & 0x01 != 0; self.ch4_to_so1 = (value >> 3) & 0x01 != 0;
        self.ch1_to_so2 = (value >> 4) & 0x01 != 0; self.ch2_to_so2 = (value >> 5) & 0x01 != 0;
        self.ch3_to_so2 = (value >> 6) & 0x01 != 0; self.ch4_to_so2 = (value >> 7) & 0x01 != 0;
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Nr50 { vin_so2_enable: bool, so2_volume: u8, vin_so1_enable: bool, so1_volume: u8 }
impl Nr50 {
    pub(super) fn new() -> Self { Self { vin_so2_enable: false, so2_volume: 7, vin_so1_enable: false, so1_volume: 7 } }
    pub(super) fn read(&self) -> u8 {
        (if self.vin_so2_enable { 0x80 } else { 0x00 }) | (self.so2_volume << 4) |
        (if self.vin_so1_enable { 0x08 } else { 0x00 }) | self.so1_volume
    }
    pub(super) fn write(&mut self, value: u8) {
        self.so1_volume = value & 0x07; self.vin_so1_enable = (value >> 3) & 0x01 != 0;
        self.so2_volume = (value >> 4) & 0x07; self.vin_so2_enable = (value >> 7) & 0x01 != 0;
    }
    pub(super) fn so1_output_level(&self) -> u8 { self.so1_volume }
    pub(super) fn so2_output_level(&self) -> u8 { self.so2_volume }
}

pub struct Apu {
    channel1: Channel1,
    channel2: Channel2,
    channel3: Channel3,
    channel4: Channel4,
    wave_ram: [u8; 16],
    nr50: Nr50,
    nr51: Nr51,
    nr52: Nr52,
    frame_sequencer_counter: u32,
    frame_sequencer_step: u8,
    hpf_capacitor_left: f32,
    hpf_capacitor_right: f32,
    skip_next_frame_sequencer_tick: bool, // For NR52 power-on DIV interaction
}

impl Apu {
    pub fn new() -> Self {
        let mut apu = Self {
            channel1: Channel1::new(),
            channel2: Channel2::new(),
            channel3: Channel3::new(),
            channel4: Channel4::new(),
            wave_ram: [0xAC,0xDD,0xDA,0x48,0x36,0x02,0xCF,0x16,0x2C,0x04,0xE5,0x2C,0xAC,0xDD,0xDA,0x48],
            nr50: Nr50::new(),
            nr51: Nr51::new(),
            nr52: Nr52::new(),
            frame_sequencer_counter: CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK,
            frame_sequencer_step: 0,
            hpf_capacitor_left: 0.0,
            hpf_capacitor_right: 0.0,
            skip_next_frame_sequencer_tick: false,
        };
        apu.reset_power_on_channel_flags();
        apu
    }

    fn reset_power_on_channel_flags(&mut self) {
        self.channel1.power_on_reset();
        self.channel2.power_on_reset();
        // self.channel3.power_on_reset();
        // self.channel4.power_on_reset();
    }

    fn full_apu_reset_on_power_off(&mut self) {
        self.channel1 = Channel1::new();
        self.channel2 = Channel2::new();
        self.channel3 = Channel3::new();
        self.channel4 = Channel4::new();

        // Reset NR50 to 0x00
        self.nr50.so1_volume = 0;
        self.nr50.vin_so1_enable = false;
        self.nr50.so2_volume = 0;
        self.nr50.vin_so2_enable = false;

        // Reset NR51 to 0x00
        self.nr51.ch1_to_so1 = false;
        self.nr51.ch2_to_so1 = false;
        self.nr51.ch3_to_so1 = false;
        self.nr51.ch4_to_so1 = false;
        self.nr51.ch1_to_so2 = false;
        self.nr51.ch2_to_so2 = false;
        self.nr51.ch3_to_so2 = false;
        self.nr51.ch4_to_so2 = false;

        self.frame_sequencer_step = 0;
        self.frame_sequencer_counter = CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK;
        self.hpf_capacitor_left = 0.0;
        self.hpf_capacitor_right = 0.0;
        self.nr52.update_status_bits(false,false,false,false);
    }

    pub fn tick(&mut self, cpu_t_cycles: u32) {
        self.frame_sequencer_counter += cpu_t_cycles;
        while self.frame_sequencer_counter >= CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK {
            self.frame_sequencer_counter -= CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK;

            if self.skip_next_frame_sequencer_tick {
                self.skip_next_frame_sequencer_tick = false;
                // The frame_sequencer_step still advances, but clocks are skipped for this one tick
                self.frame_sequencer_step = (self.frame_sequencer_step + 1) % 8;
                 self.nr52.update_status_bits( // Keep status bits updated
                    self.channel1.enabled, self.channel2.enabled,
                    self.channel3.enabled, self.channel4.enabled
                );
            } else {
                self.clock_frame_sequencer();
            }
        }

        if self.nr52.is_apu_enabled() {
            for _ in 0..cpu_t_cycles {
                self.channel1.tick(); self.channel2.tick();
                self.channel3.tick(&self.wave_ram); self.channel4.tick();
            }
        }
    }

    fn clock_frame_sequencer(&mut self) {
        // Note: self.frame_sequencer_step is advanced *after* this function in the original code.
        // To match SameBoy's (div_divider & X) == Y, where div_divider is current step:
        // old step 0 -> new step 1 (length)
        // old step 1 -> new step 2
        // old step 2 -> new step 3 (length, sweep)
        // old step 3 -> new step 4
        // old step 4 -> new step 5 (length)
        // old step 5 -> new step 6
        // old step 6 -> new step 7 (length, sweep, envelope)
        // old step 7 -> new step 0
        // So, we check self.frame_sequencer_step for what events occur *at the beginning* of this step.

        let apu_on = self.nr52.is_apu_enabled();
        if apu_on {
            // Length Clock (Steps 1, 3, 5, 7 of effective new step; or 0,2,4,6 of old step if checking before increment)
            // SameBoy: (div_divider & 1) -> steps 1,3,5,7. Our current step is 'about to become this value'.
            // If self.frame_sequencer_step is what's *about to be processed*:
            if self.frame_sequencer_step % 2 != 0 { // Steps 1, 3, 5, 7
                self.channel1.clock_length();
                self.channel2.clock_length();
                self.channel3.clock_length();
                self.channel4.clock_length();
            }

            // Sweep Clock (Ch1 only) (Steps 3, 7)
            if self.frame_sequencer_step == 3 || self.frame_sequencer_step == 7 {
                self.channel1.clock_sweep();
            }

            // Envelope Clock (Ch1, Ch2, Ch4) (Step 7)
            if self.frame_sequencer_step == 7 {
                self.channel1.clock_envelope();
                self.channel2.clock_envelope();
                self.channel4.clock_envelope();
            }
        }

        self.frame_sequencer_step = (self.frame_sequencer_step + 1) % 8;
        self.nr52.update_status_bits(
            self.channel1.enabled, self.channel2.enabled,
            self.channel3.enabled, self.channel4.enabled
        );
    }

    pub fn get_mixed_audio_samples(&mut self) -> (f32, f32) {
        if !self.nr52.is_apu_enabled() { return (0.0, 0.0); }

        let ch1_s = self.channel1.get_output_volume();
        let ch2_s = self.channel2.get_output_volume();
        let ch3_s = self.channel3.get_output_sample();
        let ch4_s = self.channel4.get_output_volume();

        let dac_conv = |val: u8, dac_is_on: bool| if dac_is_on { 1.0 - (val as f32 / 7.5) } else { 0.0 };
        let dac1 = dac_conv(ch1_s, self.channel1.nr12.dac_power());
        let dac2 = dac_conv(ch2_s, self.channel2.nr22.dac_power());
        let dac3 = dac_conv(ch3_s, self.channel3.nr30.dac_on());
        let dac4 = dac_conv(ch4_s, self.channel4.nr42.dac_power());

        let mut so1_mix = 0.0; let mut so2_mix = 0.0;
        if self.nr51.ch1_to_so1 { so1_mix += dac1; } if self.nr51.ch1_to_so2 { so2_mix += dac1; }
        if self.nr51.ch2_to_so1 { so1_mix += dac2; } if self.nr51.ch2_to_so2 { so2_mix += dac2; }
        if self.nr51.ch3_to_so1 { so1_mix += dac3; } if self.nr51.ch3_to_so2 { so2_mix += dac3; }
        if self.nr51.ch4_to_so1 { so1_mix += dac4; } if self.nr51.ch4_to_so2 { so2_mix += dac4; }

        let vol_factor = |vol: u8| (vol.wrapping_add(1)) as f32 / 8.0;
        let mut final_so1 = so1_mix * vol_factor(self.nr50.so1_output_level());
        let mut final_so2 = so2_mix * vol_factor(self.nr50.so2_output_level());

        final_so1 /= 4.0; final_so2 /= 4.0;

        let charge_factor = 0.999958_f32;
        let any_dac_on = self.channel1.nr12.dac_power() || self.channel2.nr22.dac_power() ||
                         self.channel3.nr30.dac_on() || self.channel4.nr42.dac_power();

        let hpf_out_so1 = final_so1 - self.hpf_capacitor_left;
        if any_dac_on { self.hpf_capacitor_left = final_so1 - hpf_out_so1 * charge_factor; }
        else { self.hpf_capacitor_left = 0.0; }

        let hpf_out_so2 = final_so2 - self.hpf_capacitor_right;
        if any_dac_on { self.hpf_capacitor_right = final_so2 - hpf_out_so2 * charge_factor; }
        else { self.hpf_capacitor_right = 0.0; }

        (hpf_out_so1, hpf_out_so2)
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        let _apu_on = self.nr52.is_apu_enabled();
        match addr {
            NR10_ADDR => self.channel1.nr10.read(), NR11_ADDR => self.channel1.nr11.read(),
            NR12_ADDR => self.channel1.nr12.read(), NR13_ADDR => self.channel1.nr13.read(),
            NR14_ADDR => self.channel1.nr14.read(),
            NR21_ADDR => self.channel2.nr21.read(), NR22_ADDR => self.channel2.nr22.read(),
            NR23_ADDR => self.channel2.nr23.read(), NR24_ADDR => self.channel2.nr24.read(),
            NR30_ADDR => self.channel3.nr30.read(), NR31_ADDR => self.channel3.nr31.read(),
            NR32_ADDR => self.channel3.nr32.read(), NR33_ADDR => self.channel3.nr33.read(),
            NR34_ADDR => self.channel3.nr34.read(),
            NR41_ADDR => self.channel4.nr41.read(), NR42_ADDR => self.channel4.nr42.read(),
            NR43_ADDR => self.channel4.nr43.read(), NR44_ADDR => self.channel4.nr44.read(),
            NR50_ADDR => self.nr50.read(), NR51_ADDR => self.nr51.read(), NR52_ADDR => self.nr52.read(),
            WAVE_PATTERN_RAM_START_ADDR..=WAVE_PATTERN_RAM_END_ADDR => {
                if self.channel3.enabled {
                    // CGB behavior: accesses redirect to the byte currently being read
                    let idx = self.channel3.current_wave_ram_byte_index();
                    self.wave_ram[idx]
                } else {
                    self.wave_ram[(addr - WAVE_PATTERN_RAM_START_ADDR) as usize]
                }
            }
            _ => {
                debug!("APU read from unhandled/unmapped address: {:#06X}", addr);
                0xFF
            }
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        // Store APU power state *before* potential NR52 write changes it.
        let apu_power_is_currently_off = !self.nr52.is_apu_enabled();

        if apu_power_is_currently_off {
            let is_nr52_write = addr == NR52_ADDR;
            let is_wave_ram_write = addr >= WAVE_PATTERN_RAM_START_ADDR && addr <= WAVE_PATTERN_RAM_END_ADDR;

            // Placeholder for DMG model check. Assume non-DMG for now (stricter access).
            let is_dmg_model = false; // TODO: Replace with actual model check
            let is_allowed_nrx1_write_on_dmg = if is_dmg_model {
                matches!(addr, NR11_ADDR | NR21_ADDR | NR31_ADDR | NR41_ADDR)
            } else {
                false
            };

            // If APU is off, only allow writes to NR52, Wave RAM, and (on DMG only) NRx1 registers.
            if !(is_nr52_write || is_wave_ram_write || is_allowed_nrx1_write_on_dmg) {
                // For other registers, if APU is off, the write might be ignored or behave differently.
                // SameBoy returns here for most registers if APU is off.
                // However, some registers (like NRx0, NRx2, NRx3, NRx4 for channels 1,2,4)
                // might still be writable on CGB even if APU is off, but their channels won't produce sound.
                // SameBoy's condition is:
                // `if (!gb->apu.global_enable && reg != GB_IO_NR52 && reg < GB_IO_WAV_START && (GB_is_cgb(gb) || (reg != GB_IO_NR11 ...)))`
                // This means if APU off, and not NR52, and not Wave RAM:
                //   On CGB: all other writes (0xFF10-0xFF2F) are IGNORED.
                //   On DMG: only NRx1 writes are allowed, others (NRx0, NRx2, NRx3, NRx4) are IGNORED.
                // So, the condition to RETURN (ignore write) is:
                // `apu_power_is_currently_off && !is_nr52_write && !is_wave_ram_write && !is_allowed_nrx1_write_on_dmg`
                // This seems correct. The original code was `if !apu_was_enabled && addr != NR52_ADDR { return; }`
                // which was too simple. It allowed all register writes if address was NR52, otherwise blocked all if APU off.

                // If it's not one of the allowed registers when APU is off, then ignore the write.
                return;
            }
        }

        // let trigger_val_in_write = (value >> 7) & 0x01 != 0; // This was identified as unused.
                                                              // Each NRx4 handler now derives its own 'trigger_from_value'.
        match addr {
            NR10_ADDR => {
                // Sweep Negate Glitch Logic (from SameBoy)
                // Condition: if (gb->apu.shadow_sweep_sample_length + gb->apu.channel1_completed_addend + old_negate > 0x7FF && !(value & 8))
                // shadow_sweep_sample_length == current channel1.sweep_shadow_frequency
                // channel1_completed_addend == current channel1.sweep_shadow_frequency >> old_sweep_shift
                // old_negate == 1 if previous direction was subtract, 0 otherwise. ( (old_nr10_val >> 3) & 1 )
                // !(value & 8) == new direction is addition

                let old_nr10_val = self.channel1.nr10.read(); // Read before it's overwritten
                let current_sweep_shadow_freq = self.channel1.get_sweep_shadow_frequency();

                let old_sweep_shift = old_nr10_val & 0x07;
                let old_direction_was_subtract_bit = (old_nr10_val >> 3) & 1; // 1 if subtract, 0 if add

                // Write the new NR10 value first
                self.channel1.nr10.write(value);

                let new_direction_is_add = (value & 0x08) == 0;

                if new_direction_is_add {
                    // Calculate the addend using the current sweep_shadow_frequency and the *old* shift value
                    // If old_sweep_shift is 0, SameBoy's effective addend for this check seems to be 0,
                    // because frequency_addend would not have been updated with shadow_freq >> 0.
                    // Or, if it means shadow_freq >> 0 = shadow_freq, then the sum can be very large.
                    // Based on `gb->apu.channel1_completed_addend = gb->apu.square_channels[0].frequency_addend;`
                    // and `frequency_addend = gb->apu.square_channels[index].shadow_frequency >> shift_amount;`
                    // if shift_amount is 0, this is a large value.
                    // Let's assume if old_sweep_shift is 0, the addend for *this specific glitch check* is effectively 0,
                    // as no meaningful "shift" would occur. More precisely, the hardware behavior for shift=0 needs care.
                    // Most emulators/docs say sweep with shift 0 does nothing to frequency.
                    // If old_sweep_shift == 0, addend is effectively sweep_shadow_freq, which is too large.
                    // The crucial part is `gb->apu.square_channels[0].frequency_addend` which is updated
                    // *during sweep calculation*. If shift is 0, sweep calc doesn't change frequency.
                    // So, let's consider the addend = 0 if old_sweep_shift == 0 for this specific check.
                    // No, the `channel1_completed_addend` is literally `shadow_freq >> shift`. If shift is 0, it's `shadow_freq`.

                    let addend = if old_sweep_shift == 0 {
                        // If shift is 0, the "addend" in `calculate_sweep_frequency` would be `shadow >> 0`, i.e. `shadow`.
                        // So for the purpose of this check, we use that.
                        current_sweep_shadow_freq
                    } else {
                        current_sweep_shadow_freq >> old_sweep_shift
                    };

                    let term1 = current_sweep_shadow_freq;
                    let term2 = addend;
                    let term3 = old_direction_was_subtract_bit as u16;

                    // Check if (current_shadow_freq + (current_shadow_freq >> old_shift) + old_negate_bit) > 2047
                    let sum_for_glitch_check = term1.saturating_add(term2).saturating_add(term3);

                    if sum_for_glitch_check > 2047 {
                        // self.channel1.disable_for_sweep_bug(); // Use existing method if it just disables
                        self.channel1.force_disable_channel();
                    }
                }
            },
            NR11_ADDR => {
                self.channel1.nr11.write(value);
                if self.channel1.enabled && self.channel1.nr14.is_length_enabled() {
                    self.channel1.reload_length_on_enable(self.frame_sequencer_step);
                }
            },
            NR12_ADDR => {
                let old_nr12_val = self.channel1.nr12.read();
                let mut live_volume = self.channel1.get_envelope_volume();
                // let old_envelope_period_timer = self.channel1.get_envelope_period_timer(); // Timer state not directly used for volume change

                // Approximation for SameBoy's 'lock.locked'.
                // 'lock.locked' is true if envelope period was 0 in the *previous* FS step 7.
                // If envelope is currently running, it implies it wasn't "locked" before this write.
                // If old_period was 0, it might have been locked.
                let old_period_val = old_nr12_val & 7;
                let can_tick_from_zero_period_if_old_period_was_zero = self.channel1.is_envelope_running();


                // Apply NRx2 write (updates nr12 struct in channel1)
                self.channel1.nr12.write(value);
                let new_nr12_val = value; // which is self.channel1.nr12.read() now

                // DAC power check (standard, happens after potential volume changes)
                if !self.channel1.nr12.dac_power() {
                    self.channel1.force_disable_channel();
                    // If DAC turned off, no further envelope glitch processing for volume.
                    // However, SameBoy seems to apply glitches even if DAC is off, then disables.
                    // For now, let's keep the original position of DAC check (at the very end).
                }

                let new_period_val = new_nr12_val & 7;
                let old_dir_increase = (old_nr12_val & 8) != 0;
                let new_dir_increase = (new_nr12_val & 8) != 0;

                // Simplified: !is_locked_approx. True if old_period was non-zero, OR if old_period was zero but envelope was running.
                let not_locked_approx = (old_period_val != 0) || can_tick_from_zero_period_if_old_period_was_zero;

                let mut should_tick = (new_period_val != 0) && (old_period_val == 0) && not_locked_approx;

                if (new_nr12_val & 0x0F) == 0x08 && (old_nr12_val & 0x0F) == 0x08 && not_locked_approx {
                    // This is: new_period is 0, new_dir is increase AND old_period is 0, old_dir is increase
                    should_tick = true;
                }

                let direction_inverted = new_dir_increase != old_dir_increase;

                if direction_inverted {
                    if new_dir_increase { // Direction changed to Increase
                        if old_period_val == 0 && not_locked_approx {
                            live_volume = 15u8.wrapping_sub(live_volume); // like ^= 0xF for 0-15 range
                        } else {
                            live_volume = 14u8.wrapping_sub(live_volume);
                        }
                    } else { // Direction changed to Decrease
                        live_volume = 16u8.wrapping_sub(live_volume);
                    }
                    live_volume &= 0x0F;
                    should_tick = false; // Inversion overrides ticking due to period change
                }

                if should_tick {
                    if new_dir_increase { // Envelope Adding
                        if live_volume < 15 {
                            live_volume += 1;
                        }
                    } else { // Envelope Subtracting
                        if live_volume > 0 {
                            live_volume -= 1;
                        }
                    }
                    // live_volume &= 0x0F; // Already within 0-15 due to checks
                }

                self.channel1.set_envelope_volume(live_volume);

                // Final DAC power check (if it was turned off by this NR12 write)
                if !self.channel1.nr12.dac_power() {
                    self.channel1.force_disable_channel();
                }
            },
            NR13_ADDR => self.channel1.nr13.write(value),
            NR14_ADDR => {
                let prev_len_enabled = self.channel1.nr14.is_length_enabled();
                let len_counter_was_non_zero = self.channel1.get_length_counter() > 0;

                let new_len_enabled_from_value = (value & 0x40) != 0;
                let trigger_from_value = (value & 0x80) != 0;

                // 1. Length Clock Glitch Check
                // Occurs if length is being enabled, on a FS step that normally clocks length, and length counter > 0.
                let fs_step_is_length_clocking_type = matches!(self.frame_sequencer_step, 1 | 3 | 5 | 7);
                if new_len_enabled_from_value && !prev_len_enabled && fs_step_is_length_clocking_type && len_counter_was_non_zero {
                    self.channel1.extra_length_clock(trigger_from_value); // Pass trigger state from current write
                }

                // 2. Actual NR14 write (this updates internal nr14 state including trigger flag and length_enable)
                self.channel1.nr14.write(value);

                // 3. Triggering or Length Enabling Logic
                if self.channel1.nr14.consume_trigger_flag() { // Checks internal flag set by write(value), effectively 'trigger_from_value'
                    let lf_div = 0; // Placeholder for actual (div_clock_sync >> 4) & 1 logic
                    self.channel1.trigger(self.frame_sequencer_step, lf_div);
                } else {
                    // If not triggered by this write, but length was just enabled by this write (value had bit 6 set)
                    // new_len_enabled_from_value is the same as self.channel1.nr14.is_length_enabled() at this point if write was successful
                    if new_len_enabled_from_value && !prev_len_enabled {
                        // Reload length counter. This handles:
                        // - Loading 64 (or 64-data) if it was 0.
                        // - Applying the "63 instead of 64" rule if applicable (when data is 0 for max length).
                        self.channel1.reload_length_on_enable(self.frame_sequencer_step);
                    }
                }
            },
            NR21_ADDR => {
                self.channel2.nr21.write(value);
                if self.channel2.enabled && self.channel2.nr24.is_length_enabled() {
                    self.channel2.reload_length_on_enable(self.frame_sequencer_step);
                }
            },
            NR22_ADDR => { // Apply same NRx2 glitch logic as for NR12
                let old_nr22_val = self.channel2.nr22.read();
                let mut live_volume = self.channel2.get_envelope_volume();

                let old_period_val = old_nr22_val & 7;
                let can_tick_from_zero_period_if_old_period_was_zero = self.channel2.is_envelope_running();

                self.channel2.nr22.write(value);
                let new_nr22_val = value;

                if !self.channel2.nr22.dac_power() {
                    self.channel2.force_disable_channel();
                }

                let new_period_val = new_nr22_val & 7;
                let old_dir_increase = (old_nr22_val & 8) != 0;
                let new_dir_increase = (new_nr22_val & 8) != 0;

                let not_locked_approx = (old_period_val != 0) || can_tick_from_zero_period_if_old_period_was_zero;

                let mut should_tick = (new_period_val != 0) && (old_period_val == 0) && not_locked_approx;

                if (new_nr22_val & 0x0F) == 0x08 && (old_nr22_val & 0x0F) == 0x08 && not_locked_approx {
                    should_tick = true;
                }

                let direction_inverted = new_dir_increase != old_dir_increase;

                if direction_inverted {
                    if new_dir_increase {
                        if old_period_val == 0 && not_locked_approx {
                            live_volume = 15u8.wrapping_sub(live_volume);
                        } else {
                            live_volume = 14u8.wrapping_sub(live_volume);
                        }
                    } else {
                        live_volume = 16u8.wrapping_sub(live_volume);
                    }
                    live_volume &= 0x0F;
                    should_tick = false;
                }

                if should_tick {
                    if new_dir_increase {
                        if live_volume < 15 { live_volume += 1; }
                    } else {
                        if live_volume > 0 { live_volume -= 1; }
                    }
                }

                self.channel2.set_envelope_volume(live_volume);

                if !self.channel2.nr22.dac_power() {
                    self.channel2.force_disable_channel();
                }
            },
            NR23_ADDR => self.channel2.nr23.write(value),
            NR24_ADDR => {
                let prev_len_enabled = self.channel2.nr24.is_length_enabled();
                let len_counter_was_non_zero = self.channel2.get_length_counter() > 0;

                let new_len_enabled_from_value = (value & 0x40) != 0;
                let trigger_from_value = (value & 0x80) != 0; // Used for extra_length_clock and consume_trigger_flag

                // 1. Length Clock Glitch Check (Corrected logic)
                let fs_step_is_length_clocking_type = matches!(self.frame_sequencer_step, 1 | 3 | 5 | 7);
                if new_len_enabled_from_value && !prev_len_enabled && fs_step_is_length_clocking_type && len_counter_was_non_zero {
                    self.channel2.extra_length_clock(trigger_from_value);
                }

                // 2. Actual NR24 write
                self.channel2.nr24.write(value);

                // 3. Triggering or Length Enabling Logic
                if self.channel2.nr24.consume_trigger_flag() { // Checks internal flag set by write(value)
                    let lf_div = 0; // Placeholder
                    self.channel2.trigger(self.frame_sequencer_step, lf_div);
                } else {
                    if new_len_enabled_from_value && !prev_len_enabled {
                        self.channel2.reload_length_on_enable(self.frame_sequencer_step);
                    }
                }
            },
            NR30_ADDR => {
                let prev_dac_on = self.channel3.nr30.dac_on();
                self.channel3.nr30.write(value);
                let new_dac_on = self.channel3.nr30.dac_on();

                self.channel3.enabled = new_dac_on; // Update channel enabled state based on DAC

                if !new_dac_on {
                    self.channel3.set_pulsed(false); // Clear pulsed flag if DAC turned off
                    if prev_dac_on { // If DAC was on and is now turned off
                        // SameBoy: if (gb->apu.wave_channel.sample_countdown == 0 || gb->apu.wave_channel.wave_form_just_read)
                        // Our frequency_timer is equivalent to sample_countdown for this check's purpose.
                        if self.channel3.get_frequency_timer() == 0 || self.channel3.get_wave_form_just_read() {
                            self.channel3.reload_current_sample_buffer(&self.wave_ram);
                        }
                    }
                }
            },
            NR31_ADDR => {
                self.channel3.nr31.write(value);
                if self.channel3.enabled && self.channel3.nr34.is_length_enabled() {
                    self.channel3.reload_length_on_enable(self.frame_sequencer_step);
                }
            },
            NR32_ADDR => self.channel3.nr32.write(value),
            NR33_ADDR => self.channel3.nr33.write(value),
            NR34_ADDR => {
                let ch3_was_active_for_corruption_check = self.channel3.enabled && self.channel3.nr30.dac_on();
                let trigger_is_being_set_for_corruption_check = (value >> 7) & 0x01 != 0;
                if trigger_is_being_set_for_corruption_check && ch3_was_active_for_corruption_check {
                    self.channel3.nr30.write(0x00);
                    self.channel3.nr30.write(0x80);
                }

                let prev_len_enabled = self.channel3.nr34.is_length_enabled();
                let len_counter_was_non_zero = self.channel3.get_length_counter() > 0;

                let new_len_enabled_from_value = (value & 0x40) != 0;
                let trigger_from_value = (value & 0x80) != 0;

                // 1. Length Clock Glitch Check (Corrected logic)
                let fs_step_is_length_clocking_type = matches!(self.frame_sequencer_step, 1 | 3 | 5 | 7);
                if new_len_enabled_from_value && !prev_len_enabled && fs_step_is_length_clocking_type && len_counter_was_non_zero {
                    self.channel3.extra_length_clock(trigger_from_value);
                }

                // 2. Actual NR34 write
                self.channel3.nr34.write(value);

                // 3. Triggering or Length Enabling Logic
                if self.channel3.nr34.consume_trigger_flag() {
                    self.channel3.trigger(&self.wave_ram, self.frame_sequencer_step);
                } else {
                    if new_len_enabled_from_value && !prev_len_enabled {
                        self.channel3.reload_length_on_enable(self.frame_sequencer_step);
                    }
                }
            },
            NR41_ADDR => {
                self.channel4.nr41.write(value);
                if self.channel4.enabled && self.channel4.nr44.is_length_enabled() {
                    self.channel4.reload_length_on_enable(self.frame_sequencer_step);
                }
            },
            NR42_ADDR => { // Apply same NRx2 glitch logic as for NR12
                let old_nr42_val = self.channel4.nr42.read();
                let mut live_volume = self.channel4.get_envelope_volume();

                let old_period_val = old_nr42_val & 7;
                let can_tick_from_zero_period_if_old_period_was_zero = self.channel4.is_envelope_running();

                self.channel4.nr42.write(value);
                let new_nr42_val = value;

                if !self.channel4.nr42.dac_power() {
                    self.channel4.force_disable_channel();
                }

                let new_period_val = new_nr42_val & 7;
                let old_dir_increase = (old_nr42_val & 8) != 0;
                let new_dir_increase = (new_nr42_val & 8) != 0;

                let not_locked_approx = (old_period_val != 0) || can_tick_from_zero_period_if_old_period_was_zero;

                let mut should_tick = (new_period_val != 0) && (old_period_val == 0) && not_locked_approx;

                if (new_nr42_val & 0x0F) == 0x08 && (old_nr42_val & 0x0F) == 0x08 && not_locked_approx {
                    should_tick = true;
                }

                let direction_inverted = new_dir_increase != old_dir_increase;

                if direction_inverted {
                    if new_dir_increase {
                        if old_period_val == 0 && not_locked_approx {
                            live_volume = 15u8.wrapping_sub(live_volume);
                        } else {
                            live_volume = 14u8.wrapping_sub(live_volume);
                        }
                    } else {
                        live_volume = 16u8.wrapping_sub(live_volume);
                    }
                    live_volume &= 0x0F;
                    should_tick = false;
                }

                if should_tick {
                    if new_dir_increase {
                        if live_volume < 15 { live_volume += 1; }
                    } else {
                        if live_volume > 0 { live_volume -= 1; }
                    }
                }

                self.channel4.set_envelope_volume(live_volume);

                if !self.channel4.nr42.dac_power() {
                    self.channel4.force_disable_channel();
                }
            },
            NR43_ADDR => {
                // TODO: Implement SameBoy's NR43 write glitch if necessary.
                // It involves checking old vs new (div_apu_counter >> shift_amount)
                // and potentially stepping LFSR if specific model conditions met.
                // For now, just update the parameters in Channel4.

                self.channel4.nr43.write(value); // Update raw NR43 register struct first

                let new_shift_amount = (value >> 4) & 0x0F;
                let new_raw_divider_bits = value & 0x07;

                self.channel4.set_lfsr_shift_amount(new_shift_amount);
                self.channel4.set_lfsr_clock_divider(new_raw_divider_bits);

                // The NR43 write can also affect the lfsr_step_countdown immediately
                // if the divisor changes. SameBoy seems to allow the current countdown
                // to continue and the new divisor takes effect on next reload.
                // However, if the current countdown exceeds the new divisor, it might get
                // re-adjusted. For now, we assume reload on next natural expiry or trigger.
            },
            NR44_ADDR => {
                let prev_len_enabled = self.channel4.nr44.is_length_enabled();
                let len_counter_was_non_zero = self.channel4.get_length_counter() > 0;

                let new_len_enabled_from_value = (value & 0x40) != 0;
                let trigger_from_value = (value & 0x80) != 0;

                // 1. Length Clock Glitch Check (Corrected logic)
                let fs_step_is_length_clocking_type = matches!(self.frame_sequencer_step, 1 | 3 | 5 | 7);
                if new_len_enabled_from_value && !prev_len_enabled && fs_step_is_length_clocking_type && len_counter_was_non_zero {
                    self.channel4.extra_length_clock(trigger_from_value);
                }

                // 2. Actual NR44 write
                self.channel4.nr44.write(value);

                // 3. Triggering or Length Enabling Logic
                if self.channel4.nr44.consume_trigger_flag() {
                    let lf_div = 0; // Placeholder
                    self.channel4.trigger(self.frame_sequencer_step, lf_div);
                } else {
                    if new_len_enabled_from_value && !prev_len_enabled {
                        self.channel4.reload_length_on_enable(self.frame_sequencer_step);
                    }
                }
            },
            NR50_ADDR => self.nr50.write(value),
            NR51_ADDR => self.nr51.write(value),
            NR52_ADDR => {
                let prev_power_state = self.nr52.is_apu_enabled();
                self.nr52.write(value);
                let new_power_state = self.nr52.is_apu_enabled();
                if prev_power_state && !new_power_state {
                    self.full_apu_reset_on_power_off();
                } else if !prev_power_state && new_power_state { // APU turning ON
                    self.frame_sequencer_step = 0; // Reset frame sequencer step
                    self.frame_sequencer_counter = CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK; // Reset counter for FS
                    self.reset_power_on_channel_flags();

                    // TODO: Implement full skip_div_event logic based on CPU DIV register state
                    // For now, as a placeholder, let's assume it's not skipped.
                    // If it were to be skipped: self.skip_next_frame_sequencer_tick = true;
                    // This needs info from CPU (DIV register state) at the moment of NR52 write.
                    // e.g. if cpu.get_div_apu_sync_bit_5_or_4() { self.skip_next_frame_sequencer_tick = true; }
                    self.skip_next_frame_sequencer_tick = false; // Default placeholder
                }
            }
            WAVE_PATTERN_RAM_START_ADDR..=WAVE_PATTERN_RAM_END_ADDR => {
                let ch3_is_active = self.channel3.enabled;
                if ch3_is_active {
                    // CGB behavior: redirect to byte currently being read
                    let idx = self.channel3.current_wave_ram_byte_index();
                    self.wave_ram[idx] = value;
                } else {
                    self.wave_ram[(addr - WAVE_PATTERN_RAM_START_ADDR) as usize] = value;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
