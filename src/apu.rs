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
    pub(super) fn new() -> Self { Self { wave_pattern_duty: 0b10, sound_length_data: 0x00 } }
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
    pub(super) fn new() -> Self { Self { wave_pattern_duty: 0b10, sound_length_data: 0x00 } }
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
        (if self.all_sound_on { 0x80 } else { 0x00 }) |
        0x70 |
        (if self.ch4_status { 0x08 } else { 0x00 }) |
        (if self.ch3_status { 0x04 } else { 0x00 }) |
        (if self.ch2_status { 0x02 } else { 0x00 }) |
        (if self.ch1_status { 0x01 } else { 0x00 })
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
            self.clock_frame_sequencer();
        }

        if self.nr52.is_apu_enabled() {
            for _ in 0..cpu_t_cycles {
                self.channel1.tick(); self.channel2.tick();
                self.channel3.tick(&self.wave_ram); self.channel4.tick();
            }
        }
    }

    fn clock_frame_sequencer(&mut self) {
        let apu_on = self.nr52.is_apu_enabled();
        if apu_on {
            if self.frame_sequencer_step % 2 == 0 {
                self.channel1.clock_length(); self.channel2.clock_length();
                self.channel3.clock_length(); self.channel4.clock_length();
            }
            if self.frame_sequencer_step == 2 || self.frame_sequencer_step == 6 {
                self.channel1.clock_sweep();
            }
            if self.frame_sequencer_step == 7 {
                self.channel1.clock_envelope(); self.channel2.clock_envelope();
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
                if self.nr52.is_apu_enabled() && self.channel3.nr30.dac_on() {
                    self.wave_ram[(addr - WAVE_PATTERN_RAM_START_ADDR) as usize]
                } else { 0xFF }
            }
            _ => {
                debug!("APU read from unhandled/unmapped address: {:#06X}", addr);
                0xFF
            }
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        let apu_was_enabled = self.nr52.is_apu_enabled();
        if !apu_was_enabled && addr != NR52_ADDR { return; }

        let trigger_val_in_write = (value >> 7) & 0x01 != 0;

        match addr {
            NR10_ADDR => {
                let prev_sweep_direction_was_subtraction = !self.channel1.nr10.sweep_direction_is_increase();
                self.channel1.nr10.write(value);
                let new_sweep_direction_is_addition = self.channel1.nr10.sweep_direction_is_increase();
                if prev_sweep_direction_was_subtraction && new_sweep_direction_is_addition && self.channel1.has_subtraction_sweep_calculated() {
                    self.channel1.disable_for_sweep_bug();
                }
            },
            NR11_ADDR => self.channel1.nr11.write(value),
            NR12_ADDR => {
                let old_period = self.channel1.nr12.envelope_period_val();
                let old_dir_increase = self.channel1.nr12.envelope_direction_is_increase();
                let env_was_running = self.channel1.is_envelope_running();
                let mut live_volume = self.channel1.get_envelope_volume();
                self.channel1.nr12.write(value);
                let new_dir_increase = self.channel1.nr12.envelope_direction_is_increase();
                if old_dir_increase != new_dir_increase { live_volume = 16u8.wrapping_sub(live_volume); }
                if old_period == 0 && env_was_running { live_volume = live_volume.wrapping_add(1); }
                else if !old_dir_increase { live_volume = live_volume.wrapping_add(2); }
                self.channel1.set_envelope_volume(live_volume);
                if !self.channel1.nr12.dac_power() { self.channel1.force_disable_channel(); }
            },
            NR13_ADDR => self.channel1.nr13.write(value),
            NR14_ADDR => {
                let prev_len_enabled = self.channel1.nr14.is_length_enabled();
                let len_counter_was_non_zero = self.channel1.get_length_counter() > 0;
                self.channel1.nr14.write(value);
                let new_len_enabled = self.channel1.nr14.is_length_enabled();
                let next_fs_step_will_not_clock_length = matches!(self.frame_sequencer_step, 0 | 2 | 4 | 6);
                if next_fs_step_will_not_clock_length && !prev_len_enabled && new_len_enabled && len_counter_was_non_zero {
                    self.channel1.extra_length_clock(trigger_val_in_write);
                }
                if self.channel1.nr14.consume_trigger_flag() { self.channel1.trigger(self.frame_sequencer_step); }
            },
            NR21_ADDR => self.channel2.nr21.write(value),
            NR22_ADDR => {
                let old_period = self.channel2.nr22.envelope_period_val();
                let old_dir_increase = self.channel2.nr22.envelope_direction_is_increase();
                let env_was_running = self.channel2.is_envelope_running();
                let mut live_volume = self.channel2.get_envelope_volume();
                self.channel2.nr22.write(value);
                let new_dir_increase = self.channel2.nr22.envelope_direction_is_increase();
                if old_dir_increase != new_dir_increase { live_volume = 16u8.wrapping_sub(live_volume); }
                if old_period == 0 && env_was_running { live_volume = live_volume.wrapping_add(1); }
                else if !old_dir_increase { live_volume = live_volume.wrapping_add(2); }
                self.channel2.set_envelope_volume(live_volume);
                if !self.channel2.nr22.dac_power() { self.channel2.force_disable_channel(); }
            },
            NR23_ADDR => self.channel2.nr23.write(value),
            NR24_ADDR => {
                let prev_len_enabled = self.channel2.nr24.is_length_enabled();
                let len_counter_was_non_zero = self.channel2.get_length_counter() > 0;
                self.channel2.nr24.write(value);
                let new_len_enabled = self.channel2.nr24.is_length_enabled();
                let next_fs_step_will_not_clock_length = matches!(self.frame_sequencer_step, 0 | 2 | 4 | 6);
                if next_fs_step_will_not_clock_length && !prev_len_enabled && new_len_enabled && len_counter_was_non_zero {
                    self.channel2.extra_length_clock(trigger_val_in_write);
                }
                if self.channel2.nr24.consume_trigger_flag() { self.channel2.trigger(self.frame_sequencer_step); }
            },
            NR30_ADDR => self.channel3.nr30.write(value),
            NR31_ADDR => self.channel3.nr31.write(value),
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
                self.channel3.nr34.write(value);
                let new_len_enabled = self.channel3.nr34.is_length_enabled();
                let next_fs_step_will_not_clock_length = matches!(self.frame_sequencer_step, 0 | 2 | 4 | 6);
                if next_fs_step_will_not_clock_length && !prev_len_enabled && new_len_enabled && len_counter_was_non_zero {
                    self.channel3.extra_length_clock(trigger_val_in_write);
                }
                if self.channel3.nr34.consume_trigger_flag() { self.channel3.trigger(&self.wave_ram, self.frame_sequencer_step); }
            },
            NR41_ADDR => self.channel4.nr41.write(value),
            NR42_ADDR => {
                let old_period = self.channel4.nr42.envelope_period_val();
                let old_dir_increase = self.channel4.nr42.envelope_direction_is_increase();
                let env_was_running = self.channel4.is_envelope_running();
                let mut live_volume = self.channel4.get_envelope_volume();
                self.channel4.nr42.write(value);
                let new_dir_increase = self.channel4.nr42.envelope_direction_is_increase();
                if old_dir_increase != new_dir_increase { live_volume = 16u8.wrapping_sub(live_volume); }
                if old_period == 0 && env_was_running { live_volume = live_volume.wrapping_add(1); }
                else if !old_dir_increase { live_volume = live_volume.wrapping_add(2); }
                self.channel4.set_envelope_volume(live_volume);
                if !self.channel4.nr42.dac_power() { self.channel4.force_disable_channel(); }
            },
            NR43_ADDR => self.channel4.nr43.write(value),
            NR44_ADDR => {
                let prev_len_enabled = self.channel4.nr44.is_length_enabled();
                let len_counter_was_non_zero = self.channel4.get_length_counter() > 0;
                self.channel4.nr44.write(value);
                let new_len_enabled = self.channel4.nr44.is_length_enabled();
                let next_fs_step_will_not_clock_length = matches!(self.frame_sequencer_step, 0 | 2 | 4 | 6);
                if next_fs_step_will_not_clock_length && !prev_len_enabled && new_len_enabled && len_counter_was_non_zero {
                    self.channel4.extra_length_clock(trigger_val_in_write);
                }
                if self.channel4.nr44.consume_trigger_flag() { self.channel4.trigger(self.frame_sequencer_step); }
            },
            NR50_ADDR => self.nr50.write(value),
            NR51_ADDR => self.nr51.write(value),
            NR52_ADDR => {
                let prev_power_state = self.nr52.is_apu_enabled();
                self.nr52.write(value);
                let new_power_state = self.nr52.is_apu_enabled();
                if prev_power_state && !new_power_state {
                    self.full_apu_reset_on_power_off();
                } else if !prev_power_state && new_power_state {
                    self.frame_sequencer_step = 0;
                    self.frame_sequencer_counter = CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK;
                    self.reset_power_on_channel_flags();
                }
            }
            WAVE_PATTERN_RAM_START_ADDR..=WAVE_PATTERN_RAM_END_ADDR => {
                if apu_was_enabled {
                     self.wave_ram[(addr - WAVE_PATTERN_RAM_START_ADDR) as usize] = value;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
