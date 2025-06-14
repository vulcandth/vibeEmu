use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const CPU_CLOCK_HZ: u32 = 4_194_304;
// 512 Hz frame sequencer tick (not doubled in CGB mode)
const FRAME_SEQUENCER_PERIOD: u32 = 8192;
const VOLUME_FACTOR: i16 = 64;

#[derive(Default, Clone, Copy)]
struct Envelope {
    initial: u8,
    period: u8,
    add: bool,
    volume: u8,
    timer: u8,
}

impl Envelope {
    fn clock(&mut self) {
        let period = if self.period == 0 { 8 } else { self.period };
        if self.timer == 0 {
            self.timer = period;
            if self.add && self.volume < 15 {
                self.volume += 1;
            } else if !self.add && self.volume > 0 {
                self.volume -= 1;
            }
        } else {
            self.timer -= 1;
        }
    }

    fn reset(&mut self, val: u8) {
        self.initial = val >> 4;
        self.volume = self.initial;
        self.period = val & 0x07;
        self.add = val & 0x08 != 0;
        self.timer = if self.period == 0 { 8 } else { self.period };
    }
}

#[derive(Default)]
// Handles Channel 1 frequency sweep logic. See TODO.md #257.
struct Sweep {
    period: u8,
    negate: bool,
    shift: u8,
    timer: u8,
    shadow: u16,
    enabled: bool,
}

impl Sweep {
    fn calculate(&self) -> u16 {
        let delta = self.shadow >> self.shift;
        if self.negate {
            self.shadow.wrapping_sub(delta)
        } else {
            self.shadow.wrapping_add(delta)
        }
    }

    fn set_params(&mut self, val: u8) {
        self.period = (val >> 4) & 0x07;
        self.negate = val & 0x08 != 0;
        self.shift = val & 0x07;
    }

    fn reload(&mut self, freq: u16) {
        self.shadow = freq;
        self.timer = if self.period == 0 { 8 } else { self.period };
        self.enabled = self.period != 0 || self.shift != 0;
    }
}

#[derive(Default)]
struct SquareChannel {
    enabled: bool,
    dac_enabled: bool,
    length: u8,
    length_enable: bool,
    duty: u8,
    duty_pos: u8,
    frequency: u16,
    timer: i32,
    envelope: Envelope,
    sweep: Option<Sweep>,
}

impl SquareChannel {
    fn new(with_sweep: bool) -> Self {
        Self {
            sweep: if with_sweep {
                Some(Sweep::default())
            } else {
                None
            },
            ..Default::default()
        }
    }

    fn period(&self) -> i32 {
        ((2048 - self.frequency) * 4) as i32
    }

    fn step(&mut self, cycles: u32) {
        if !self.enabled || !self.dac_enabled {
            return;
        }
        let mut cycles = cycles as i32;
        while self.timer <= cycles {
            cycles -= self.timer;
            self.timer = self.period();
            self.duty_pos = (self.duty_pos + 1) & 7;
        }
        self.timer -= cycles;
    }

    fn output(&self) -> u8 {
        if !self.enabled || !self.dac_enabled {
            return 0;
        }
        const DUTY_TABLE: [[u8; 8]; 4] = [
            [0, 1, 0, 0, 0, 0, 0, 0], // 12.5%
            [0, 1, 1, 0, 0, 0, 0, 0], // 25%
            [0, 1, 1, 1, 1, 0, 0, 0], // 50%
            [1, 0, 0, 1, 1, 1, 1, 1], // 75%
        ];
        let level = DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        level * self.envelope.volume
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn clock_sweep(&mut self) {
        if let Some(sweep) = self.sweep.as_mut() {
            if !sweep.enabled {
                return;
            }
            if sweep.timer > 0 {
                sweep.timer -= 1;
            }
            if sweep.timer == 0 {
                sweep.timer = if sweep.period == 0 { 8 } else { sweep.period };
                let mut new_freq = sweep.calculate();
                if new_freq > 2047 {
                    self.enabled = false;
                    sweep.enabled = false;
                } else if sweep.shift != 0 {
                    sweep.shadow = new_freq;
                    self.frequency = new_freq;
                    new_freq = sweep.calculate();
                    if new_freq > 2047 {
                        self.enabled = false;
                        sweep.enabled = false;
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct WaveChannel {
    enabled: bool,
    dac_enabled: bool,
    length: u16,
    length_enable: bool,
    volume: u8,
    position: u8,
    last_sample: u8,
    frequency: u16,
    timer: i32,
}

impl WaveChannel {
    fn period(&self) -> i32 {
        ((2048 - self.frequency) * 2) as i32
    }

    fn step(&mut self, cycles: u32, wave_ram: &[u8; 0x10]) {
        if !self.enabled || !self.dac_enabled {
            return;
        }
        let mut cycles = cycles as i32;
        while self.timer <= cycles {
            cycles -= self.timer;
            self.timer = self.period();
            self.position = (self.position + 1) & 0x1F;
            let byte = wave_ram[(self.position / 2) as usize];
            self.last_sample = if self.position & 1 == 0 {
                byte >> 4
            } else {
                byte & 0x0F
            };
        }
        self.timer -= cycles;
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled || !self.dac_enabled {
            return 0;
        }
        match self.volume {
            0 => 0,
            1 => self.last_sample,
            2 => self.last_sample >> 1,
            3 => self.last_sample >> 2,
            _ => 0,
        }
    }
}

#[derive(Default)]
struct NoiseChannel {
    enabled: bool,
    dac_enabled: bool,
    length: u8,
    length_enable: bool,
    envelope: Envelope,
    clock_shift: u8,
    divisor: u8,
    width7: bool,
    lfsr: u16,
    timer: i32,
}

impl NoiseChannel {
    fn period(&self) -> i32 {
        let r = match self.divisor {
            0 => 8,
            _ => (self.divisor as i32) * 16,
        };
        r << self.clock_shift
    }

    fn step(&mut self, cycles: u32) {
        if !self.enabled || !self.dac_enabled {
            return;
        }
        let mut cycles = cycles as i32;
        while self.timer <= cycles {
            cycles -= self.timer;
            self.timer = self.period();
            let bit = (self.lfsr & 1) ^ ((self.lfsr >> 1) & 1);
            self.lfsr >>= 1;
            self.lfsr |= bit << 14;
            if self.width7 {
                self.lfsr = (self.lfsr & !0x40) | (bit << 6);
            }
        }
        self.timer -= cycles;
    }

    fn output(&self) -> u8 {
        if !self.enabled || !self.dac_enabled {
            return 0;
        }
        if self.lfsr & 1 == 0 {
            self.envelope.volume
        } else {
            0
        }
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }
}

struct FrameSequencer {
    step: u8,
}

impl FrameSequencer {
    fn new() -> Self {
        Self { step: 0 }
    }

    fn advance(&mut self) -> u8 {
        let s = self.step;
        self.step = (self.step + 1) & 7;
        s
    }
}

pub struct Apu {
    ch1: SquareChannel,
    ch2: SquareChannel,
    ch3: WaveChannel,
    ch4: NoiseChannel,
    wave_ram: [u8; 0x10],
    nr50: u8,
    nr51: u8,
    nr52: u8,
    sequencer: FrameSequencer,
    seq_counter: u32,
    sample_timer: u32,
    sample_rate: u32,
    samples: VecDeque<i16>,
    hp_prev_input_left: f32,
    hp_prev_output_left: f32,
    hp_prev_input_right: f32,
    hp_prev_output_right: f32,
}

impl Apu {
    fn read_mask(addr: u16) -> u8 {
        match addr {
            0xFF10 => 0x80,
            0xFF11 => 0x3F,
            0xFF12 => 0x00,
            0xFF13 => 0xFF,
            0xFF14 => 0xBF,
            0xFF16 => 0x3F,
            0xFF17 => 0x00,
            0xFF18 => 0xFF,
            0xFF19 => 0xBF,
            0xFF1A => 0x7F,
            0xFF1B => 0xFF,
            0xFF1C => 0x9F,
            0xFF1D => 0xFF,
            0xFF1E => 0xBF,
            0xFF20 => 0xFF,
            0xFF21 => 0x00,
            0xFF22 => 0x00,
            0xFF23 => 0xBF,
            0xFF24 => 0x00,
            0xFF25 => 0x00,
            0xFF26 => 0x70,
            0xFF15 | 0xFF1F => 0xFF,
            0xFF30..=0xFF3F => 0x00,
            _ => 0xFF,
        }
    }

    fn power_off(&mut self) {
        self.ch1 = SquareChannel::new(true);
        self.ch2 = SquareChannel::new(false);
        self.ch3 = WaveChannel::default();
        self.ch4 = NoiseChannel::default();
        self.nr50 = 0;
        self.nr51 = 0;
        self.samples.clear();
        self.hp_prev_input_left = 0.0;
        self.hp_prev_output_left = 0.0;
        self.hp_prev_input_right = 0.0;
        self.hp_prev_output_right = 0.0;
    }
    pub fn new() -> Self {
        let mut apu = Self {
            ch1: SquareChannel::new(true),
            ch2: SquareChannel::new(false),
            ch3: WaveChannel::default(),
            ch4: NoiseChannel::default(),
            wave_ram: [0; 0x10],
            nr50: 0x77,
            nr51: 0xF3,
            nr52: 0xF1,
            sequencer: FrameSequencer::new(),
            seq_counter: 0,
            sample_timer: 0,
            sample_rate: 44100,
            samples: VecDeque::with_capacity(4096),
            hp_prev_input_left: 0.0,
            hp_prev_output_left: 0.0,
            hp_prev_input_right: 0.0,
            hp_prev_output_right: 0.0,
        };

        // Initialize channels to power-on register defaults
        apu.ch1.duty = 2;
        apu.ch1.length = 0x3F;
        apu.ch1.envelope.initial = 0xF;
        apu.ch1.envelope.volume = 0xF;
        apu.ch1.envelope.period = 3;
        apu.ch1.frequency = 0x03FF;
        apu.ch1.dac_enabled = true;

        apu.ch2.length = 0x3F;
        apu.ch2.frequency = 0x03FF;
        apu.ch2.dac_enabled = false;

        apu.ch3.dac_enabled = true;
        apu.ch3.length = 0xFF;
        apu.ch3.frequency = 0x03FF;

        apu.ch4.length = 0xFF;
        apu.ch4.dac_enabled = false;

        apu
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        if addr == 0xFF26 {
            let mut val = 0x70;
            if self.nr52 & 0x80 != 0 {
                val |= 0x80;
            }
            if self.ch1.enabled {
                val |= 0x01;
            }
            if self.ch2.enabled {
                val |= 0x02;
            }
            if self.ch3.enabled {
                val |= 0x04;
            }
            if self.ch4.enabled {
                val |= 0x08;
            }
            return val;
        }

        if self.nr52 & 0x80 == 0 && !(0xFF30..=0xFF3F).contains(&addr) {
            return Apu::read_mask(addr);
        }

        let value = match addr {
            0xFF10 => self
                .ch1
                .sweep
                .as_ref()
                .map(|s| (s.period << 4) | ((s.negate as u8) << 3) | s.shift)
                .unwrap_or(0x00),
            0xFF11 => (self.ch1.duty << 6) | self.ch1.length,
            0xFF12 => {
                (self.ch1.envelope.initial << 4)
                    | ((self.ch1.envelope.add as u8) << 3)
                    | self.ch1.envelope.period
            }
            0xFF13 => (self.ch1.frequency & 0xFF) as u8,
            0xFF14 => {
                ((self.ch1.length_enable as u8) << 6) | ((self.ch1.frequency >> 8) as u8 & 0x07)
            }
            0xFF16 => (self.ch2.duty << 6) | self.ch2.length,
            0xFF17 => {
                (self.ch2.envelope.initial << 4)
                    | ((self.ch2.envelope.add as u8) << 3)
                    | self.ch2.envelope.period
            }
            0xFF18 => (self.ch2.frequency & 0xFF) as u8,
            0xFF19 => {
                ((self.ch2.length_enable as u8) << 6) | ((self.ch2.frequency >> 8) as u8 & 0x07)
            }
            0xFF1A => {
                if self.ch3.dac_enabled {
                    0x80
                } else {
                    0
                }
            }
            0xFF1B => self.ch3.length as u8,
            0xFF1C => (self.ch3.volume << 5) | 0x9F,
            0xFF1D => (self.ch3.frequency & 0xFF) as u8,
            0xFF1E => {
                ((self.ch3.length_enable as u8) << 6) | ((self.ch3.frequency >> 8) as u8 & 0x07)
            }
            0xFF20 => self.ch4.length,
            0xFF21 => {
                (self.ch4.envelope.initial << 4)
                    | ((self.ch4.envelope.add as u8) << 3)
                    | self.ch4.envelope.period
            }
            0xFF22 => {
                (self.ch4.clock_shift << 4) | ((self.ch4.width7 as u8) << 3) | self.ch4.divisor
            }
            0xFF23 => (self.ch4.length_enable as u8) << 6,
            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF30..=0xFF3F => {
                if self.ch3.enabled && self.ch3.dac_enabled {
                    0xFF
                } else {
                    self.wave_ram[(addr - 0xFF30) as usize]
                }
            }
            _ => 0xFF,
        };

        value | Apu::read_mask(addr)
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        if self.nr52 & 0x80 == 0 && addr != 0xFF26 && !(0xFF30..=0xFF3F).contains(&addr) {
            return;
        }
        match addr {
            0xFF10 => {
                if let Some(s) = self.ch1.sweep.as_mut() {
                    s.set_params(val);
                }
            }
            0xFF11 => {
                self.ch1.duty = val >> 6;
                self.ch1.length = 64 - (val & 0x3F);
            }
            0xFF12 => {
                self.ch1.envelope.reset(val);
                self.ch1.dac_enabled = val & 0xF0 != 0;
                if !self.ch1.dac_enabled {
                    self.ch1.enabled = false;
                }
            }
            0xFF13 => self.ch1.frequency = (self.ch1.frequency & 0x700) | val as u16,
            0xFF14 => {
                self.ch1.length_enable = val & 0x40 != 0;
                self.ch1.frequency = (self.ch1.frequency & 0xFF) | (((val & 0x07) as u16) << 8);
                if val & 0x80 != 0 {
                    self.trigger_square(1);
                }
            }
            0xFF16 => {
                self.ch2.duty = val >> 6;
                self.ch2.length = 64 - (val & 0x3F);
            }
            0xFF17 => {
                self.ch2.envelope.reset(val);
                self.ch2.dac_enabled = val & 0xF0 != 0;
                if !self.ch2.dac_enabled {
                    self.ch2.enabled = false;
                }
            }
            0xFF18 => self.ch2.frequency = (self.ch2.frequency & 0x700) | val as u16,
            0xFF19 => {
                self.ch2.length_enable = val & 0x40 != 0;
                self.ch2.frequency = (self.ch2.frequency & 0xFF) | (((val & 0x07) as u16) << 8);
                if val & 0x80 != 0 {
                    self.trigger_square(2);
                }
            }
            0xFF1A => self.ch3.dac_enabled = val & 0x80 != 0,
            0xFF1B => self.ch3.length = 256 - val as u16,
            0xFF1C => self.ch3.volume = (val >> 5) & 0x03,
            0xFF1D => self.ch3.frequency = (self.ch3.frequency & 0x700) | val as u16,
            0xFF1E => {
                self.ch3.length_enable = val & 0x40 != 0;
                self.ch3.frequency = (self.ch3.frequency & 0xFF) | (((val & 0x07) as u16) << 8);
                if val & 0x80 != 0 {
                    self.trigger_wave();
                }
            }
            0xFF20 => self.ch4.length = 64 - (val & 0x3F),
            0xFF21 => {
                self.ch4.envelope.reset(val);
                self.ch4.dac_enabled = val & 0xF0 != 0;
                if !self.ch4.dac_enabled {
                    self.ch4.enabled = false;
                }
            }
            0xFF22 => {
                let new_width7 = val & 0x08 != 0;
                if !self.ch4.width7 && new_width7 && (self.ch4.lfsr & 0x7F) == 0x7F {
                    self.ch4.enabled = false;
                }
                self.ch4.clock_shift = val >> 4;
                self.ch4.width7 = new_width7;
                self.ch4.divisor = val & 0x07;
            }
            0xFF23 => {
                self.ch4.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.trigger_noise();
                }
            }
            0xFF24 => self.nr50 = val,
            0xFF25 => self.nr51 = val,
            0xFF26 => {
                if val & 0x80 == 0 {
                    self.nr52 &= 0x7F;
                    self.power_off();
                } else {
                    self.nr52 |= 0x80;
                }
            }
            0xFF30..=0xFF3F => {
                if !(self.ch3.enabled && self.ch3.dac_enabled) {
                    self.wave_ram[(addr - 0xFF30) as usize] = val;
                }
            }
            _ => {}
        }
    }

    fn trigger_square(&mut self, idx: u8) {
        let ch = if idx == 1 {
            &mut self.ch1
        } else {
            &mut self.ch2
        };
        ch.enabled = true;
        ch.duty_pos = 0;
        ch.timer = ch.period();
        ch.envelope.volume = ch.envelope.initial;
        if idx == 1 {
            if let Some(s) = ch.sweep.as_mut() {
                s.reload(ch.frequency);
                if s.shift != 0 {
                    let new_freq = s.calculate();
                    if new_freq > 2047 {
                        ch.enabled = false;
                        s.enabled = false;
                    } else {
                        s.shadow = new_freq;
                        ch.frequency = new_freq;
                    }
                }
            }
        }
        if ch.length == 0 {
            ch.length = 64;
        }
    }

    fn trigger_wave(&mut self) {
        self.ch3.enabled = true;
        self.ch3.position = 0;
        self.ch3.timer = self.ch3.period();
        if self.ch3.length == 0 {
            self.ch3.length = 256;
        }
    }

    fn trigger_noise(&mut self) {
        self.ch4.enabled = true;
        self.ch4.lfsr = 0x7FFF;
        self.ch4.timer = self.ch4.period();
        self.ch4.envelope.volume = self.ch4.envelope.initial;
        if self.ch4.length == 0 {
            self.ch4.length = 64;
        }
    }

    fn clock_frame_sequencer(&mut self, step: u8) {
        if matches!(step, 0 | 2 | 4 | 6) {
            self.ch1.clock_length();
            self.ch2.clock_length();
            self.ch3.clock_length();
            self.ch4.clock_length();
        }
        if step == 2 || step == 6 {
            self.ch1.clock_sweep();
        }
        if step == 7 {
            self.ch1.envelope.clock();
            self.ch2.envelope.clock();
            self.ch4.envelope.clock();
        }
    }

    pub fn step(&mut self, cycles: u16) {
        let cycles = cycles as u32;
        self.seq_counter += cycles;
        while self.seq_counter >= FRAME_SEQUENCER_PERIOD {
            self.seq_counter -= FRAME_SEQUENCER_PERIOD;
            let step = self.sequencer.advance();
            self.clock_frame_sequencer(step);
        }
        self.ch1.step(cycles);
        self.ch2.step(cycles);
        self.ch3.step(cycles, &self.wave_ram);
        self.ch4.step(cycles);
        self.sample_timer += cycles;
        let cps = CPU_CLOCK_HZ / self.sample_rate;
        while self.sample_timer >= cps {
            self.sample_timer -= cps;
            let (left, right) = self.mix_output();
            self.samples.push_back(left);
            self.samples.push_back(right);
        }
    }

    fn mix_output(&mut self) -> (i16, i16) {
        let ch1 = self.ch1.output() as i16 - 8;
        let ch2 = self.ch2.output() as i16 - 8;
        let ch3 = self.ch3.output() as i16 - 8;
        let ch4 = self.ch4.output() as i16 - 8;

        let mut left = 0i16;
        let mut right = 0i16;

        if self.nr51 & 0x10 != 0 {
            left += ch1;
        }
        if self.nr51 & 0x01 != 0 {
            right += ch1;
        }
        if self.nr51 & 0x20 != 0 {
            left += ch2;
        }
        if self.nr51 & 0x02 != 0 {
            right += ch2;
        }
        if self.nr51 & 0x40 != 0 {
            left += ch3;
        }
        if self.nr51 & 0x04 != 0 {
            right += ch3;
        }
        if self.nr51 & 0x80 != 0 {
            left += ch4;
        }
        if self.nr51 & 0x08 != 0 {
            right += ch4;
        }

        let left_vol = ((self.nr50 >> 4) & 0x07) + 1;
        let right_vol = (self.nr50 & 0x07) + 1;

        let left_sample = left * left_vol as i16 * VOLUME_FACTOR;
        let right_sample = right * right_vol as i16 * VOLUME_FACTOR;

        self.dc_block(left_sample, right_sample)
    }

    fn dc_block(&mut self, left: i16, right: i16) -> (i16, i16) {
        const DC_FILTER_R: f32 = 0.999;
        let left_in = left as f32;
        let right_in = right as f32;
        let left_out = left_in - self.hp_prev_input_left + DC_FILTER_R * self.hp_prev_output_left;
        let right_out =
            right_in - self.hp_prev_input_right + DC_FILTER_R * self.hp_prev_output_right;
        self.hp_prev_input_left = left_in;
        self.hp_prev_output_left = left_out;
        self.hp_prev_input_right = right_in;
        self.hp_prev_output_right = right_out;
        (left_out.round() as i16, right_out.round() as i16)
    }

    pub fn ch1_frequency(&self) -> u16 {
        self.ch1.frequency
    }

    pub fn pop_sample(&mut self) -> Option<i16> {
        self.samples.pop_front()
    }

    pub fn sequencer_step(&self) -> u8 {
        self.sequencer.step
    }

    pub fn start_stream(apu: Arc<Mutex<Self>>) -> cpal::Stream {
        let host = cpal::default_host();
        let device = host.default_output_device().expect("no output device");
        let supported = device
            .default_output_config()
            .expect("no supported output config");
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        {
            let mut a = apu.lock().unwrap();
            a.sample_rate = config.sample_rate.0;
        }
        let channels = config.channels as usize;
        let err_fn = |err| eprintln!("cpal stream error: {err}");

        let stream = match sample_format {
            cpal::SampleFormat::I16 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [i16], _| {
                        let mut apu = apu.lock().unwrap();
                        for frame in data.chunks_mut(channels) {
                            let left = apu.pop_sample().unwrap_or(0);
                            let right = apu.pop_sample().unwrap_or(0);
                            frame[0] = left;
                            if channels > 1 {
                                frame[1] = right;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .unwrap(),
            cpal::SampleFormat::U16 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [u16], _| {
                        let mut apu = apu.lock().unwrap();
                        for frame in data.chunks_mut(channels) {
                            let left = apu.pop_sample().unwrap_or(0);
                            let right = apu.pop_sample().unwrap_or(0);
                            frame[0] = (left as i32 + 32768) as u16;
                            if channels > 1 {
                                frame[1] = (right as i32 + 32768) as u16;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .unwrap(),
            cpal::SampleFormat::F32 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [f32], _| {
                        let mut apu = apu.lock().unwrap();
                        for frame in data.chunks_mut(channels) {
                            let left = apu.pop_sample().unwrap_or(0) as f32 / 32768.0;
                            let right = apu.pop_sample().unwrap_or(0) as f32 / 32768.0;
                            frame[0] = left;
                            if channels > 1 {
                                frame[1] = right;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .unwrap(),
            _ => panic!("Unsupported sample format"),
        };

        stream.play().expect("failed to play stream");
        stream
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}
