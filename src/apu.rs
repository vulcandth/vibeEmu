// src/apu.rs

pub mod channel1;
pub mod channel2;
pub mod channel3;
pub mod channel4;
use self::channel1::Channel1;
use self::channel2::Channel2;
use self::channel3::Channel3;
use self::channel4::Channel4;
use crate::bus::SystemMode;
use log::debug;

pub const CPU_CLOCK_HZ: u32 = 4194304;
const FRAME_SEQUENCER_FREQUENCY_HZ: u32 = 512;
const CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK: u32 = CPU_CLOCK_HZ / FRAME_SEQUENCER_FREQUENCY_HZ;

const NR10_ADDR: u16 = 0xFF10; const NR11_ADDR: u16 = 0xFF11; const NR12_ADDR: u16 = 0xFF12; const NR13_ADDR: u16 = 0xFF13; const NR14_ADDR: u16 = 0xFF14;
const NR21_ADDR: u16 = 0xFF16; const NR22_ADDR: u16 = 0xFF17; const NR23_ADDR: u16 = 0xFF18; const NR24_ADDR: u16 = 0xFF19;
const NR30_ADDR: u16 = 0xFF1A; const NR31_ADDR: u16 = 0xFF1B; const NR32_ADDR: u16 = 0xFF1C; const NR33_ADDR: u16 = 0xFF1D; const NR34_ADDR: u16 = 0xFF1E;
const NR41_ADDR: u16 = 0xFF20; const NR42_ADDR: u16 = 0xFF21; const NR43_ADDR: u16 = 0xFF22; const NR44_ADDR: u16 = 0xFF23;
const NR50_ADDR: u16 = 0xFF24; const NR51_ADDR: u16 = 0xFF25; const NR52_ADDR: u16 = 0xFF26;
const WAVE_PATTERN_RAM_START_ADDR: u16 = 0xFF30; const WAVE_PATTERN_RAM_END_ADDR: u16 = 0xFF3F;

#[derive(Debug, Clone, Copy)] pub(super) struct Nr10 { sweep_time: u8, sweep_direction: u8, sweep_shift: u8, }
impl Nr10 { pub(super) fn new() -> Self { Self { sweep_time: 0, sweep_direction: 0, sweep_shift: 0 } } pub(super) fn read(&self) -> u8 { 0x80 | (self.sweep_time << 4) | (self.sweep_direction << 3) | self.sweep_shift } pub(super) fn write(&mut self, value: u8) { self.sweep_time = (value >> 4) & 0x07; self.sweep_direction = (value >> 3) & 0x01; self.sweep_shift = value & 0x07; } pub(super) fn sweep_period(&self) -> u8 { self.sweep_time } pub(super) fn sweep_shift_val(&self) -> u8 { self.sweep_shift } pub(super) fn sweep_direction_is_increase(&self) -> bool { self.sweep_direction == 0 } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr11 { wave_pattern_duty: u8, sound_length_data: u8, }
impl Nr11 { pub(super) fn new() -> Self { Self { wave_pattern_duty: 0b00, sound_length_data: 0x00 } } pub(super) fn read(&self) -> u8 { (self.wave_pattern_duty << 6) | 0x3F } pub(super) fn write(&mut self, value: u8) { self.wave_pattern_duty = (value >> 6) & 0x03; self.sound_length_data = value & 0x3F; } pub(super) fn initial_length_timer_val(&self) -> u8 { self.sound_length_data } pub(super) fn wave_pattern_duty_val(&self) -> u8 { self.wave_pattern_duty } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr12 { initial_volume: u8, envelope_direction: u8, envelope_period: u8, }
impl Nr12 { pub(super) fn new() -> Self { Self { initial_volume: 0, envelope_direction: 0, envelope_period: 0 } } pub(super) fn read(&self) -> u8 { (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period } pub(super) fn write(&mut self, value: u8) { self.initial_volume = (value >> 4) & 0x0F; self.envelope_direction = (value >> 3) & 0x01; self.envelope_period = value & 0x07; } pub(super) fn initial_volume_val(&self) -> u8 { self.initial_volume } pub(super) fn dac_power(&self) -> bool { (self.read() & 0xF8) != 0 } pub(super) fn envelope_period_val(&self) -> u8 { self.envelope_period } pub(super) fn envelope_direction_is_increase(&self) -> bool { self.envelope_direction == 1 } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr13 { freq_lo: u8 }
impl Nr13 { pub(super) fn new() -> Self { Self { freq_lo: 0x00 } } pub(super) fn read(&self) -> u8 { 0xFF } pub(super) fn write(&mut self, value: u8) { self.freq_lo = value; } pub(super) fn freq_lo_val(&self) -> u8 { self.freq_lo } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr14 { trigger_bit_in_write: bool, length_enable: bool, freq_hi: u8, }
impl Nr14 { pub(super) fn new() -> Self { Self { trigger_bit_in_write: false, length_enable: false, freq_hi: 0 } } pub(super) fn read(&self) -> u8 { (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF } pub(super) fn write(&mut self, value: u8) { self.trigger_bit_in_write = (value >> 7) & 0x01 != 0; self.length_enable = (value >> 6) & 0x01 != 0; self.freq_hi = value & 0x07; } pub(super) fn is_length_enabled(&self) -> bool { self.length_enable } pub(super) fn frequency_msb_val(&self) -> u8 { self.freq_hi } pub(super) fn write_frequency_msb(&mut self, val: u8) { self.freq_hi = val & 0x07; } pub(super) fn consume_trigger_flag(&mut self) -> bool { let triggered = self.trigger_bit_in_write; self.trigger_bit_in_write = false; triggered } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr21 { wave_pattern_duty: u8, sound_length_data: u8 }
impl Nr21 { pub(super) fn new() -> Self { Self { wave_pattern_duty: 0b00, sound_length_data: 0x00 } } pub(super) fn read(&self) -> u8 { (self.wave_pattern_duty << 6) | 0x3F } pub(super) fn write(&mut self, value: u8) { self.wave_pattern_duty = (value >> 6) & 0x03; self.sound_length_data = value & 0x3F; } pub(super) fn initial_length_timer_val(&self) -> u8 { self.sound_length_data } pub(super) fn wave_pattern_duty_val(&self) -> u8 { self.wave_pattern_duty } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr22 { initial_volume: u8, envelope_direction: u8, envelope_period: u8 }
impl Nr22 { pub(super) fn new() -> Self { Self { initial_volume: 0, envelope_direction: 0, envelope_period: 0 } } pub(super) fn read(&self) -> u8 { (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period } pub(super) fn write(&mut self, value: u8) { self.initial_volume = (value >> 4) & 0x0F; self.envelope_direction = (value >> 3) & 0x01; self.envelope_period = value & 0x07; } pub(super) fn initial_volume_val(&self) -> u8 { self.initial_volume } pub(super) fn dac_power(&self) -> bool { (self.read() & 0xF8) != 0 } pub(super) fn envelope_period_val(&self) -> u8 { self.envelope_period } pub(super) fn envelope_direction_is_increase(&self) -> bool { self.envelope_direction == 1 } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr23 { freq_lo: u8 }
impl Nr23 { pub(super) fn new() -> Self { Self { freq_lo: 0x00 } } pub(super) fn read(&self) -> u8 { 0xFF } pub(super) fn write(&mut self, value: u8) { self.freq_lo = value; } pub(super) fn freq_lo_val(&self) -> u8 { self.freq_lo } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr24 { trigger_bit_in_write: bool, length_enable: bool, freq_hi: u8 }
impl Nr24 { pub(super) fn new() -> Self { Self { trigger_bit_in_write: false, length_enable: false, freq_hi: 0 } } pub(super) fn read(&self) -> u8 { (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF } pub(super) fn write(&mut self, value: u8) { self.trigger_bit_in_write = (value >> 7) & 0x01 != 0; self.length_enable = (value >> 6) & 0x01 != 0; self.freq_hi = value & 0x07; } pub(super) fn is_length_enabled(&self) -> bool { self.length_enable } pub(super) fn frequency_msb_val(&self) -> u8 { self.freq_hi } pub(super) fn consume_trigger_flag(&mut self) -> bool { let triggered = self.trigger_bit_in_write; self.trigger_bit_in_write = false; triggered } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr30 { sound_on: bool }
impl Nr30 { pub(super) fn new() -> Self { Self { sound_on: false } } pub(super) fn read(&self) -> u8 { (if self.sound_on { 0x80 } else { 0x00 }) | 0x7F } pub(super) fn write(&mut self, value: u8) { self.sound_on = (value >> 7) & 0x01 != 0; } pub(super) fn dac_on(&self) -> bool { self.sound_on } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr31 { sound_length: u8 }
impl Nr31 { pub(super) fn new() -> Self { Self { sound_length: 0x00 } } pub(super) fn read(&self) -> u8 { 0xFF } pub(super) fn write(&mut self, value: u8) { self.sound_length = value; } pub(super) fn sound_length_val(&self) -> u8 { self.sound_length } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr32 { output_level: u8 }
impl Nr32 { pub(super) fn new() -> Self { Self { output_level: 0b00 } } pub(super) fn read(&self) -> u8 { (self.output_level << 5) | 0x9F } pub(super) fn write(&mut self, value: u8) { self.output_level = (value >> 5) & 0x03; } pub(super) fn get_volume_shift(&self) -> u8 { match self.output_level { 0b01 => 0, 0b10 => 1, 0b11 => 2, _ => 4 } } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr33 { freq_lo: u8 }
impl Nr33 { pub(super) fn new() -> Self { Self { freq_lo: 0x00 } } pub(super) fn read(&self) -> u8 { 0xFF } pub(super) fn write(&mut self, value: u8) { self.freq_lo = value; } pub(super) fn freq_lo_val(&self) -> u8 { self.freq_lo } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr34 { trigger_bit_in_write: bool, length_enable: bool, freq_hi: u8 }
impl Nr34 { pub(super) fn new() -> Self { Self { trigger_bit_in_write: false, length_enable: false, freq_hi: 0 } } pub(super) fn read(&self) -> u8 { (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF } pub(super) fn write(&mut self, value: u8) { self.trigger_bit_in_write = (value >> 7) & 0x01 != 0; self.length_enable = (value >> 6) & 0x01 != 0; self.freq_hi = value & 0x07; } pub(super) fn is_length_enabled(&self) -> bool { self.length_enable } pub(super) fn frequency_msb_val(&self) -> u8 { self.freq_hi } pub(super) fn consume_trigger_flag(&mut self) -> bool { let triggered = self.trigger_bit_in_write; self.trigger_bit_in_write = false; triggered } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr41 { sound_length_data: u8 }
impl Nr41 { pub(super) fn new() -> Self { Self { sound_length_data: 0x00 } } pub(super) fn read(&self) -> u8 { 0xFF } pub(super) fn write(&mut self, value: u8) { self.sound_length_data = value & 0x3F; } pub(super) fn initial_length_timer_val(&self) -> u8 { self.sound_length_data } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr42 { initial_volume: u8, envelope_direction: u8, envelope_period: u8 }
impl Nr42 { pub(super) fn new() -> Self { Self { initial_volume: 0, envelope_direction: 0, envelope_period: 0 } } pub(super) fn read(&self) -> u8 { (self.initial_volume << 4) | (self.envelope_direction << 3) | self.envelope_period } pub(super) fn write(&mut self, value: u8) { self.initial_volume = (value >> 4) & 0x0F; self.envelope_direction = (value >> 3) & 0x01; self.envelope_period = value & 0x07; } pub(super) fn initial_volume_val(&self) -> u8 { self.initial_volume } pub(super) fn dac_power(&self) -> bool { (self.read() & 0xF8) != 0 } pub(super) fn envelope_period_val(&self) -> u8 { self.envelope_period } pub(super) fn envelope_direction_is_increase(&self) -> bool { self.envelope_direction == 1 } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr43 { shift_clock_freq: u8, counter_width: u8, dividing_ratio: u8 }
impl Nr43 { pub(super) fn new() -> Self { Self { shift_clock_freq: 0, counter_width: 0, dividing_ratio: 0 } } pub(super) fn read(&self) -> u8 { (self.shift_clock_freq << 4) | (self.counter_width << 3) | self.dividing_ratio } pub(super) fn write(&mut self, value: u8) { self.shift_clock_freq = (value >> 4) & 0x0F; self.counter_width = (value >> 3) & 0x01; self.dividing_ratio = value & 0x07; } pub(super) fn clock_shift(&self) -> u8 { self.shift_clock_freq } pub(super) fn lfsr_width_is_7bit(&self) -> bool { self.counter_width == 1 } pub(super) fn clock_divider_val(&self) -> u8 { self.dividing_ratio } }
#[derive(Debug, Clone, Copy)] pub(super) struct Nr44 { trigger_bit_in_write: bool, length_enable: bool }
impl Nr44 { pub(super) fn new() -> Self { Self { trigger_bit_in_write: false, length_enable: false } } pub(super) fn read(&self) -> u8 { (if self.length_enable { 0x40 } else { 0x00 }) | 0xBF } pub(super) fn write(&mut self, value: u8) { self.trigger_bit_in_write = (value >> 7) & 0x01 != 0; self.length_enable = (value >> 6) & 0x01 != 0; } pub(super) fn is_length_enabled(&self) -> bool { self.length_enable } pub(super) fn consume_trigger_flag(&mut self) -> bool { let triggered = self.trigger_bit_in_write; self.trigger_bit_in_write = false; triggered } }
#[derive(Debug, Clone, Copy, Default)] struct Nr52 { all_sound_on: bool, ch4_status: bool, ch3_status: bool, ch2_status: bool, ch1_status: bool, }
impl Nr52 { fn new() -> Self { Self::default() } fn is_apu_enabled(&self) -> bool { self.all_sound_on } fn read(&self) -> u8 { (if self.all_sound_on { 0x80 } else { 0x00 }) | 0x70 | (if self.ch4_status { 0x08 } else { 0x00 }) | (if self.ch3_status { 0x04 } else { 0x00 }) | (if self.ch2_status { 0x02 } else { 0x00 }) | (if self.ch1_status { 0x01 } else { 0x00 }) } fn write(&mut self, value: u8) { self.all_sound_on = (value >> 7) & 0x01 != 0; } fn update_status_bits(&mut self, ch1_on: bool, ch2_on: bool, ch3_on: bool, ch4_on: bool) { self.ch1_status = ch1_on; self.ch2_status = ch2_on; self.ch3_status = ch3_on; self.ch4_status = ch4_on; } }

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
    pub fn is_ch1_to_so1(&self) -> bool { self.ch1_to_so1 }
    pub fn is_ch2_to_so1(&self) -> bool { self.ch2_to_so1 }
    pub fn is_ch3_to_so1(&self) -> bool { self.ch3_to_so1 }
    pub fn is_ch4_to_so1(&self) -> bool { self.ch4_to_so1 }
    pub fn is_ch1_to_so2(&self) -> bool { self.ch1_to_so2 }
    pub fn is_ch2_to_so2(&self) -> bool { self.ch2_to_so2 }
    pub fn is_ch3_to_so2(&self) -> bool { self.ch3_to_so2 }
    pub fn is_ch4_to_so2(&self) -> bool { self.ch4_to_so2 }

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

#[derive(Debug, Clone, Copy)] pub(super) struct Nr50 { vin_so2_enable: bool, so2_volume: u8, vin_so1_enable: bool, so1_volume: u8 }
impl Nr50 { pub(super) fn new() -> Self { Self { vin_so2_enable: false, so2_volume: 7, vin_so1_enable: false, so1_volume: 7 } } pub(super) fn read(&self) -> u8 { (if self.vin_so2_enable { 0x80 } else { 0x00 }) | (self.so2_volume << 4) | (if self.vin_so1_enable { 0x08 } else { 0x00 }) | self.so1_volume } pub(super) fn write(&mut self, value: u8) { self.so1_volume = value & 0x07; self.vin_so1_enable = (value >> 3) & 0x01 != 0; self.so2_volume = (value >> 4) & 0x07; self.vin_so2_enable = (value >> 7) & 0x01 != 0; } pub(super) fn so1_output_level(&self) -> u8 { self.so1_volume } pub(super) fn so2_output_level(&self) -> u8 { self.so2_volume } }

pub struct Apu {
    pub system_mode: crate::bus::SystemMode,
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
    hpf_capacitor_charge_factor_config: f32,
    lf_div: u8,
    skip_next_frame_sequencer_increment: bool,
    frame_sequencer_clock_is_being_skipped: bool,
    master_t_cycle_count: u64,
}

impl Apu {

    // Port of Sameboy's effective_channel4_counter
    fn calculate_effective_noise_lfsr_for_nr43_write(&self, current_nr43_val: u8) -> u16 {
        let mut effective_lfsr = self.channel4.lfsr;
        let narrow_mode_from_new_nr43 = (current_nr43_val & 8) != 0;

        match self.system_mode {
            SystemMode::DMG | SystemMode::CGB_0 | SystemMode::CGB_A | SystemMode::CGB_B | SystemMode::CGB_C => {
                if (effective_lfsr & 0x8) != 0 { effective_lfsr |= 0xE; }
                if (effective_lfsr & 0x80) != 0 { effective_lfsr |= 0xFF; }
                if narrow_mode_from_new_nr43 {
                    if (effective_lfsr & 0x2) != 0 { effective_lfsr |= 0x3; }
                    if (effective_lfsr & 0x20) != 0 { effective_lfsr |= 0x3F; }
                } else {
                    if (effective_lfsr & 0x4) != 0 { effective_lfsr |= 0x6; }
                    if (effective_lfsr & 0x40) != 0 { effective_lfsr |= 0xCF; }
                }
            }
            SystemMode::CGB_D => {
                if (effective_lfsr & (if narrow_mode_from_new_nr43 { 0x40 } else { 0x80 })) != 0 {
                    effective_lfsr |= 0xFF;
                }
                if (effective_lfsr & 0x100) != 0 { effective_lfsr |= 0x1; }
                if (effective_lfsr & 0x200) != 0 { effective_lfsr |= 0x2; }
                if (effective_lfsr & 0x400) != 0 { effective_lfsr |= 0x4; }
                if (effective_lfsr & 0x800) != 0 { effective_lfsr |= 0x8; }
                if (effective_lfsr & 0x1000) != 0 { effective_lfsr |= 0x10; }
                if (effective_lfsr & 0x2000) != 0 { effective_lfsr |= 0x20; }
                if (effective_lfsr & 0x4000) != 0 { effective_lfsr |= 0x40; }
            }
            SystemMode::AGB => {
                if narrow_mode_from_new_nr43 {
                    if (effective_lfsr & 0x40) != 0 { effective_lfsr |= 0x7F; }
                } else {
                    if (effective_lfsr & 0x4000) != 0 { effective_lfsr |= 0x7FFF; }
                }
            }
            SystemMode::CGB_E => {
                 if narrow_mode_from_new_nr43 {
                    if (effective_lfsr & 0x40) != 0 { effective_lfsr |= 0x7F; }
                } else {
                    if (effective_lfsr & 0x80) != 0 { effective_lfsr |= 0xFF; }
                }
            }
        }
        effective_lfsr
    }

    // Changed to static method, no longer takes &self
    fn static_set_envelope_clock_channel(
        is_new_period_zero: bool,
        is_direction_increase: bool,
        current_volume: u8,
        envelope_clock_active: &mut bool,
        envelope_clock_should_lock: &mut bool,
        envelope_clock_locked: &mut bool,
    ) {
        if !is_new_period_zero {
            *envelope_clock_active = true;
            *envelope_clock_should_lock = (current_volume == 0xF && is_direction_increase) || (current_volume == 0x0 && !is_direction_increase);
        } else {
            *envelope_clock_active = false;
            *envelope_clock_locked |= *envelope_clock_should_lock;
            *envelope_clock_should_lock = false;
        }
    }

    // Changed to static method, no longer takes &self
    fn static_apply_nrx2_glitch_core(
        live_volume: &mut u8,
        new_nrx2_val: u8,
        old_nrx2_val: u8,
        period_timer: &mut u8,
        clock_active: &mut bool,
        clock_should_lock: &mut bool,
        clock_locked: &mut bool,
        system_mode: SystemMode, // SystemMode passed directly
    ) {
        let old_period = old_nrx2_val & 7;
        let new_period = new_nrx2_val & 7;
        let old_direction_increase = (old_nrx2_val & 8) != 0;
        let new_direction_increase = (new_nrx2_val & 8) != 0;

        if old_direction_increase != new_direction_increase {
            *live_volume = 16 - *live_volume;
        }

        if old_period == 0 && *clock_active {
            if matches!(system_mode, SystemMode::DMG | SystemMode::CGB_0) {
                *live_volume = (*live_volume + 2) & 0xF;
            } else {
                *live_volume = (*live_volume + 1) & 0xF;
            }
        } else if !old_direction_increase {
            if matches!(system_mode, SystemMode::DMG | SystemMode::CGB_0) {
            } else {
                *live_volume = (*live_volume + 2) & 0xF;
            }
        }

        if *live_volume > 0xF { *live_volume = 0; }

        // Call the static version
        Apu::static_set_envelope_clock_channel(
            new_period == 0, new_direction_increase, *live_volume,
            clock_active, clock_should_lock, clock_locked
        );

        if *clock_active {
            *period_timer = new_period;
            if *period_timer == 0 { *period_timer = 8; }
        }
    }

    fn apply_nrx2_glitch_to_channel(&mut self, channel_idx: u8, new_nrx2_val: u8, old_nrx2_val: u8) {
        // Copy system_mode to avoid borrowing issues with self later
        let current_system_mode = self.system_mode;

        let (live_volume, period_timer, clock_active, clock_should_lock, clock_locked) = match channel_idx {
            0 => (
                &mut self.channel1.envelope_volume, &mut self.channel1.envelope_period_timer,
                &mut self.channel1.envelope_clock_active, &mut self.channel1.envelope_clock_should_lock,
                &mut self.channel1.envelope_clock_locked,
            ),
            1 => (
                &mut self.channel2.envelope_volume, &mut self.channel2.envelope_period_timer,
                &mut self.channel2.envelope_clock_active, &mut self.channel2.envelope_clock_should_lock,
                &mut self.channel2.envelope_clock_locked,
            ),
            3 => (
                &mut self.channel4.envelope_volume, &mut self.channel4.envelope_period_timer,
                &mut self.channel4.envelope_clock_active, &mut self.channel4.envelope_clock_should_lock,
                &mut self.channel4.envelope_clock_locked,
            ),
            _ => return,
        };

        if matches!(current_system_mode, SystemMode::DMG | SystemMode::CGB_0 | SystemMode::CGB_A | SystemMode::CGB_B | SystemMode::CGB_C) {
            Apu::static_apply_nrx2_glitch_core(live_volume, 0xFF, old_nrx2_val, period_timer, clock_active, clock_should_lock, clock_locked, current_system_mode);
            Apu::static_apply_nrx2_glitch_core(live_volume, new_nrx2_val, 0xFF, period_timer, clock_active, clock_should_lock, clock_locked, current_system_mode);
        } else {
            Apu::static_apply_nrx2_glitch_core(live_volume, new_nrx2_val, old_nrx2_val, period_timer, clock_active, clock_should_lock, clock_locked, current_system_mode);
        }
    }

    pub fn new(system_mode: crate::bus::SystemMode) -> Self {
        let mut apu = Self {
            system_mode,
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
            hpf_capacitor_charge_factor_config: 0.999958_f32,
            lf_div: 0,
            skip_next_frame_sequencer_increment: false,
            frame_sequencer_clock_is_being_skipped: false,
            master_t_cycle_count: 0,
        };
        apu.reset_power_on_channel_flags();
        apu
    }

    fn reset_power_on_channel_flags(&mut self) {
        self.channel1.power_on_reset();
        self.channel2.power_on_reset();
        self.channel4.power_on_reset(); // Added for Channel 4
    }

    fn full_apu_reset_on_power_off(&mut self) {
        self.channel1 = Channel1::new();
        self.channel2 = Channel2::new();
        self.channel3 = Channel3::new();
        self.channel4 = Channel4::new();

        self.nr50.so1_volume = 0; self.nr50.vin_so1_enable = false; self.nr50.so2_volume = 0; self.nr50.vin_so2_enable = false;
        self.nr51.ch1_to_so1 = false; self.nr51.ch2_to_so1 = false; self.nr51.ch3_to_so1 = false; self.nr51.ch4_to_so1 = false;
        self.nr51.ch1_to_so2 = false; self.nr51.ch2_to_so2 = false; self.nr51.ch3_to_so2 = false; self.nr51.ch4_to_so2 = false;

        self.frame_sequencer_step = 0;
        self.frame_sequencer_counter = CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK;
        self.hpf_capacitor_left = 0.0;
        self.hpf_capacitor_right = 0.0;
        self.lf_div = 0;
        self.skip_next_frame_sequencer_increment = false;
        self.frame_sequencer_clock_is_being_skipped = false;
        self.master_t_cycle_count = 0;
        self.nr52.update_status_bits(false,false,false,false);
    }

    #[allow(unused_variables)]
    fn nr10_write_glitch(&mut self, value: u8, is_double_speed: bool) {
        let lf_div_is_odd = (self.lf_div & 1) != 0;
        if self.channel1.sweep_calculate_countdown != 0 || self.channel1.sweep_calculate_countdown_reload_timer != 0 {
            match self.system_mode {
                SystemMode::DMG | SystemMode::CGB_0 | SystemMode::CGB_A | SystemMode::CGB_B | SystemMode::CGB_C => {
                    if self.channel1.sweep_calculate_countdown_reload_timer == 0 {
                        if self.channel1.sweep_calculate_countdown != 0 {
                            self.channel1.sweep_calculate_countdown = 0;
                            if !self.channel1.sweep_instant_calculation_done {
                                self.channel1.sweep_calculation_done(self.system_mode, lf_div_is_odd);
                            }
                        }
                    } else if self.channel1.sweep_calculate_countdown == 0 {
                        self.channel1.sweep_calculate_countdown = self.channel1.sweep_calculate_countdown_reload_timer;
                        if !self.channel1.sweep_instant_calculation_done {
                            self.channel1.sweep_calculation_done(self.system_mode, lf_div_is_odd);
                        }
                    }
                }
                SystemMode::CGB_D | SystemMode::CGB_E | SystemMode::AGB => {
                    if is_double_speed {
                        if self.channel1.sweep_calculate_countdown_reload_timer == 0 {
                            if self.channel1.sweep_calculate_countdown != 0 {
                                self.channel1.sweep_calculate_countdown = 0;
                                if !self.channel1.sweep_instant_calculation_done {
                                    self.channel1.sweep_calculation_done(self.system_mode, lf_div_is_odd);
                                }
                            }
                        }
                        else if self.channel1.sweep_calculate_countdown == 0 {
                             self.channel1.sweep_calculate_countdown = self.channel1.sweep_calculate_countdown_reload_timer;
                             if !self.channel1.sweep_instant_calculation_done {
                                 self.channel1.sweep_calculation_done(self.system_mode, lf_div_is_odd);
                             }
                        }
                    } else {
                        if self.channel1.sweep_calculate_countdown_reload_timer == 0 {
                            if self.channel1.sweep_calculate_countdown != 0 {
                                self.channel1.sweep_calculate_countdown = 0;
                                if !self.channel1.sweep_instant_calculation_done {
                                    self.channel1.sweep_calculation_done(self.system_mode, lf_div_is_odd);
                                }
                            }
                        } else if lf_div_is_odd {
                            if self.channel1.sweep_calculate_countdown != 0 {
                                self.channel1.sweep_calculate_countdown = self.channel1.sweep_calculate_countdown_reload_timer;
                            }
                        } else {
                            if self.channel1.sweep_calculate_countdown == 0 {
                                 self.channel1.sweep_calculate_countdown = self.channel1.sweep_calculate_countdown_reload_timer;
                                 if !self.channel1.sweep_instant_calculation_done {
                                     self.channel1.sweep_calculation_done(self.system_mode, lf_div_is_odd);
                                 }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn tick(&mut self, cpu_t_cycles: u32) {
        self.master_t_cycle_count = self.master_t_cycle_count.wrapping_add(cpu_t_cycles as u64);
        self.lf_div = ((self.master_t_cycle_count / 2) % 2) as u8;

        if self.skip_next_frame_sequencer_increment {
            self.skip_next_frame_sequencer_increment = false;
            self.frame_sequencer_clock_is_being_skipped = true;
        }

        self.frame_sequencer_counter += cpu_t_cycles;
        while self.frame_sequencer_counter >= CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK {
            self.frame_sequencer_counter -= CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK;
            if !self.frame_sequencer_clock_is_being_skipped {
                self.clock_frame_sequencer();
            } else {
                self.frame_sequencer_step = (self.frame_sequencer_step + 1) % 8;
                self.frame_sequencer_clock_is_being_skipped = false;
            }
        }

        if self.nr52.is_apu_enabled() {
            for _ in 0..cpu_t_cycles {
                self.channel1.tick();
                self.channel2.tick();
                self.channel3.tick(&self.wave_ram);
                self.channel4.tick(self.system_mode, self.lf_div, self.frame_sequencer_step);
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
                self.channel1.clock_sweep_fs_tick(self.system_mode, (self.lf_div & 1) != 0);
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
        let mut ch3_s = self.channel3.get_output_sample();
        let ch4_s = self.channel4.get_output_volume();

        if matches!(self.system_mode, SystemMode::AGB) {
            let ch1_bias = if self.channel1.enabled { self.channel1.get_envelope_volume() } else { 0 };
            let ch2_bias = if self.channel2.enabled { self.channel2.get_envelope_volume() } else { 0 };
            let ch3_bias = 0u8;
            let ch4_bias = if self.channel4.enabled { self.channel4.get_envelope_volume() } else { 0 };

            ch3_s ^= 0x0F;

            const CH1_SILENCE: u8 = 0; const CH2_SILENCE: u8 = 0; const CH3_SILENCE: u8 = 7; const CH4_SILENCE: u8 = 0;

            let mut so1_mixed_agb: i16 = 0;
            let mut so2_mixed_agb: i16 = 0;

            let val1_eff_so1 = if self.nr51.is_ch1_to_so1() { ch1_s } else { CH1_SILENCE };
            let val1_eff_so2 = if self.nr51.is_ch1_to_so2() { ch1_s } else { CH1_SILENCE };
            so1_mixed_agb = so1_mixed_agb.wrapping_add( (0x0F_i16).wrapping_sub((val1_eff_so1 as i16).wrapping_mul(2)).wrapping_add(ch1_bias as i16) );
            so2_mixed_agb = so2_mixed_agb.wrapping_add( (0x0F_i16).wrapping_sub((val1_eff_so2 as i16).wrapping_mul(2)).wrapping_add(ch1_bias as i16) );

            let val2_eff_so1 = if self.nr51.is_ch2_to_so1() { ch2_s } else { CH2_SILENCE };
            let val2_eff_so2 = if self.nr51.is_ch2_to_so2() { ch2_s } else { CH2_SILENCE };
            so1_mixed_agb = so1_mixed_agb.wrapping_add( (0x0F_i16).wrapping_sub((val2_eff_so1 as i16).wrapping_mul(2)).wrapping_add(ch2_bias as i16) );
            so2_mixed_agb = so2_mixed_agb.wrapping_add( (0x0F_i16).wrapping_sub((val2_eff_so2 as i16).wrapping_mul(2)).wrapping_add(ch2_bias as i16) );

            let val3_eff_so1 = if self.nr51.is_ch3_to_so1() { ch3_s } else { CH3_SILENCE };
            let val3_eff_so2 = if self.nr51.is_ch3_to_so2() { ch3_s } else { CH3_SILENCE };
            so1_mixed_agb = so1_mixed_agb.wrapping_add( (0x0F_i16).wrapping_sub((val3_eff_so1 as i16).wrapping_mul(2)).wrapping_add(ch3_bias as i16) );
            so2_mixed_agb = so2_mixed_agb.wrapping_add( (0x0F_i16).wrapping_sub((val3_eff_so2 as i16).wrapping_mul(2)).wrapping_add(ch3_bias as i16) );

            let val4_eff_so1 = if self.nr51.is_ch4_to_so1() { ch4_s } else { CH4_SILENCE };
            let val4_eff_so2 = if self.nr51.is_ch4_to_so2() { ch4_s } else { CH4_SILENCE };
            so1_mixed_agb = so1_mixed_agb.wrapping_add( (0x0F_i16).wrapping_sub((val4_eff_so1 as i16).wrapping_mul(2)).wrapping_add(ch4_bias as i16) );
            so2_mixed_agb = so2_mixed_agb.wrapping_add( (0x0F_i16).wrapping_sub((val4_eff_so2 as i16).wrapping_mul(2)).wrapping_add(ch4_bias as i16) );

            let master_so1_vol = (self.nr50.so1_output_level().wrapping_add(1)) as i16;
            let master_so2_vol = (self.nr50.so2_output_level().wrapping_add(1)) as i16;
            let final_so1_agb_i16 = so1_mixed_agb.wrapping_mul(master_so1_vol);
            let final_so2_agb_i16 = so2_mixed_agb.wrapping_mul(master_so2_vol);

            let mut final_so1_f32 = final_so1_agb_i16 as f32 / 480.0;
            let mut final_so2_f32 = final_so2_agb_i16 as f32 / 480.0;

            final_so1_f32 = final_so1_f32.max(-1.0).min(1.0);
            final_so2_f32 = final_so2_f32.max(-1.0).min(1.0);

            let any_dac_on_agb = self.channel1.enabled || self.channel2.enabled || self.channel3.enabled || self.channel4.enabled;
            let hpf_out_so1 = final_so1_f32 - self.hpf_capacitor_left;
            if any_dac_on_agb { self.hpf_capacitor_left = final_so1_f32 - hpf_out_so1 * self.hpf_capacitor_charge_factor_config; } else { self.hpf_capacitor_left = 0.0; }
            let hpf_out_so2 = final_so2_f32 - self.hpf_capacitor_right;
            if any_dac_on_agb { self.hpf_capacitor_right = final_so2_f32 - hpf_out_so2 * self.hpf_capacitor_charge_factor_config; } else { self.hpf_capacitor_right = 0.0; }
            return (hpf_out_so1, hpf_out_so2);

        } else {
            let dac_conv = |val: u8, dac_is_on: bool| if dac_is_on { 1.0 - (val as f32 / 7.5) } else { 0.0 };
            let dac1 = dac_conv(ch1_s, self.channel1.nr12.dac_power());
            let dac2 = dac_conv(ch2_s, self.channel2.nr22.dac_power());
            let dac3 = dac_conv(ch3_s, self.channel3.nr30.dac_on());
            let dac4 = dac_conv(ch4_s, self.channel4.nr42.dac_power());

            let mut so1_mix = 0.0; let mut so2_mix = 0.0;
            if self.nr51.is_ch1_to_so1() { so1_mix += dac1; } if self.nr51.is_ch1_to_so2() { so2_mix += dac1; }
            if self.nr51.is_ch2_to_so1() { so1_mix += dac2; } if self.nr51.is_ch2_to_so2() { so2_mix += dac2; }
            if self.nr51.is_ch3_to_so1() { so1_mix += dac3; } if self.nr51.is_ch3_to_so2() { so2_mix += dac3; }
            if self.nr51.is_ch4_to_so1() { so1_mix += dac4; } if self.nr51.is_ch4_to_so2() { so2_mix += dac4; }

            let vol_factor = |vol: u8| (vol.wrapping_add(1)) as f32 / 8.0;
            let mut final_so1 = so1_mix * vol_factor(self.nr50.so1_output_level());
            let mut final_so2 = so2_mix * vol_factor(self.nr50.so2_output_level());

            final_so1 /= 4.0; final_so2 /= 4.0;

            let any_dac_on = self.channel1.nr12.dac_power() || self.channel2.nr22.dac_power() ||
                             self.channel3.nr30.dac_on() || self.channel4.nr42.dac_power();

            let hpf_out_so1 = final_so1 - self.hpf_capacitor_left;
            if any_dac_on { self.hpf_capacitor_left = final_so1 - hpf_out_so1 * self.hpf_capacitor_charge_factor_config; }
            else { self.hpf_capacitor_left = 0.0; }

            let hpf_out_so2 = final_so2 - self.hpf_capacitor_right;
            if any_dac_on { self.hpf_capacitor_right = final_so2 - hpf_out_so2 * self.hpf_capacitor_charge_factor_config; }
            else { self.hpf_capacitor_right = 0.0; }

            (hpf_out_so1, hpf_out_so2)
        }
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
                    let is_cgb = !matches!(self.system_mode, SystemMode::DMG);
                    if !is_cgb && !self.channel3.wave_form_just_read_get() { return 0xFF; }
                    if matches!(self.system_mode, SystemMode::AGB) { return 0xFF; }
                    if matches!(self.system_mode, SystemMode::CGB_E) { return 0xFF; }

                    let read_idx = self.channel3.current_wave_ram_byte_index();
                    if read_idx < self.wave_ram.len() { return self.wave_ram[read_idx]; } else { return 0xFF; }
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

    pub fn write_byte(&mut self, addr: u16, value: u8, is_double_speed: bool, div_counter_for_nr52: u16) {
        let apu_was_enabled = self.nr52.is_apu_enabled();
        if !apu_was_enabled && addr != NR52_ADDR { return; }

        let trigger_val_in_write = (value >> 7) & 0x01 != 0;

        match addr {
            NR10_ADDR => {
                let prev_sweep_direction_is_subtraction = !self.channel1.nr10.sweep_direction_is_increase();
                let sweep_period_was_zero = self.channel1.nr10.sweep_period() == 0;

                if self.channel1.sweep_calculate_countdown != 0 || self.channel1.sweep_calculate_countdown_reload_timer != 0 {
                    self.nr10_write_glitch(value, is_double_speed);
                }

                self.channel1.nr10.write(value);

                let mut old_negate_check_value: u16 = 0;
                if prev_sweep_direction_is_subtraction {
                    if matches!(self.system_mode, SystemMode::DMG | SystemMode::CGB_0 | SystemMode::CGB_A | SystemMode::CGB_B | SystemMode::CGB_C) {
                        old_negate_check_value = 1;
                    } else {
                        old_negate_check_value = 1;
                    }
                } else {
                     if matches!(self.system_mode, SystemMode::DMG | SystemMode::CGB_0 | SystemMode::CGB_A | SystemMode::CGB_B | SystemMode::CGB_C) {
                        old_negate_check_value = 1;
                    } else {
                        old_negate_check_value = 0;
                    }
                }

                if !sweep_period_was_zero && self.channel1.nr10.sweep_period() == 0 {
                    self.channel1.enabled = false;
                }

                if self.channel1.sweep_shadow_frequency
                    .saturating_add(self.channel1.channel1_completed_addend)
                    .saturating_add(old_negate_check_value) > 0x7FF &&
                   self.channel1.nr10.sweep_direction_is_increase() {
                    self.channel1.enabled = false;
                    self.channel1.force_output_zero_for_next_sample = true;
                }

                self.channel1.trigger_sweep_calculation(self.system_mode, (self.lf_div & 1) != 0);
            },
            NR11_ADDR => {
                self.channel1.nr11.write(value);
            },
            NR12_ADDR => {
                let old_nr12_val = self.channel1.nr12.read();
                self.channel1.nr12.write(value);
                self.apply_nrx2_glitch_to_channel(0, value, old_nr12_val);
                if !self.channel1.nr12.dac_power() { self.channel1.force_disable_channel(); }
            },
            NR13_ADDR => self.channel1.nr13.write(value),
            NR14_ADDR => {
                let prev_len_enabled = self.channel1.length_enabled_internal;
                self.channel1.nr14.write(value);
                let new_length_enable_bit = (value >> 6) & 1 != 0;

                let div_divider_lsb_is_1 = (self.frame_sequencer_step % 2) != 0;
                let model_allows_glitch = !matches!(self.system_mode, SystemMode::CGB_D | SystemMode::CGB_E | SystemMode::AGB) || new_length_enable_bit;

                if model_allows_glitch && !prev_len_enabled && new_length_enable_bit &&
                   div_divider_lsb_is_1 && self.channel1.get_length_counter() > 0 {
                    self.channel1.extra_length_clock(trigger_val_in_write);
                }
                self.channel1.length_enabled_internal = new_length_enable_bit;

                if trigger_val_in_write {
                    // TODO: Update channel1.trigger signature if it needs system_mode/lf_div for sweep_restart_hold
                    self.channel1.trigger(self.frame_sequencer_step);
                }
            },
            NR21_ADDR => {
                self.channel2.nr21.write(value);
            },
            NR22_ADDR => {
                let old_nr22_val = self.channel2.nr22.read();
                self.channel2.nr22.write(value);
                self.apply_nrx2_glitch_to_channel(1, value, old_nr22_val);
                if !self.channel2.nr22.dac_power() { self.channel2.force_disable_channel(); }
            },
            NR23_ADDR => self.channel2.nr23.write(value),
            NR24_ADDR => {
                let prev_len_enabled = self.channel2.length_enabled_internal;
                self.channel2.nr24.write(value);
                let new_length_enable_bit = (value >> 6) & 1 != 0;

                let div_divider_lsb_is_1 = (self.frame_sequencer_step % 2) != 0;
                let model_allows_glitch = !matches!(self.system_mode, SystemMode::CGB_D | SystemMode::CGB_E | SystemMode::AGB) || new_length_enable_bit;

                if model_allows_glitch && !prev_len_enabled && new_length_enable_bit &&
                   div_divider_lsb_is_1 && self.channel2.get_length_counter() > 0 {
                    self.channel2.extra_length_clock(trigger_val_in_write);
                }
                self.channel2.length_enabled_internal = new_length_enable_bit;

                if trigger_val_in_write {
                    self.channel2.trigger(self.frame_sequencer_step);
                }
            },
            NR30_ADDR => self.channel3.nr30.write(value),
            NR31_ADDR => {
                self.channel3.nr31.write(value);
                if self.channel3.enabled && self.channel3.nr34.is_length_enabled() {
                    self.channel3.reload_length_on_enable(self.frame_sequencer_step);
                }
            },
            NR32_ADDR => self.channel3.nr32.write(value),
            NR33_ADDR => self.channel3.nr33.write(value),
            NR34_ADDR => {
                let ch3_active_for_corruption = self.channel3.enabled && self.channel3.nr30.dac_on();

                if trigger_val_in_write && ch3_active_for_corruption &&
                   self.channel3.sample_countdown() == 0 &&
                   !matches!(self.system_mode, SystemMode::CGB_0 | SystemMode::CGB_A | SystemMode::CGB_B | SystemMode::CGB_C | SystemMode::CGB_D | SystemMode::CGB_E | SystemMode::AGB) {
                    let offset = ((self.channel3.current_sample_index() + 1) >> 1) & 0xF;
                    if offset < 4 {
                        let first_byte = self.wave_ram[0];
                        for i in 0..4 {
                            if i < self.wave_ram.len() { self.wave_ram[i] = first_byte; }
                        }
                    } else {
                        for i in 0..4 {
                            if (offset as usize + i) < self.wave_ram.len() && i < self.wave_ram.len() {
                                self.wave_ram[offset as usize + i] = self.wave_ram[i];
                            }
                        }
                    }
                }

                let prev_len_enabled = self.channel3.length_enabled_internal;
                self.channel3.nr34.write(value);
                let new_length_enable_bit = (value >> 6) & 1 != 0;

                let div_divider_lsb_is_1 = (self.frame_sequencer_step % 2) != 0;
                let model_allows_glitch = !matches!(self.system_mode, SystemMode::CGB_D | SystemMode::CGB_E | SystemMode::AGB) || new_length_enable_bit;

                if model_allows_glitch && !prev_len_enabled && new_length_enable_bit &&
                   div_divider_lsb_is_1 && self.channel3.get_length_counter() > 0 {
                    self.channel3.extra_length_clock(trigger_val_in_write);
                }
                self.channel3.length_enabled_internal = new_length_enable_bit;

                if trigger_val_in_write {
                    self.channel3.trigger(&self.wave_ram, self.frame_sequencer_step, self.system_mode, self.lf_div);
                }
            },
            NR41_ADDR => {
                self.channel4.nr41.write(value);
            },
            NR42_ADDR => {
                let old_nr42_val = self.channel4.nr42.read();
                self.channel4.nr42.write(value);
                self.apply_nrx2_glitch_to_channel(3, value, old_nr42_val);
                if !self.channel4.nr42.dac_power() { self.channel4.force_disable_channel(); }
            },
            NR43_ADDR => {
                let old_nr43_val = self.channel4.nr43.read();
                self.channel4.nr43.write(value);
                self.channel4.lfsr_narrow_mode = self.channel4.nr43.lfsr_width_is_7bit();

                let effective_lfsr = self.calculate_effective_noise_lfsr_for_nr43_write(value);

                let old_shift_amount = old_nr43_val >> 4;
                let new_shift_amount = value >> 4;
                let old_poly_bit = (effective_lfsr >> old_shift_amount) & 1;
                let new_poly_bit = (effective_lfsr >> new_shift_amount) & 1;

                if new_poly_bit == 1 {
                    if !matches!(self.system_mode, SystemMode::CGB_D | SystemMode::CGB_E | SystemMode::AGB) {
                        let previous_narrow_mode_state = self.channel4.lfsr_narrow_mode;
                        self.channel4.lfsr_narrow_mode = true;
                        self.channel4.step_lfsr();
                        self.channel4.lfsr_narrow_mode = previous_narrow_mode_state;
                        self.channel4.step_lfsr();
                    } else {
                        if old_poly_bit == 0 {
                            self.channel4.step_lfsr();
                        }
                    }
                }

                if self.channel4.countdown_was_reloaded_by_tick {
                    let r = self.channel4.nr43.clock_divider_val();
                    let s = self.channel4.nr43.clock_shift();
                    let divisor_val: u16 = if r == 0 { 8 } else { (r as u16) * 16 };
                    self.channel4.sample_countdown = divisor_val << s;
                    self.channel4.countdown_was_reloaded_by_tick = false;
                }
            },
            NR44_ADDR => {
                let prev_length_enabled_internal = self.channel4.length_enabled_internal;
                self.channel4.nr44.write(value);

                let new_length_enable_bit = (value >> 6) & 1 != 0;

                let div_divider_lsb_is_1 = (self.frame_sequencer_step % 2) != 0;

                let model_allows_glitch_on_write = !matches!(self.system_mode, SystemMode::CGB_D | SystemMode::CGB_E | SystemMode::AGB) || new_length_enable_bit;

                if model_allows_glitch_on_write && !prev_length_enabled_internal && new_length_enable_bit &&
                   div_divider_lsb_is_1 && self.channel4.get_length_counter() > 0 {
                    self.channel4.extra_length_clock(trigger_val_in_write);
                }
                self.channel4.length_enabled_internal = new_length_enable_bit;

                if trigger_val_in_write {
                    self.channel4.trigger(self.system_mode, self.lf_div, self.frame_sequencer_step);
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
                } else if !prev_power_state && new_power_state {
                    self.frame_sequencer_step = 0;
                    self.frame_sequencer_counter = CPU_CLOCKS_PER_FRAME_SEQUENCER_TICK;
                    self.reset_power_on_channel_flags();

                    let relevant_div_bit = if is_double_speed { 0x2000 } else { 0x1000 };
                    if (div_counter_for_nr52 & relevant_div_bit) != 0 {
                        self.skip_next_frame_sequencer_increment = true;
                        self.frame_sequencer_step = 1;
                    }
                }
            }
            WAVE_PATTERN_RAM_START_ADDR..=WAVE_PATTERN_RAM_END_ADDR => {
                if self.channel3.enabled {
                    let is_cgb = !matches!(self.system_mode, SystemMode::DMG);
                    if (!is_cgb && !self.channel3.wave_form_just_read_get()) ||
                       matches!(self.system_mode, SystemMode::AGB | SystemMode::CGB_E) {
                        return;
                    }
                    let write_idx = self.channel3.current_wave_ram_byte_index();
                    if write_idx < self.wave_ram.len() { self.wave_ram[write_idx] = value; }
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
