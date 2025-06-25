use std::collections::VecDeque;

const CPU_CLOCK_HZ: u32 = 4_194_304;
// 512 Hz frame sequencer tick (not doubled in CGB mode)
const FRAME_SEQUENCER_PERIOD: u32 = 8192;
const VOLUME_FACTOR: i16 = 64;
pub const AUDIO_LATENCY_MS: u32 = 40;
/// Hardware waits exactly **one machine-cycle (4 CPU cycles)** between a
/// channel trigger and the first tick of its frequency timer.
const TRIGGER_DELAY_CYCLES: i32 = 4; // cycles

const POWER_ON_REGS: [u8; 0x30] = [
    0x80, 0xBF, 0xF3, 0xFF, 0xBF, 0xFF, 0x3F, 0x00, 0xFF, 0xBF, 0x7F, 0xFF, 0x9F, 0xFF, 0xBF, 0xFF,
    0xFF, 0x00, 0x00, 0xBF, 0x77, 0xF3, 0xF1, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

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
    pending_reset: bool,
    frequency: u16,
    timer: i32,
    envelope: Envelope,
    sweep: Option<Sweep>,
    first_sample: bool,
    out_latched: u8,
    out_stage1: u8,
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
            if self.pending_reset {
                self.pending_reset = false;
                self.duty_pos = 0;
            } else {
                self.duty_pos = (self.duty_pos + 1) & 7;
            }
        }
        self.timer -= cycles;
    }

    fn compute_output(&mut self) -> u8 {
        if !self.enabled || !self.dac_enabled {
            return 0;
        }
        if self.first_sample {
            self.first_sample = false;
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

    fn output(&mut self) -> u8 {
        self.compute_output()
    }

    fn tick_1mhz(&mut self) {
        let fresh = self.compute_output();
        self.out_latched = self.out_stage1;
        self.out_stage1 = fresh;
    }

    fn current_sample(&self) -> u8 {
        self.out_latched
    }

    fn peek_sample(&self) -> u8 {
        if !self.enabled || !self.dac_enabled || self.pending_reset || self.first_sample {
            return 0;
        }
        const DUTY_TABLE: [[u8; 8]; 4] = [
            [0, 1, 0, 0, 0, 0, 0, 0],
            [0, 1, 1, 0, 0, 0, 0, 0],
            [0, 1, 1, 1, 1, 0, 0, 0],
            [1, 0, 0, 1, 1, 1, 1, 1],
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

    fn peek_sample(&self) -> u8 {
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

    fn peek_sample(&self) -> u8 {
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
    sample_timer: u32,
    sample_rate: u32,
    samples: VecDeque<i16>,
    speed_factor: f32,
    hp_prev_input_left: f32,
    hp_prev_output_left: f32,
    hp_prev_input_right: f32,
    hp_prev_output_right: f32,
    pcm12: u8,
    pcm34: u8,
    regs: [u8; 0x30],
    wave_shadow: [u8; 0x10],
    cpu_cycles: u64,
}

impl Apu {
    // Keep <= 40 ms of stereo samples in the queue
    const MAX_SAMPLES: usize = ((44100 * AUDIO_LATENCY_MS as usize) / 1000) * 2;

    pub fn set_speed(&mut self, speed: f32) {
        self.speed_factor = speed;
    }

    pub fn push_sample(&mut self, s: i16) {
        if self.speed_factor != 1.0 {
            return;
        }
        if self.samples.len() >= Self::MAX_SAMPLES {
            let excess = self.samples.len() + 1 - Self::MAX_SAMPLES;
            self.samples.drain(..excess);
        }
        self.samples.push_back(s);
    }

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
        self.regs.fill(0);
        self.nr50 = 0;
        self.nr51 = 0;
        self.samples.clear();
        self.speed_factor = 1.0;
        self.hp_prev_input_left = 0.0;
        self.hp_prev_output_left = 0.0;
        self.hp_prev_input_right = 0.0;
        self.hp_prev_output_right = 0.0;
        self.pcm12 = 0;
        self.pcm34 = 0;
    }
    pub fn new() -> Self {
        let mut apu = Self {
            ch1: SquareChannel::new(true),
            ch2: SquareChannel::new(false),
            ch3: WaveChannel::default(),
            ch4: NoiseChannel::default(),
            wave_ram: [0; 0x10],
            regs: POWER_ON_REGS,
            wave_shadow: [0; 0x10],
            nr50: 0x77,
            nr51: 0xF3,
            nr52: 0xF1,
            sequencer: FrameSequencer::new(),
            sample_timer: 0,
            sample_rate: 44100,
            samples: VecDeque::with_capacity(4096),
            speed_factor: 1.0,
            hp_prev_input_left: 0.0,
            hp_prev_output_left: 0.0,
            hp_prev_input_right: 0.0,
            hp_prev_output_right: 0.0,
            pcm12: 0,
            pcm34: 0,
            cpu_cycles: 0,
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
            let mut val = self.regs[(addr - 0xFF10) as usize] & 0x7F;
            val |= self.nr52 & 0x80;
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
            return val | Apu::read_mask(addr);
        }

        if (0xFF30..=0xFF3F).contains(&addr) {
            if self.ch3.enabled && self.ch3.dac_enabled {
                return 0xFF;
            }
            return self.wave_ram[(addr - 0xFF30) as usize];
        }

        let idx = (addr - 0xFF10) as usize;
        self.regs[idx] | Apu::read_mask(addr)
    }

    pub fn read_pcm(&self, addr: u16) -> u8 {
        if self.nr52 & 0x80 == 0 {
            return 0xFF;
        }
        match addr {
            0xFF76 => self.pcm12,
            0xFF77 => self.pcm34,
            _ => 0xFF,
        }
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        if self.nr52 & 0x80 == 0 && addr != 0xFF26 && !(0xFF30..=0xFF3F).contains(&addr) {
            return;
        }

        if (0xFF30..=0xFF3F).contains(&addr) {
            self.wave_shadow[(addr - 0xFF30) as usize] = val;
        }

        if addr != 0xFF26 && (0xFF10..=0xFF3F).contains(&addr) {
            self.regs[(addr - 0xFF10) as usize] = val;
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
                let prev = self.ch1.length_enable;
                self.ch1.length_enable = val & 0x40 != 0;
                if !prev && self.ch1.length_enable {
                    let next_step = (self.sequencer.step + 1) & 7;
                    Apu::maybe_extra_len_clock(&mut self.ch1, next_step);
                }
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
                let prev = self.ch2.length_enable;
                self.ch2.length_enable = val & 0x40 != 0;
                if !prev && self.ch2.length_enable {
                    let next_step = (self.sequencer.step + 1) & 7;
                    Apu::maybe_extra_len_clock(&mut self.ch2, next_step);
                }
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
                let prev = self.ch4.length_enable;
                self.ch4.length_enable = val & 0x40 != 0;
                if !prev && self.ch4.length_enable {
                    let next_step = (self.sequencer.step + 1) & 7;
                    if !matches!(next_step, 0 | 2 | 4 | 6) && self.ch4.length > 0 {
                        self.ch4.clock_length();
                    }
                }
                if val & 0x80 != 0 {
                    self.trigger_noise();
                }
            }
            0xFF24 => self.nr50 = val,
            0xFF25 => self.nr51 = val,
            0xFF26 => {
                if val & 0x80 == 0 {
                    self.nr52 &= !0x80;
                    self.power_off();
                } else {
                    if self.nr52 & 0x80 == 0 {
                        self.ch1.out_latched = 0;
                        self.ch1.out_stage1 = 0;
                        self.ch2.out_latched = 0;
                        self.ch2.out_stage1 = 0;
                        self.cpu_cycles = 0;
                        self.sequencer.step = 0;
                    }
                    self.nr52 |= 0x80;
                }
                let idx = (addr - 0xFF10) as usize;
                self.regs[idx] = 0x70 | (self.nr52 & 0x80);
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
        // ➊  Reload the 11-bit frequency immediately
        // ➋  Wait one M-cycle (4 CPU cycles) before the first decrement
        let period = ch.period();
        ch.timer = period + TRIGGER_DELAY_CYCLES;
        ch.pending_reset = true;
        ch.first_sample = true;
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
        if ch.length == 64 && ch.length_enable {
            let upcoming = self.sequencer.step;
            if matches!(upcoming, 0 | 2 | 4 | 6) {
                ch.length = 63;
            }
        }
    }

    fn trigger_wave(&mut self) {
        self.ch3.enabled = true;
        self.ch3.position = 0;
        self.ch3.timer = self.ch3.period();
        if self.ch3.length == 0 {
            self.ch3.length = 256;
        }
        if self.ch3.length == 256 && self.ch3.length_enable {
            let upcoming = self.sequencer.step;
            if matches!(upcoming, 0 | 2 | 4 | 6) {
                self.ch3.length = 255;
            }
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
        if self.ch4.length == 64 && self.ch4.length_enable {
            let upcoming = self.sequencer.step;
            if matches!(upcoming, 0 | 2 | 4 | 6) {
                self.ch4.length = 63;
            }
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

    /// Call once per M-cycle / machine step.
    pub fn tick(&mut self, div_prev: u16, div_now: u16, double_speed: bool) {
        self.ch1.tick_1mhz();
        self.ch2.tick_1mhz();
        // A 512 Hz tick occurs **on the falling edge** of DIV bit 4
        // (bit 5 in double-speed mode).
        let bit = if double_speed { 5 } else { 4 };
        let falling_edge = ((div_prev >> bit) & 1) == 1 && ((div_now >> bit) & 1) == 0;
        if falling_edge {
            let step = self.sequencer.advance();
            self.clock_frame_sequencer(step);
        }
        self.refresh_pcm_regs();
    }

    fn maybe_extra_len_clock(ch: &mut SquareChannel, upcoming_step: u8) {
        if !matches!(upcoming_step, 0 | 2 | 4 | 6) && ch.length > 0 {
            ch.clock_length();
        }
    }

    /// Update FF76/FF77 to reflect the current channel outputs.
    fn refresh_pcm_regs(&mut self) {
        let ch1 = self.ch1.current_sample();
        let ch2 = self.ch2.current_sample();
        let ch3 = self.ch3.peek_sample();
        let ch4 = self.ch4.peek_sample();
        self.pcm12 = (ch2 << 4) | ch1;
        self.pcm34 = (ch4 << 4) | ch3;
    }

    pub fn step(&mut self, cycles: u16) {
        let cycles32 = cycles as u32;
        self.cpu_cycles = self.cpu_cycles.wrapping_add(cycles as u64);
        self.ch1.step(cycles32);
        self.ch2.step(cycles32);
        self.ch3.step(cycles32, &self.wave_ram);
        self.ch4.step(cycles32);
        self.sample_timer += cycles32;
        let cps = CPU_CLOCK_HZ / self.sample_rate;
        while self.sample_timer >= cps {
            self.sample_timer -= cps;
            let (left, right) = self.mix_output();
            self.push_sample(left);
            self.push_sample(right);
        }
    }

    fn mix_output(&mut self) -> (i16, i16) {
        let out1 = self.ch1.current_sample();
        let out2 = self.ch2.current_sample();
        let out3 = self.ch3.output();
        let out4 = self.ch4.output();

        let ch1 = out1 as i16 - 8;
        let ch2 = out2 as i16 - 8;
        let ch3 = out3 as i16 - 8;
        let ch4 = out4 as i16 - 8;

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

    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate;
    }

    pub fn pop_sample(&mut self) -> Option<i16> {
        self.samples.pop_front()
    }

    pub fn sequencer_step(&self) -> u8 {
        self.sequencer.step
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}
