use std::collections::VecDeque;

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
        if self.period == 0 {
            return;
        }
        if self.timer == 0 {
            self.timer = self.period;
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
        self.timer = self.period;
    }
}

#[derive(Default)]
struct Sweep {
    period: u8,
    negate: bool,
    shift: u8,
    timer: u8,
    shadow: u16,
    enabled: bool,
}

impl Sweep {
    fn clock(&mut self, ch: &mut SquareChannel) {
        if !self.enabled || self.period == 0 {
            return;
        }
        if self.timer == 0 {
            self.timer = if self.period == 0 { 8 } else { self.period };
            let mut new_freq = self.calculate();
            if new_freq > 2047 {
                ch.enabled = false;
            } else if self.shift != 0 {
                self.shadow = new_freq;
                ch.frequency = new_freq;
                new_freq = self.calculate();
                if new_freq > 2047 {
                    ch.enabled = false;
                }
            }
        } else {
            self.timer -= 1;
        }
    }

    fn calculate(&self) -> u16 {
        let delta = self.shadow >> self.shift;
        if self.negate {
            self.shadow.wrapping_sub(delta)
        } else {
            self.shadow.wrapping_add(delta)
        }
    }

    fn reload(&mut self, val: u8, freq: u16) {
        self.period = (val >> 4) & 0x07;
        self.negate = val & 0x08 != 0;
        self.shift = val & 0x07;
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
            if !sweep.enabled || sweep.period == 0 {
                return;
            }
            if sweep.timer == 0 {
                sweep.timer = if sweep.period == 0 { 8 } else { sweep.period };
                let mut new_freq = sweep.calculate();
                if new_freq > 2047 {
                    self.enabled = false;
                } else if sweep.shift != 0 {
                    sweep.shadow = new_freq;
                    self.frequency = new_freq;
                    new_freq = sweep.calculate();
                    if new_freq > 2047 {
                        self.enabled = false;
                    }
                }
            } else {
                sweep.timer -= 1;
            }
        }
    }
}

#[derive(Default)]
struct WaveChannel {
    enabled: bool,
    dac_enabled: bool,
    length: u8,
    length_enable: bool,
    volume: u8,
    position: u8,
    frequency: u16,
    timer: i32,
}

impl WaveChannel {
    fn period(&self) -> i32 {
        ((2048 - self.frequency) * 2) as i32
    }

    fn step(&mut self, cycles: u32) {
        if !self.enabled || !self.dac_enabled {
            return;
        }
        let mut cycles = cycles as i32;
        while self.timer <= cycles {
            cycles -= self.timer;
            self.timer = self.period();
            self.position = (self.position + 1) & 0x1F;
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

    fn output(&self, wave_ram: &[u8; 0x10]) -> u8 {
        if !self.enabled || !self.dac_enabled {
            return 0;
        }
        let byte = wave_ram[(self.position / 2) as usize];
        let sample = if self.position & 1 == 0 {
            byte >> 4
        } else {
            byte & 0x0F
        };
        match self.volume {
            0 => 0,
            1 => sample,
            2 => sample >> 1,
            3 => sample >> 2,
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
}

impl Apu {
    pub fn new() -> Self {
        Self {
            ch1: SquareChannel::new(true),
            ch2: SquareChannel::new(false),
            ch3: WaveChannel::default(),
            ch4: NoiseChannel::default(),
            wave_ram: [0; 0x10],
            nr50: 0,
            nr51: 0,
            nr52: 0,
            sequencer: FrameSequencer::new(),
            seq_counter: 0,
            sample_timer: 0,
            sample_rate: 44100,
            samples: VecDeque::with_capacity(4096),
        }
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        match addr {
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
            0xFF1B => self.ch3.length,
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
            0xFF26 => self.nr52 | 0x70,
            0xFF30..=0xFF3F => self.wave_ram[(addr - 0xFF30) as usize],
            _ => 0xFF,
        }
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF10 => {
                if let Some(s) = self.ch1.sweep.as_mut() {
                    s.period = (val >> 4) & 0x07;
                    s.negate = val & 0x08 != 0;
                    s.shift = val & 0x07;
                }
            }
            0xFF11 => {
                self.ch1.duty = val >> 6;
                self.ch1.length = val & 0x3F;
            }
            0xFF12 => self.ch1.envelope.reset(val),
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
                self.ch2.length = val & 0x3F;
            }
            0xFF17 => self.ch2.envelope.reset(val),
            0xFF18 => self.ch2.frequency = (self.ch2.frequency & 0x700) | val as u16,
            0xFF19 => {
                self.ch2.length_enable = val & 0x40 != 0;
                self.ch2.frequency = (self.ch2.frequency & 0xFF) | (((val & 0x07) as u16) << 8);
                if val & 0x80 != 0 {
                    self.trigger_square(2);
                }
            }
            0xFF1A => self.ch3.dac_enabled = val & 0x80 != 0,
            0xFF1B => self.ch3.length = val,
            0xFF1C => self.ch3.volume = (val >> 5) & 0x03,
            0xFF1D => self.ch3.frequency = (self.ch3.frequency & 0x700) | val as u16,
            0xFF1E => {
                self.ch3.length_enable = val & 0x40 != 0;
                self.ch3.frequency = (self.ch3.frequency & 0xFF) | (((val & 0x07) as u16) << 8);
                if val & 0x80 != 0 {
                    self.trigger_wave();
                }
            }
            0xFF20 => self.ch4.length = val & 0x3F,
            0xFF21 => self.ch4.envelope.reset(val),
            0xFF22 => {
                self.ch4.clock_shift = val >> 4;
                self.ch4.width7 = val & 0x08 != 0;
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
                    self.ch1.enabled = false;
                    self.ch2.enabled = false;
                    self.ch3.enabled = false;
                    self.ch4.enabled = false;
                    self.samples.clear();
                } else {
                    self.nr52 |= 0x80;
                }
            }
            0xFF30..=0xFF3F => {
                self.wave_ram[(addr - 0xFF30) as usize] = val;
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
                s.reload(0, ch.frequency);
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
            self.ch3.length = 255;
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
        while self.seq_counter >= 8192 {
            self.seq_counter -= 8192;
            let step = self.sequencer.advance();
            self.clock_frame_sequencer(step);
        }
        self.ch1.step(cycles);
        self.ch2.step(cycles);
        self.ch3.step(cycles);
        self.ch4.step(cycles);
        // sample generation not yet implemented
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
