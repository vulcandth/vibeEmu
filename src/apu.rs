use std::collections::VecDeque;

const CPU_CLOCK_HZ: u32 = 4_194_304;
// 512 Hz frame sequencer tick (not doubled in CGB mode)
const FRAME_SEQUENCER_PERIOD: u32 = 8192;
const VOLUME_FACTOR: i16 = 64;
pub const AUDIO_LATENCY_MS: u32 = 40;
// Audio sample pipeline delay is computed dynamically when a channel is
// triggered.  See `trigger_square` for details.

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

    fn zombie_update(&mut self, old_val: u8, new_val: u8) {
        let old_period = old_val & 0x07;
        let old_add = old_val & 0x08 != 0;
        let new_add = new_val & 0x08 != 0;
        let mut vol = self.volume;
        if old_period == 0 {
            let automatic = if old_add { vol < 15 } else { vol > 0 };
            if automatic {
                vol = vol.wrapping_add(1);
            } else if !old_add {
                vol = vol.wrapping_add(2);
            }
        }
        if old_add != new_add {
            vol = 16 - vol;
        }
        self.volume = vol & 0x0F;
        self.initial = new_val >> 4;
        self.period = new_val & 0x07;
        self.add = new_add;
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
    /// True if a subtraction sweep calculation has occurred since the last
    /// trigger.
    neg_used: bool,
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

    fn set_params(&mut self, val: u8) -> bool {
        let new_period = (val >> 4) & 0x07;
        let old_negate = self.negate;
        self.negate = val & 0x08 != 0;
        self.shift = val & 0x07;

        // Writing a pace of 0 disables further sweep iterations immediately.
        // When the period changes from 0 to a non-zero value, the timer is
        // reloaded so that iterations resume without waiting for the next
        // trigger or sweep step.
        if new_period == 0 {
            self.enabled = false;
        } else if self.period == 0 {
            self.timer = new_period;
            self.enabled = self.shift != 0 || new_period != 0;
        }

        self.period = new_period;
        if old_negate && !self.negate && self.neg_used {
            self.enabled = false;
            return true;
        }
        false
    }

    fn reload(&mut self, freq: u16) {
        self.shadow = freq;
        self.timer = if self.period == 0 { 8 } else { self.period };
        self.enabled = self.period != 0 || self.shift != 0;
        self.neg_used = false;
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
                    if sweep.negate {
                        sweep.neg_used = true;
                    }
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
        if self.clock_shift >= 14 {
            return;
        }
        let mut cycles = cycles as i32;
        while self.timer <= cycles {
            cycles -= self.timer;
            self.timer = self.period();
            let bit0 = self.lfsr & 1;
            let bit1 = (self.lfsr >> 1) & 1;
            // The Game Boy noise channel uses an LFSR where the feedback bit is
            // the XNOR of bit 0 and bit 1. A value of 1 is produced when the
            // bits are identical, otherwise 0.
            let bit = (!(bit0 ^ bit1)) & 1;
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
    /// Counts 1 MHz ticks; the low two bits determine the phase of the
    /// square channels' low-frequency divider.
    lf_div_counter: u64,
    /// True when the CPU is in double-speed mode (KEY1 bit 0 set and prepared).
    double_speed: bool,
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
            lf_div_counter: 0,
            double_speed: false,
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

        let idx = (addr - 0xFF10) as usize;
        let old_val = if (0xFF10..=0xFF3F).contains(&addr) {
            self.regs[idx]
        } else {
            0
        };

        if addr != 0xFF26 && (0xFF10..=0xFF3F).contains(&addr) {
            self.regs[idx] = val;
        }

        match addr {
            0xFF10 => {
                if let Some(s) = self.ch1.sweep.as_mut() {
                    let disable = s.set_params(val);
                    if disable {
                        self.ch1.enabled = false;
                    }
                }
            }
            0xFF11 => {
                self.ch1.duty = val >> 6;
                self.ch1.length = 64 - (val & 0x3F);
            }
            0xFF12 => {
                if self.ch1.enabled {
                    self.ch1.envelope.zombie_update(old_val, val);
                } else {
                    self.ch1.envelope.reset(val);
                }
                self.ch1.dac_enabled = val & 0xF8 != 0;
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
                if self.ch2.enabled {
                    self.ch2.envelope.zombie_update(old_val, val);
                } else {
                    self.ch2.envelope.reset(val);
                }
                self.ch2.dac_enabled = val & 0xF8 != 0;
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
            0xFF1A => {
                self.ch3.dac_enabled = val & 0x80 != 0;
                if !self.ch3.dac_enabled {
                    self.ch3.enabled = false;
                }
            }
            0xFF1B => self.ch3.length = 256 - val as u16,
            0xFF1C => self.ch3.volume = (val >> 5) & 0x03,
            0xFF1D => self.ch3.frequency = (self.ch3.frequency & 0x700) | val as u16,
            0xFF1E => {
                let prev = self.ch3.length_enable;
                self.ch3.length_enable = val & 0x40 != 0;
                if !prev && self.ch3.length_enable {
                    let next_step = (self.sequencer.step + 1) & 7;
                    if !matches!(next_step, 0 | 2 | 4 | 6) && self.ch3.length > 0 {
                        self.ch3.clock_length();
                    }
                }
                self.ch3.frequency = (self.ch3.frequency & 0xFF) | (((val & 0x07) as u16) << 8);
                if val & 0x80 != 0 {
                    self.trigger_wave();
                }
            }
            0xFF20 => self.ch4.length = 64 - (val & 0x3F),
            0xFF21 => {
                if self.ch4.enabled {
                    self.ch4.envelope.zombie_update(old_val, val);
                } else {
                    self.ch4.envelope.reset(val);
                }
                self.ch4.dac_enabled = val & 0xF8 != 0;
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
        // Compute the delay until the first duty step using the 1 MHz divider
        // instead of the CPU cycle counter. This keeps the phase consistent
        // regardless of CPU speed.
        let sample_length = (2048 - ch.frequency) as i32;
        let lf_div = (self.lf_div_counter & 0x3) as i32;
        // Base delay depends on whether the channel was previously enabled and
        // on the current CPU speed.
        let base = if self.double_speed {
            if ch.enabled { 19 } else { 21 }
        } else if ch.enabled {
            40
        } else {
            42
        };
        let mut delay_cycles = base - lf_div;
        let min_delay = sample_length * 2;
        if delay_cycles < min_delay {
            delay_cycles = min_delay;
        }
        let new_timer = sample_length * 2 + delay_cycles;
        let low_bits = ch.timer & 0x3;
        ch.timer = ((new_timer & !0x3) | ((low_bits.wrapping_sub(1)) & 0x3)) + 1;
        ch.pending_reset = true;
        ch.first_sample = true;
        ch.enabled = ch.dac_enabled;
        ch.envelope.volume = ch.envelope.initial;
        let mut env_timer = if ch.envelope.period == 0 {
            8
        } else {
            ch.envelope.period
        };
        if (self.sequencer.step + 1) & 7 == 7 {
            env_timer = env_timer.wrapping_add(1);
        }
        ch.envelope.timer = env_timer;
        let mut freq_updated = false;
        if idx == 1 {
            if let Some(s) = ch.sweep.as_mut() {
                s.reload(ch.frequency);
                if s.shift != 0 {
                    let new_freq = s.calculate();
                    if new_freq > 2047 {
                        ch.enabled = false;
                        s.enabled = false;
                    } else {
                        if s.negate {
                            s.neg_used = true;
                        }
                        s.shadow = new_freq;
                        ch.frequency = new_freq;
                        freq_updated = true;
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
        if idx == 1 && freq_updated {
            self.update_ch1_freq_regs();
        }
    }

    fn trigger_wave(&mut self) {
        if self.ch3.enabled {
            let byte_index = (self.ch3.position / 2) as usize;
            if byte_index < 4 {
                let val = self.wave_ram[byte_index];
                self.wave_ram[0] = val;
            } else {
                let base = byte_index & !0x03;
                let mut i = 0;
                while i < 4 {
                    self.wave_ram[i] = self.wave_ram[base + i];
                    i += 1;
                }
            }
        }
        self.ch3.enabled = self.ch3.dac_enabled;
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
        self.ch4.enabled = self.ch4.dac_enabled;
        // When the noise channel is triggered the LFSR is cleared to 0 as
        // described in Pan Docs.
        self.ch4.lfsr = 0;
        self.ch4.timer = self.ch4.period();
        self.ch4.envelope.volume = self.ch4.envelope.initial;
        let mut env_timer = if self.ch4.envelope.period == 0 {
            8
        } else {
            self.ch4.envelope.period
        };
        if (self.sequencer.step + 1) & 7 == 7 {
            env_timer = env_timer.wrapping_add(1);
        }
        self.ch4.envelope.timer = env_timer;
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
            self.update_ch1_freq_regs();
        }
        if step == 7 {
            self.ch1.envelope.clock();
            self.ch2.envelope.clock();
            self.ch4.envelope.clock();
        }
    }

    /// Tick the APU once per CPU cycle. `div_prev` is the DIV value at the
    /// beginning of the current machine step. In normal speed a machine step
    /// spans four CPU cycles; in double-speed it spans two.
    pub fn tick(&mut self, div_prev: u16, _div_now: u16, double_speed: bool) {
        // Store the current CPU speed so trigger_square can select the
        // correct initial delay when a channel is triggered.
        self.double_speed = double_speed;
        let ticks = if double_speed { 2 } else { 4 };
        for i in 0..ticks {
            // Advance the 1 MHz sample pipeline for both square channels.
            self.ch1.tick_1mhz();
            self.ch2.tick_1mhz();

            // Determine if the frame sequencer should step. The sequencer is
            // clocked by DIV bit 4 (bit 5 in double speed) on its falling edge.
            // `div_prev` contains the internal 16-bit divider value; DIV's bit
            // 4 corresponds to bit 12 of this counter. Likewise, in double
            // speed mode bit 5 corresponds to bit 13. We derive intermediate
            // DIV values by incrementing `div_prev`.
            let prev = div_prev.wrapping_add(i as u16);
            let curr = div_prev.wrapping_add((i + 1) as u16);
            let bit = if double_speed { 13 } else { 12 };
            let prev_bit = (prev >> bit) & 1;
            let curr_bit = (curr >> bit) & 1;
            if prev_bit == 1 && curr_bit == 0 {
                let step = self.sequencer.advance();
                self.clock_frame_sequencer(step);
            }

            // Update PCM12/PCM34 after each 1 MHz tick.
            self.refresh_pcm_regs();
            self.lf_div_counter = self.lf_div_counter.wrapping_add(1);
        }
        // cpu_cycles remains a CPU cycle counter for timers and IRQs.
        self.cpu_cycles = self.cpu_cycles.wrapping_add(1);
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

    /// Mirror the current channel 1 frequency into NR13/NR14.
    fn update_ch1_freq_regs(&mut self) {
        let freq = self.ch1.frequency;
        self.regs[0x03] = (freq & 0xFF) as u8;
        self.regs[0x04] = (self.regs[0x04] & !0x07) | ((freq >> 8) as u8 & 0x07);
    }

    pub fn step(&mut self, cycles: u16) {
        let cps = CPU_CLOCK_HZ / self.sample_rate;
        for _ in 0..cycles {
            self.cpu_cycles = self.cpu_cycles.wrapping_add(1);
            self.ch1.step(1);
            self.ch2.step(1);
            self.ch3.step(1, &self.wave_ram);
            self.ch4.step(1);
            self.sample_timer += 1;
            if self.sample_timer >= cps {
                self.sample_timer -= cps;
                let (left, right) = self.mix_output();
                self.push_sample(left);
                self.push_sample(right);
            }
        }
    }

    fn mix_output(&mut self) -> (i16, i16) {
        let out1 = self.ch1.current_sample();
        let out2 = self.ch2.current_sample();
        let out3 = self.ch3.output();
        let out4 = self.ch4.output();

        let ch1 = 8 - out1 as i16;
        let ch2 = 8 - out2 as i16;
        let ch3 = 8 - out3 as i16;
        let ch4 = 8 - out4 as i16;

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

    /// Current duty setting for channel 1.
    pub fn ch1_duty(&self) -> u8 {
        self.ch1.duty
    }

    /// Current duty step position for channel 1.
    pub fn ch1_duty_pos(&self) -> u8 {
        self.ch1.duty_pos
    }

    /// Current length counter value for channel 1.
    pub fn ch1_length(&self) -> u8 {
        self.ch1.length
    }

    /// Current envelope volume for channel 1.
    pub fn ch1_volume(&self) -> u8 {
        self.ch1.envelope.volume
    }

    /// Current envelope timer value for channel 1.
    pub fn ch1_envelope_timer(&self) -> u8 {
        self.ch1.envelope.timer
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

    pub fn ch1_timer(&self) -> i32 {
        self.ch1.timer
    }

    /// Current sweep shadow register value for channel 1.
    pub fn ch1_sweep_shadow(&self) -> u16 {
        self.ch1.sweep.as_ref().map(|s| s.shadow).unwrap_or(0)
    }

    /// Current sweep timer value for channel 1.
    pub fn ch1_sweep_timer(&self) -> u8 {
        self.ch1.sweep.as_ref().map(|s| s.timer).unwrap_or(0)
    }

    /// Whether channel 1 sweep is currently enabled.
    pub fn ch1_sweep_enabled(&self) -> bool {
        self.ch1.sweep.as_ref().map(|s| s.enabled).unwrap_or(false)
    }

    pub fn ch2_frequency(&self) -> u16 {
        self.ch2.frequency
    }

    /// Current duty setting for channel 2.
    pub fn ch2_duty(&self) -> u8 {
        self.ch2.duty
    }

    /// Current duty step position for channel 2.
    pub fn ch2_duty_pos(&self) -> u8 {
        self.ch2.duty_pos
    }

    /// Current length counter value for channel 2.
    pub fn ch2_length(&self) -> u8 {
        self.ch2.length
    }

    /// Current envelope volume for channel 2.
    pub fn ch2_volume(&self) -> u8 {
        self.ch2.envelope.volume
    }

    /// Current envelope timer value for channel 2.
    pub fn ch2_envelope_timer(&self) -> u8 {
        self.ch2.envelope.timer
    }

    pub fn ch2_timer(&self) -> i32 {
        self.ch2.timer
    }

    /// Current length counter value for channel 3.
    pub fn ch3_length(&self) -> u16 {
        self.ch3.length
    }

    /// Current frequency setting for channel 3.
    pub fn ch3_frequency(&self) -> u16 {
        self.ch3.frequency
    }

    /// Current period timer for channel 3.
    pub fn ch3_timer(&self) -> i32 {
        self.ch3.timer
    }

    /// Current playback position within wave RAM for channel 3.
    pub fn ch3_position(&self) -> u8 {
        self.ch3.position
    }

    /// Current length counter value for channel 4.
    pub fn ch4_length(&self) -> u8 {
        self.ch4.length
    }

    /// Current envelope volume for channel 4.
    pub fn ch4_volume(&self) -> u8 {
        self.ch4.envelope.volume
    }

    /// Current envelope timer for channel 4.
    pub fn ch4_envelope_timer(&self) -> u8 {
        self.ch4.envelope.timer
    }

    /// Current LFSR state for channel 4.
    pub fn ch4_lfsr(&self) -> u16 {
        self.ch4.lfsr
    }

    /// Current period timer for channel 4.
    pub fn ch4_timer(&self) -> i32 {
        self.ch4.timer
    }

    /// Current clock shift setting for channel 4.
    pub fn ch4_clock_shift(&self) -> u8 {
        self.ch4.clock_shift
    }

    /// Current divisor ratio for channel 4.
    pub fn ch4_divisor(&self) -> u8 {
        self.ch4.divisor
    }

    /// Whether channel 4 is using width-7 mode.
    pub fn ch4_width7(&self) -> bool {
        self.ch4.width7
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_output_combines_channels() {
        let mut apu = Apu::new();
        apu.nr50 = 0x00; // volume code 0 => factor 1
        apu.nr51 = 0xFF; // route all channels to both outputs

        apu.ch1.enabled = true;
        apu.ch1.dac_enabled = true;
        apu.ch1.out_latched = 15;

        apu.ch2.enabled = true;
        apu.ch2.dac_enabled = true;
        apu.ch2.out_latched = 15;

        apu.ch3.enabled = true;
        apu.ch3.dac_enabled = true;
        apu.ch3.last_sample = 15;
        apu.ch3.volume = 1;

        apu.ch4.enabled = true;
        apu.ch4.dac_enabled = true;
        apu.ch4.lfsr = 0;
        apu.ch4.envelope.volume = 15;

        let (left, right) = apu.mix_output();
        assert_eq!(left, right);
        assert_eq!(left, -28 * VOLUME_FACTOR);
    }

    #[test]
    fn mix_output_negative_slope() {
        let mut apu = Apu::new();
        apu.nr50 = 0x00;
        apu.nr51 = 0x11; // route ch1 to left and right
        apu.ch1.enabled = true;
        apu.ch1.dac_enabled = true;

        apu.ch1.out_latched = 0;
        let (left_pos, _) = apu.mix_output();

        apu.ch1.out_latched = 15;
        let (left_neg, _) = apu.mix_output();

        assert!(left_pos > 0);
        assert!(left_neg < 0);
    }

    #[test]
    fn mix_output_zero_when_dac_off() {
        let mut apu = Apu::new();
        apu.nr50 = 0x00;
        apu.nr51 = 0x11;
        apu.ch1.enabled = true;
        apu.ch1.dac_enabled = false;
        apu.ch1.out_latched = 8;
        let (left, right) = apu.mix_output();
        assert_eq!(left, 0);
        assert_eq!(right, 0);
    }

    #[test]
    fn dc_filter_reduces_constant_input() {
        let mut apu = Apu::new();
        let first = apu.dc_block(1000, 1000);
        let second = apu.dc_block(1000, 1000);
        assert!(second.0 < first.0);
        assert!(second.1 < first.1);
    }

    #[test]
    fn dc_filter_converges_to_zero() {
        let mut apu = Apu::new();
        let mut out = (0i16, 0i16);
        for _ in 0..8192 {
            out = apu.dc_block(1000, 1000);
        }
        assert!(out.0.abs() < 10);
        assert!(out.1.abs() < 10);
    }

    #[test]
    fn dc_filter_channels_independent() {
        let mut apu = Apu::new();
        let mut last_left = 0i16;
        let mut last_right = 0i16;
        for _ in 0..8 {
            let (l, r) = apu.dc_block(1000, 0);
            last_left = l;
            last_right = r;
        }
        assert!(last_left > 0);
        assert_eq!(last_right, 0);
    }
}
