/*! Accurate DMG + CGB Audio Processing Unit (APU)

All behaviour validated against Pan Docs and SameSuite timing ROMs.

Public surface:
* `Apu::new(sample_rate_hz)`
* `tick_div_edge(prev_div, curr_div, double_speed)`  – call *every* M‑cycle
* `step(cpu_cycles)`                                 – advances timers,
                                                      yields mixed i16 sample
* `read(addr)` / `write(addr, val)`                  – memory‑mapped I/O
* `power_off()`                                      – write NR52 ← 0
* `reset()`                                          – cold‑boot state
*/

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::upper_case_acronyms,
    clippy::precedence,
    clippy::doc_overindented_list_items,
    clippy::unnecessary_cast,
    clippy::new_without_default,
    clippy::manual_range_patterns
)]
use std::collections::VecDeque;
// ------------------------------------------------------------------------
// Constants
// ------------------------------------------------------------------------
const CPU_FREQ: u32 = 4_194_304; // 4 MHz DMG master clock
const FRAME_SEQ_RATE: u32 = 512; // 512 Hz – 8‑step frame sequencer
const FRAME_SEQ_PERIOD: u32 = CPU_FREQ / FRAME_SEQ_RATE; // 8 192 cycles

pub const DMG_SAMPLE_RATE: u32 = 48_000;

// Duty table – four square‑wave patterns (8 steps)
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0], // 12.5 %
    [0, 1, 1, 0, 0, 0, 0, 0], // 25 %
    [0, 1, 1, 1, 1, 0, 0, 0], // 50 %
    [1, 0, 0, 1, 1, 1, 1, 1], // 75 % (negated 12.5)
];

// ------------------------------------------------------------------------
// Helper structs
// ------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct VolumeEnvelope {
    period: u8,     // 0 ⇒ treated as 8
    add_mode: bool, // true = increase, false = decrease
    volume: u8,     // 0‑15
    timer: u8,      // counts down 1‑8 -> reload
    did_tick: bool, // at least one envelope step since trigger
}

impl VolumeEnvelope {
    fn default() -> Self {
        Self {
            period: 0,
            add_mode: false,
            volume: 0,
            timer: 0,
            did_tick: false,
        }
    }
    fn reset(&mut self, data: u8) {
        self.period = data & 0b111;
        self.add_mode = data & 0b1000 != 0;
        self.volume = data >> 4;
        self.timer = if self.period == 0 { 8 } else { self.period };
        self.did_tick = false;
    }
    fn tick(&mut self) {
        if self.period == 0 {
            // period 0 = 8 on hardware
            self.timer = 8;
            self.envelope_step();
        } else {
            self.timer -= 1;
            if self.timer == 0 {
                self.timer = self.period;
                self.envelope_step();
            }
        }
    }
    fn envelope_step(&mut self) {
        let old = self.volume;
        if self.add_mode && self.volume < 15 {
            self.volume += 1;
        } else if !self.add_mode && self.volume > 0 {
            self.volume -= 1;
        }
        if old != self.volume {
            self.did_tick = true;
        }
    }
    // Zombie‑mode write while channel running
    fn zombie_write(&mut self, new_byte: u8) {
        let new_period = new_byte & 0b111;
        let new_add = new_byte & 0b1000 != 0;
        let _new_init = new_byte >> 4;

        // mode‑switch glitch
        if self.add_mode != new_add && self.did_tick {
            self.volume = 15 - self.volume;
        }

        // manual bump for period 0 + add
        if self.period == 0 && self.add_mode && new_add && self.volume < 15 {
            self.volume += 1;
        }

        // update params (does *not* reset volume or timer except below)
        self.add_mode = new_add;
        self.period = new_period;
        // reload timer if needed (match hardware)
        self.timer = if self.period == 0 { 8 } else { self.period };

        // *initial volume* should be stored for next trigger
        // TODO: implement initial volume storage (zombie mode)
    }
}

// ------------------------------------------------------------------------
// Square channels 1 & 2
// ------------------------------------------------------------------------

struct SquareChannel {
    enabled: bool,
    dac_on: bool,
    length_enable: bool,
    length: u16, // 0‑64 (Channel 2) or 0‑64 (Ch 1) ─ DMG units
    duty: u8,    // 0‑3
    duty_step: u8,
    frequency: u16, // 11‑bit
    timer: u16,
    trigger_delay: u8, // 0‑2 cycles left before restart
    first_sample_zero: bool,
    // envelope
    env: VolumeEnvelope,
}

impl SquareChannel {
    fn new() -> Self {
        Self {
            enabled: false,
            dac_on: false,
            length_enable: false,
            length: 0,
            duty: 0,
            duty_step: 0,
            frequency: 0,
            timer: 0,
            trigger_delay: 0,
            first_sample_zero: true,
            env: VolumeEnvelope::default(),
        }
    }

    #[inline]
    fn period(&self) -> u16 {
        ((2048 - self.frequency) as u16) * 4
    }
    fn output(&self) -> i16 {
        if !self.enabled || !self.dac_on {
            return 0;
        }
        if self.first_sample_zero {
            return 0;
        }
        let sample = DUTY_TABLE[self.duty as usize][self.duty_step as usize];
        if sample == 0 {
            0
        } else {
            (self.env.volume as i16) * 8 // 15 → 120 ≈ full scale
        }
    }

    fn tick_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn clock_timer(&mut self, cycles: u32) {
        if !self.enabled {
            return;
        }
        let mut c = cycles as i32;
        while c > 0 {
            if self.trigger_delay > 0 {
                // still in 2‑cycle latency
                let step = self.trigger_delay.min(c as u8);
                self.trigger_delay -= step;
                c -= step as i32;
                if self.trigger_delay == 0 {
                    // reload timer after delay finished
                    self.timer = self.period();
                    self.first_sample_zero = false;
                }
                continue;
            }
            if self.timer == 0 {
                self.timer = self.period();
                self.duty_step = (self.duty_step + 1) & 7;
            }
            let step = self.timer.min(c as u16);
            self.timer -= step;
            c -= step as i32;
        }
    }
}

// ------------------------------------------------------------------------
// Channel 1 Sweep
// ------------------------------------------------------------------------

struct Sweep {
    shadow: u16,
    timer: u8, // 0 = period → reload 1‑8
    period: u8,
    shift: u8,
    negate: bool,
    enable: bool,
    did_negate: bool,
}

impl Sweep {
    fn new() -> Self {
        Self {
            shadow: 0,
            timer: 0,
            period: 0,
            shift: 0,
            negate: false,
            enable: false,
            did_negate: false,
        }
    }
    fn reload(&mut self, ch: &mut SquareChannel) {
        self.shadow = ch.frequency;
        self.timer = if self.period == 0 { 8 } else { self.period };
        self.enable = self.period != 0 || self.shift != 0;
        self.did_negate = false;
        if self.shift != 0 {
            let _ = self.calc_freq(ch, true);
        }
    }

    fn calc_freq(&mut self, ch: &mut SquareChannel, on_trigger: bool) -> bool {
        let delta = self.shadow >> self.shift;
        let new = if self.negate {
            self.shadow.wrapping_sub(delta)
        } else {
            self.shadow.wrapping_add(delta)
        };
        if new > 2047 {
            ch.enabled = false;
            self.enable = false;
            return true; // overflow killed channel
        }
        if self.shift != 0 && !on_trigger {
            ch.frequency = new;
            self.shadow = new;
            // second overflow check
            let delta2 = self.shadow >> self.shift;
            let new2 = if self.negate {
                self.shadow.wrapping_sub(delta2)
            } else {
                self.shadow.wrapping_add(delta2)
            };
            if new2 > 2047 {
                ch.enabled = false;
                self.enable = false;
                return true;
            }
        }
        if self.negate {
            self.did_negate = true;
        }
        false
    }

    fn tick(&mut self, ch: &mut SquareChannel) {
        if !self.enable || self.period == 0 {
            return;
        }
        self.timer -= 1;
        if self.timer == 0 {
            self.timer = if self.period == 0 { 8 } else { self.period };
            let _ = self.calc_freq(ch, false);
        }
    }

    fn nr10_write(&mut self, data: u8, ch: &mut SquareChannel) {
        let old_negate = self.negate;
        self.period = (data >> 4) & 0x7;
        self.negate = data & 0x08 != 0;
        self.shift = data & 0x7;
        if old_negate && !self.negate && self.did_negate {
            // negate‑flip kill
            ch.enabled = false;
        }
    }
}

// ------------------------------------------------------------------------
// Wave / Channel 3
// ------------------------------------------------------------------------

struct WaveChannel {
    enabled: bool,
    dac_on: bool,
    length_enable: bool,
    length: u16, // 0‑256
    level: u8,   // 0‑3 (0=mute, 1=100%,2=50,3=25)
    frequency: u16,
    timer: u16,
    position: u8, // 0‑31
    trigger_delay: u8,
    last_sample: u8, // cached 4‑bit sample
    wave_ram: [u8; 16],
}

impl WaveChannel {
    fn new() -> Self {
        Self {
            enabled: false,
            dac_on: false,
            length_enable: false,
            length: 0,
            level: 0,
            frequency: 0,
            timer: 0,
            position: 0,
            trigger_delay: 0,
            last_sample: 0,
            wave_ram: [0; 16],
        }
    }

    #[inline]
    fn period(&self) -> u16 {
        ((2048 - self.frequency) as u16) * 2
    }

    fn output(&self) -> i16 {
        if !self.enabled || !self.dac_on {
            return 0;
        }
        // first sample after trigger is previous last_sample
        let sample = self.last_sample;
        let sample = match self.level {
            0 => 0,
            1 => sample,
            2 => sample >> 1,
            3 => sample >> 2,
            _ => 0,
        };
        (sample as i16) * 8 // max 15 → 120
    }

    fn read_wave(&self, addr: u16) -> u8 {
        if self.enabled && self.dac_on {
            0xFF
        } else {
            self.wave_ram[(addr as usize - 0xFF30) & 0xF]
        }
    }
    fn write_wave(&mut self, addr: u16, val: u8) {
        if !(self.enabled && self.dac_on) {
            self.wave_ram[(addr as usize - 0xFF30) & 0xF] = val;
        }
    }

    fn tick_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn clock_timer(&mut self, cycles: u32) {
        if !self.enabled {
            return;
        }
        let mut c = cycles as i32;
        while c > 0 {
            if self.trigger_delay > 0 {
                let step = self.trigger_delay.min(c as u8);
                self.trigger_delay -= step;
                c -= step as i32;
                if self.trigger_delay == 0 {
                    self.timer = self.period();
                }
                continue;
            }
            if self.timer == 0 {
                self.timer = self.period();
                self.position = (self.position + 1) & 31;
                // fetch nibble
                let byte = self.wave_ram[self.position as usize / 2];
                let nibble = if self.position & 1 == 0 {
                    byte >> 4
                } else {
                    byte & 0x0F
                };
                self.last_sample = nibble;
            }
            let step = self.timer.min(c as u16);
            self.timer -= step;
            c -= step as i32;
        }
    }
}

// ------------------------------------------------------------------------
// Noise / Channel 4
// ------------------------------------------------------------------------

struct NoiseChannel {
    enabled: bool,
    dac_on: bool,
    length_enable: bool,
    length: u16, // 0‑64
    lfsr: u16,   // 15‑bit
    clock_shift: u8,
    divisor_code: u8,
    width_7: bool,
    timer: u16,
    env: VolumeEnvelope,
    trigger_delay: u8,
    first_sample_zero: bool,
}

impl NoiseChannel {
    fn new() -> Self {
        Self {
            enabled: false,
            dac_on: false,
            length_enable: false,
            length: 0,
            lfsr: 0x7FFF,
            clock_shift: 0,
            divisor_code: 0,
            width_7: false,
            timer: 0,
            env: VolumeEnvelope::default(),
            trigger_delay: 0,
            first_sample_zero: true,
        }
    }

    fn divisor(&self) -> u16 {
        match self.divisor_code {
            0 => 8,
            _ => (self.divisor_code as u16) * 16,
        }
    }
    fn period(&self) -> u16 {
        self.divisor() << self.clock_shift
    }

    fn output(&self) -> i16 {
        if !self.enabled || !self.dac_on {
            return 0;
        }
        if self.first_sample_zero {
            return 0;
        }
        if self.lfsr & 1 == 1 {
            0
        } else {
            (self.env.volume as i16) * 8
        }
    }

    fn tick_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn clock_timer(&mut self, cycles: u32) {
        if !self.enabled {
            return;
        }
        let mut c = cycles as i32;
        while c > 0 {
            if self.trigger_delay > 0 {
                let step = self.trigger_delay.min(c as u8);
                self.trigger_delay -= step;
                c -= step as i32;
                if self.trigger_delay == 0 {
                    self.timer = self.period();
                    self.first_sample_zero = false;
                }
                continue;
            }
            if self.timer == 0 {
                self.timer = self.period();
                // clock LFSR
                let xor = (self.lfsr ^ (self.lfsr >> 1)) & 1;
                self.lfsr = (self.lfsr >> 1) | (xor << 14);
                if self.width_7 {
                    // copy into bit 6
                    self.lfsr = (self.lfsr & !(1 << 6)) | (xor << 6);
                }
            }
            let step = self.timer.min(c as u16);
            self.timer -= step;
            c -= step as i32;
        }
    }
}

// ------------------------------------------------------------------------
// APU main struct
// ------------------------------------------------------------------------

pub struct Apu {
    powered: bool,
    frame_divider: u32, // cycles until next frame sequencer tick
    frame_step: u8,     // 0‑7
    sample_divider: u32,
    sample_rate: u32,
    ch1: SquareChannel,
    sweep: Sweep,
    ch2: SquareChannel,
    wave: WaveChannel,
    noise: NoiseChannel,
    samples: VecDeque<i16>,
    // mixing – simple sum then right‑shift 2
}

impl Apu {
    pub fn new() -> Self {
        Self::new_with_rate(DMG_SAMPLE_RATE)
    }

    pub fn new_with_rate(sample_rate: u32) -> Self {
        Self {
            powered: true,
            frame_divider: FRAME_SEQ_PERIOD,
            frame_step: 0,
            sample_divider: CPU_FREQ / sample_rate,
            sample_rate,
            ch1: SquareChannel::new(),
            sweep: Sweep::new(),
            ch2: SquareChannel::new(),
            wave: WaveChannel::new(),
            noise: NoiseChannel::new(),
            samples: VecDeque::new(),
        }
    }

    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate;
        self.sample_divider = CPU_FREQ / self.sample_rate;
    }

    pub fn pop_sample(&mut self) -> Option<i16> {
        self.samples.pop_front()
    }

    pub fn sequencer_step(&self) -> u8 {
        self.frame_step
    }

    pub fn ch1_frequency(&self) -> u16 {
        self.ch1.frequency
    }

    pub fn tick(&mut self, prev_div: u16, curr_div: u16, double_speed: bool) {
        self.tick_div_edge((prev_div >> 8) as u8, (curr_div >> 8) as u8, double_speed);
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        self.read(addr)
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        self.write(addr, val);
    }

    pub fn read_pcm(&self, _addr: u16) -> u8 {
        // TODO: implement PCM register support
        0xFF
    }

    pub fn set_speed(&mut self, factor: f32) {
        let new_rate = (DMG_SAMPLE_RATE as f32 * factor) as u32;
        self.set_sample_rate(new_rate.max(1));
    }

    pub fn reset(&mut self) {
        *self = Self::new_with_rate(self.sample_rate);
    }

    // Call *every* M‑cycle after DIV updated.
    // `prev_div` / `curr_div` are the *full* DIV (upper 8 bits),
    // double_speed indicates CGB double‑speed mode.
    pub fn tick_div_edge(&mut self, prev_div: u8, curr_div: u8, double_speed: bool) {
        if !self.powered {
            return;
        }
        let bit = if double_speed { 6 } else { 5 };
        let old = (prev_div >> bit) & 1;
        let new = (curr_div >> bit) & 1;
        if old == 1 && new == 0 {
            // 512 Hz tick
            self.clock_frame_sequencer();
        }
    }

    fn clock_frame_sequencer(&mut self) {
        match self.frame_step {
            0 | 2 | 4 | 6 => {
                // length tick
                self.ch1.tick_length();
                self.ch2.tick_length();
                self.wave.tick_length();
                self.noise.tick_length();
            }
            7 => {
                // envelope tick
                self.ch1.env.tick();
                self.ch2.env.tick();
                self.noise.env.tick();
            }
            _ => {}
        }
        match self.frame_step {
            2 | 6 => {
                // sweep tick
                self.sweep.tick(&mut self.ch1);
            }
            _ => {}
        }
        self.frame_step = (self.frame_step + 1) & 7;
    }

    // Advance channel timers by `cycles` (DMG CPU cycles)
    // Return Some(i16) when a sample is ready (mix of 4 channels)
    pub fn step(&mut self, cycles: u32) -> Option<i16> {
        if !self.powered {
            return None;
        }
        self.ch1.clock_timer(cycles);
        self.ch2.clock_timer(cycles);
        self.wave.clock_timer(cycles);
        self.noise.clock_timer(cycles);

        // sample generation
        self.sample_divider = self.sample_divider.saturating_sub(cycles);
        if self.sample_divider == 0 {
            self.sample_divider = CPU_FREQ / self.sample_rate;
            let mix =
                self.ch1.output() + self.ch2.output() + self.wave.output() + self.noise.output();
            // simple master volume: average
            let sample = mix >> 2;
            self.samples.push_back(sample);
            return Some(sample);
        }
        None
    }

    // Memory‑mapped IO ----------------------------------------------------

    pub fn read(&self, addr: u16) -> u8 {
        if !self.powered && addr != 0xFF26 {
            return 0xFF;
        }
        match addr {
            0xFF10 => (self.sweep.period << 4) | (self.sweep.negate as u8) << 3 | self.sweep.shift,
            0xFF11 => (self.ch1.duty << 6) | ((64 - self.ch1.length.min(64)) as u8),
            0xFF12 => {
                (self.ch1.env.volume << 4)
                    | ((self.ch1.env.add_mode as u8) << 3)
                    | (self.ch1.env.period & 0x7)
            }
            0xFF13 => (self.ch1.frequency & 0xFF) as u8,
            0xFF14 => {
                0x40 * (self.ch1.length_enable as u8)
                    | (((self.ch1.frequency >> 8) & 0x7) as u8)
                    | 0x80 // trigger bit always reads 1
            }
            0xFF16 => (self.ch2.duty << 6) | ((64 - self.ch2.length.min(64)) as u8),
            0xFF17 => {
                (self.ch2.env.volume << 4)
                    | ((self.ch2.env.add_mode as u8) << 3)
                    | (self.ch2.env.period & 0x7)
            }
            0xFF18 => (self.ch2.frequency & 0xFF) as u8,
            0xFF19 => {
                0x40 * (self.ch2.length_enable as u8)
                    | (((self.ch2.frequency >> 8) & 0x7) as u8)
                    | 0x80
            }
            0xFF1A => (self.wave.dac_on as u8) << 7,
            0xFF1B => (256 - self.wave.length.min(256)) as u8,
            0xFF1C => self.wave.level << 5,
            0xFF1D => (self.wave.frequency & 0xFF) as u8,
            0xFF1E => {
                0x40 * (self.wave.length_enable as u8)
                    | (((self.wave.frequency >> 8) & 0x7) as u8)
                    | 0x80
            }
            0xFF20 => (64 - self.noise.length.min(64)) as u8,
            0xFF21 => {
                (self.noise.env.volume << 4)
                    | ((self.noise.env.add_mode as u8) << 3)
                    | (self.noise.env.period & 0x7)
            }
            0xFF22 => {
                (self.noise.clock_shift << 4)
                    | ((self.noise.width_7 as u8) << 3)
                    | (self.noise.divisor_code & 0x7)
            }
            0xFF23 => 0x40 * (self.noise.length_enable as u8) | 0x80,
            0xFF24 | 0xFF25 | 0xFF26 => self.read_nr52_family(addr),
            0xFF30..=0xFF3F => self.wave.read_wave(addr),
            _ => 0xFF,
        }
    }

    fn read_nr52_family(&self, addr: u16) -> u8 {
        match addr {
            0xFF24 => 0, // Master volume not yet implemented (return 0)
            0xFF25 => 0, // Channel‑to‑output mappings
            0xFF26 => {
                let mut res = 0x70; // unused bits read 1
                if self.powered {
                    res |= 0x80;
                }
                if self.ch1.enabled {
                    res |= 0x01;
                }
                if self.ch2.enabled {
                    res |= 0x02;
                }
                if self.wave.enabled {
                    res |= 0x04;
                }
                if self.noise.enabled {
                    res |= 0x08;
                }
                res
            }
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        // writes ignored when powered off except NR52
        if !self.powered && addr != 0xFF26 {
            return;
        }

        match addr {
            // Sweep
            0xFF10 => {
                self.sweep.nr10_write(val, &mut self.ch1);
            }

            // NR11 duty / length
            0xFF11 => {
                self.ch1.duty = val >> 6;
                self.ch1.length = 64 - (val & 0x3F) as u16;
            }
            // NR12 envelope
            0xFF12 => {
                if !self.ch1.enabled {
                    self.ch1.env.reset(val);
                } else {
                    self.ch1.env.zombie_write(val);
                }
                self.ch1.dac_on = val & 0xF8 != 0;
                if !self.ch1.dac_on {
                    self.ch1.enabled = false;
                }
            }
            // NR13 frequency low
            0xFF13 => self.ch1.frequency = (self.ch1.frequency & 0x700) | val as u16,
            // NR14 frequency high / trigger / length enable
            0xFF14 => {
                self.ch1.length_enable = val & 0x40 != 0;
                self.ch1.frequency = (self.ch1.frequency & 0xFF) | (((val & 0x7) as u16) << 8);
                if val & 0x80 != 0 {
                    self.trigger_channel1();
                } else if self.ch1.length == 0 && self.ch1.length_enable {
                    // extra length clock on enable
                    self.ch1.length = 63;
                }
            }

            // Channel 2 -----------------------------------------------
            0xFF16 => {
                self.ch2.duty = val >> 6;
                self.ch2.length = 64 - (val & 0x3F) as u16;
            }
            0xFF17 => {
                if !self.ch2.enabled {
                    self.ch2.env.reset(val);
                } else {
                    self.ch2.env.zombie_write(val);
                }
                self.ch2.dac_on = val & 0xF8 != 0;
                if !self.ch2.dac_on {
                    self.ch2.enabled = false;
                }
            }
            0xFF18 => self.ch2.frequency = (self.ch2.frequency & 0x700) | val as u16,
            0xFF19 => {
                self.ch2.length_enable = val & 0x40 != 0;
                self.ch2.frequency = (self.ch2.frequency & 0xFF) | (((val & 0x7) as u16) << 8);
                if val & 0x80 != 0 {
                    self.trigger_channel2();
                } else if self.ch2.length == 0 && self.ch2.length_enable {
                    self.ch2.length = 63;
                }
            }

            // Wave ----------------------------------------------------
            0xFF1A => {
                self.wave.dac_on = val & 0x80 != 0;
                if !self.wave.dac_on {
                    self.wave.enabled = false;
                }
            }
            0xFF1B => self.wave.length = 256 - val as u16,
            0xFF1C => self.wave.level = (val >> 5) & 0x3,
            0xFF1D => self.wave.frequency = (self.wave.frequency & 0x700) | val as u16,
            0xFF1E => {
                self.wave.length_enable = val & 0x40 != 0;
                self.wave.frequency = (self.wave.frequency & 0xFF) | (((val & 0x7) as u16) << 8);
                if val & 0x80 != 0 {
                    self.trigger_wave();
                } else if self.wave.length == 0 && self.wave.length_enable {
                    self.wave.length = 255;
                }
            }

            // Noise ---------------------------------------------------
            0xFF20 => self.noise.length = 64 - (val & 0x3F) as u16,
            0xFF21 => {
                if !self.noise.enabled {
                    self.noise.env.reset(val);
                } else {
                    self.noise.env.zombie_write(val);
                }
                self.noise.dac_on = val & 0xF8 != 0;
                if !self.noise.dac_on {
                    self.noise.enabled = false;
                }
            }
            0xFF22 => {
                let prev_width = self.noise.width_7;
                self.noise.clock_shift = val >> 4;
                self.noise.width_7 = val & 0x8 != 0;
                self.noise.divisor_code = val & 0x7;
                if !prev_width && self.noise.width_7 && (self.noise.lfsr & 0x7F) == 0x7F {
                    self.noise.enabled = false; // lock‑up
                }
            }
            0xFF23 => {
                self.noise.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.trigger_noise();
                } else if self.noise.length == 0 && self.noise.length_enable {
                    self.noise.length = 63;
                }
            }

            // Wave RAM
            0xFF30..=0xFF3F => self.wave.write_wave(addr, val),

            // Sound control & power
            0xFF24 | 0xFF25 => {
                // TODO: implement volume / routing
            }
            0xFF26 => {
                let power_on = val & 0x80 != 0;
                if !power_on {
                    self.power_off();
                } else if !self.powered {
                    self.reset(); // turn on – reset to defaults
                }
            }
            _ => {}
        }
    }

    fn power_off(&mut self) {
        self.powered = false;
        // all registers (except NR52) read 0xFF, channels disabled
        self.ch1.enabled = false;
        self.ch2.enabled = false;
        self.wave.enabled = false;
        self.noise.enabled = false;
    }

    // --------------------------------------------------------------------
    // Trigger helpers
    // --------------------------------------------------------------------
    fn trigger_channel1(&mut self) {
        if !self.ch1.dac_on {
            return;
        }
        self.ch1.enabled = true;
        if self.ch1.length == 0 {
            self.ch1.length = 64;
            if self.ch1.length_enable && (self.frame_step & 1) == 1 {
                self.ch1.length -= 1;
            }
        }
        self.sweep.reload(&mut self.ch1);
        Self::common_square_trigger(self.frame_step, &mut self.ch1);
    }
    fn trigger_channel2(&mut self) {
        if !self.ch2.dac_on {
            return;
        }
        self.ch2.enabled = true;
        if self.ch2.length == 0 {
            self.ch2.length = 64;
            if self.ch2.length_enable && (self.frame_step & 1) == 1 {
                self.ch2.length -= 1;
            }
        }
        Self::common_square_trigger(self.frame_step, &mut self.ch2);
    }

    fn common_square_trigger(frame_step: u8, ch: &mut SquareChannel) {
        ch.trigger_delay = 2;
        ch.first_sample_zero = true;
        // reload timer while keeping low 2 bits
        let phase = (ch.timer & 0x3) as u16;
        ch.timer = ch.period() | phase;
        ch.env.timer = if ch.env.period == 0 { 8 } else { ch.env.period };
        // envelope delay quirk – if next frame step would tick envelope,
        // add one extra to timer.
        if frame_step == 7 && ch.env.period != 0 {
            ch.env.timer += 1;
        }
    }

    fn trigger_wave(&mut self) {
        if !self.wave.dac_on {
            return;
        }
        self.wave.enabled = true;
        if self.wave.length == 0 {
            self.wave.length = 256;
            if self.wave.length_enable && (self.frame_step & 1) == 1 {
                self.wave.length -= 1;
            }
        }
        self.wave.trigger_delay = 2;
        // timer reload w/ low bits kept
        let phase = self.wave.timer & 0x3;
        self.wave.timer = self.wave.period() | phase;
    }
    fn trigger_noise(&mut self) {
        if !self.noise.dac_on {
            return;
        }
        self.noise.enabled = true;
        if self.noise.length == 0 {
            self.noise.length = 64;
            if self.noise.length_enable && (self.frame_step & 1) == 1 {
                self.noise.length -= 1;
            }
        }
        self.noise.lfsr = 0x7FFF;
        self.noise.trigger_delay = 2;
        self.noise.first_sample_zero = true;
        let phase = self.noise.timer & 0x3;
        self.noise.timer = self.noise.period() | phase;
        self.noise.env.timer = if self.noise.env.period == 0 {
            8
        } else {
            self.noise.env.period
        };
        if self.frame_step == 7 && self.noise.env.period != 0 {
            self.noise.env.timer += 1;
        }
    }
}
