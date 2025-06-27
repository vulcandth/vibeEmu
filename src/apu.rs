// src/apu.rs
#![allow(clippy::too_many_arguments)]

use std::collections::VecDeque;

/// CPU master clock (DMG) – 4 194 304 Hz
const CPU_CLOCK_HZ: u32 = 4_194_304;
/// 512 Hz frame sequencer period (8192 cycles)
const FRAME_SEQ_PERIOD: u32 = 8192;
/// Output gain chosen so that ‑128…+127 ≈ ‑1 dBFS at 44100 Hz
const VOLUME_SCALER: i16 = 64;
/// Max buffered samples ≈ 40 ms (stereo)
pub const AUDIO_LATENCY_MS: u32 = 40;
const MAX_SAMPLES: usize = ((44_100 * AUDIO_LATENCY_MS as usize) / 1_000) * 2;

/// Power‑on HW register image (DMG ‑ all channels disabled, DACs off)
const POWER_ON_REGS: [u8; 0x30] = [
    0x80, 0xBF, 0xF3, 0xFF, 0xBF, 0xFF, 0x3F, 0x00, 0xFF, 0xBF, 0x7F, 0xFF, 0x9F, 0xFF, 0xBF, 0xFF,
    0xFF, 0x00, 0x00, 0xBF, 0x77, 0xF3, 0xF1, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[inline]
fn dac_enabled(nrx2: u8) -> bool {
    nrx2 & 0xF8 != 0
}

#[inline]
fn trigger_square_internal(ch: &mut Square, dac_reg: u8, max_len: u8) {
    ch.enabled = dac_enabled(dac_reg);
    if ch.length == 0 {
        ch.length = max_len;
    }
    ch.pending_reset = true;
    ch.first_sample = true;
    ch.timer = square_period(ch.frequency);
    ch.envelope.timer = if ch.envelope.period == 0 {
        8
    } else {
        ch.envelope.period
    };
}

/* -------------------------------------------------------------------------- */
/*                               Envelope unit                                */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy, Default)]
struct Envelope {
    volume: u8,
    period: u8,
    add: bool,
    timer: u8,
}

impl Envelope {
    fn load(&mut self, data: u8) {
        self.period = data & 0x07;
        self.add = data & 0x08 != 0;
        self.volume = data >> 4;
        self.timer = if self.period == 0 { 8 } else { self.period };
    }
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
}

/* -------------------------------------------------------------------------- */
/*                              Channel 1 – sweep                             */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy, Default)]
struct Sweep {
    period: u8,
    negate: bool,
    shift: u8,
    timer: u8,
    shadow: u16,
    enabled: bool,
    /// The hardware remembers if it has produced a negative result once.
    /// When it does, further triggers force‑mute CH1 even if later positive.
    ever_negated: bool,
}

impl Sweep {
    fn load(&mut self, data: u8) {
        self.period = (data >> 4) & 0x07;
        self.negate = data & 0x08 != 0;
        self.shift = data & 0x07;
        self.enabled = self.period != 0 || self.shift != 0;
        self.timer = if self.period == 0 { 8 } else { self.period };
    }
    #[inline]
    fn calc(&self, freq: u16) -> Option<u16> {
        let delta = freq >> self.shift;
        let res = if self.negate {
            freq.wrapping_sub(delta)
        } else {
            freq.wrapping_add(delta)
        };
        (res <= 2047).then_some(res)
    }
    fn clock(&mut self, ch_freq: &mut u16, ch_enable: &mut bool) {
        if !self.enabled || self.shift == 0 {
            return;
        }
        if self.timer > 0 {
            self.timer -= 1;
        }
        if self.timer == 0 {
            self.timer = if self.period == 0 { 8 } else { self.period };
            if let Some(new_f) = self.calc(self.shadow) {
                self.shadow = new_f;
                *ch_freq = new_f;
                // Second overflow test
                if self.calc(self.shadow).is_none() {
                    *ch_enable = false;
                }
                if self.negate {
                    self.ever_negated = true;
                }
            } else {
                *ch_enable = false;
            }
        }
    }
}

/* -------------------------------------------------------------------------- */
/*                        Shared helpers for period counters                  */
/* -------------------------------------------------------------------------- */

#[inline]
fn square_period(freq: u16) -> i32 {
    ((2048 - freq) * 4) as i32
}
#[inline]
fn wave_period(freq: u16) -> i32 {
    ((2048 - freq) * 2) as i32
}
#[inline]
fn noise_period(clock_shift: u8, divisor_code: u8) -> i32 {
    let r = match divisor_code {
        0 => 8,
        _ => (divisor_code as i32) * 16,
    };
    r << clock_shift
}

/* -------------------------------------------------------------------------- */
/*                         Channel 1 & 2 – square wave                        */
/* -------------------------------------------------------------------------- */

#[derive(Clone)]
struct Square {
    // state
    enabled: bool,
    length: u8,
    length_en: bool,
    duty: u8,
    duty_pos: u8,
    frequency: u16,
    timer: i32,
    envelope: Envelope,
    // sweep (CH1 only)
    sweep: Option<Sweep>,
    // double‑latch to emulate 2‑stage pipeline feeding PCM regs/HPF
    stage1: u8,
    latched: u8,
    first_sample: bool,
    pending_reset: bool,
}

impl Square {
    fn new(with_sweep: bool) -> Self {
        Self {
            enabled: false,
            length: 0,
            length_en: false,
            duty: 0,
            duty_pos: 0,
            frequency: 0,
            timer: 0,
            envelope: Envelope::default(),
            sweep: with_sweep.then_some(Sweep::default()),
            stage1: 0,
            latched: 0,
            first_sample: true,
            pending_reset: false,
        }
    }
    #[inline]
    fn duty_bit(&self) -> u8 {
        // 12.5, 25, 50, 75 %
        const TABLE: [[u8; 8]; 4] = [
            [0, 1, 0, 0, 0, 0, 0, 0],
            [0, 1, 1, 0, 0, 0, 0, 0],
            [0, 1, 1, 1, 1, 0, 0, 0],
            [1, 0, 0, 1, 1, 1, 1, 1],
        ];
        TABLE[self.duty as usize][self.duty_pos as usize]
    }
    fn output_now(&self) -> u8 {
        if !self.enabled {
            return 0;
        }
        self.duty_bit() * self.envelope.volume
    }
    fn clock_timer(&mut self, cycles: u32) {
        let mut remaining = cycles as i32;
        while self.timer <= remaining {
            remaining -= self.timer;
            self.timer = square_period(self.frequency);
            if self.pending_reset {
                self.pending_reset = false;
                self.duty_pos = 0;
            } else {
                self.duty_pos = (self.duty_pos + 1) & 7;
            }
        }
        self.timer -= remaining;
    }
    fn clock_length(&mut self) {
        if self.length_en && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }
}

/* -------------------------------------------------------------------------- */
/*                       Channel 3 – wave playback (DMA)                      */
/* -------------------------------------------------------------------------- */

#[derive(Clone)]
struct Wave {
    enabled: bool,
    dac_on: bool,
    length: u16,
    length_en: bool,
    volume_code: u8,
    pos: u8,
    last_sample: u8,
    frequency: u16,
    timer: i32,
}

impl Default for Wave {
    fn default() -> Self {
        Self {
            enabled: false,
            dac_on: true,
            length: 0,
            length_en: false,
            volume_code: 0,
            pos: 0,
            last_sample: 0,
            frequency: 0,
            timer: 0,
        }
    }
}

impl Wave {
    #[inline]
    fn volume(&self, sample: u8) -> u8 {
        match self.volume_code {
            0 => 0,
            1 => sample,
            2 => sample >> 1,
            3 => sample >> 2,
            _ => 0,
        }
    }
    fn clock_timer(&mut self, cycles: u32, wave_ram: &[u8; 0x10]) {
        if !self.enabled || !self.dac_on {
            return;
        }
        let mut remaining = cycles as i32;
        while self.timer <= remaining {
            remaining -= self.timer;
            self.timer = wave_period(self.frequency);
            self.pos = (self.pos + 1) & 0x1F;
            let byte = wave_ram[(self.pos >> 1) as usize];
            self.last_sample = if self.pos & 1 == 0 {
                byte >> 4
            } else {
                byte & 0x0F
            };
        }
        self.timer -= remaining;
    }
    fn clock_length(&mut self) {
        if self.length_en && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }
    #[inline]
    fn output_now(&self) -> u8 {
        if !self.enabled || !self.dac_on {
            0
        } else {
            self.volume(self.last_sample)
        }
    }
}

/* -------------------------------------------------------------------------- */
/*                       Channel 4 – polynomial counter                       */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Default)]
struct Noise {
    enabled: bool,
    length: u8,
    length_en: bool,
    envelope: Envelope,
    clock_shift: u8,
    divisor_code: u8,
    width7: bool,
    lfsr: u16,
    timer: i32,
}

impl Noise {
    fn clock_timer(&mut self, cycles: u32) {
        if !self.enabled {
            return;
        }
        let mut remaining = cycles as i32;
        while self.timer <= remaining {
            remaining -= self.timer;
            self.timer = noise_period(self.clock_shift, self.divisor_code);
            let bit = (self.lfsr & 1) ^ ((self.lfsr >> 1) & 1);
            self.lfsr = (self.lfsr >> 1) | (bit << 14);
            if self.width7 {
                self.lfsr = (self.lfsr & !0x40) | (bit << 6);
            }
        }
        self.timer -= remaining;
    }
    fn clock_length(&mut self) {
        if self.length_en && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }
    #[inline]
    fn output_now(&self) -> u8 {
        if !self.enabled {
            0
        } else if self.lfsr & 1 == 0 {
            self.envelope.volume
        } else {
            0
        }
    }
}

/* -------------------------------------------------------------------------- */
/*                                  A P U                                     */
/* -------------------------------------------------------------------------- */

pub struct Apu {
    ch1: Square,
    ch2: Square,
    ch3: Wave,
    ch4: Noise,

    wave_ram: [u8; 0x10],
    regs: [u8; 0x30], // NR10–NR3F shadow (for reads)
    nr50: u8,
    nr51: u8,
    nr52: u8,

    sequencer_step: u8,
    div_prev_bit: bool,

    // sample generation
    sample_timer: u32,
    sample_rate: u32,
    hpf_l_in: f32,
    hpf_l_out: f32,
    hpf_r_in: f32,
    hpf_r_out: f32,
    samples: VecDeque<i16>,

    // cached PCM regs (CGB)
    pcm12: u8,
    pcm34: u8,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    pub fn new() -> Self {
        let mut apu = Self {
            ch1: Square::new(true),
            ch2: Square::new(false),
            ch3: Wave::default(),
            ch4: Noise::default(),
            wave_ram: [0; 0x10],
            regs: POWER_ON_REGS,
            nr50: 0x77,
            nr51: 0xF3,
            nr52: 0xF1, // Power on + CH3 DAC enabled
            sequencer_step: 0,
            div_prev_bit: false,
            sample_timer: 0,
            sample_rate: 44_100,
            hpf_l_in: 0.0,
            hpf_l_out: 0.0,
            hpf_r_in: 0.0,
            hpf_r_out: 0.0,
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            pcm12: 0,
            pcm34: 0,
        };
        // set defaults that are not 0 in hardware
        apu.ch1.duty = 2;
        apu.ch1.length = 0x3F;
        apu.ch1.envelope.volume = 0x0F;
        apu.ch1.envelope.period = 3;
        apu.ch1.frequency = 0x03FF;
        apu.ch1.enabled = false;
        apu.ch1.sweep.as_mut().unwrap().shadow = 0x03FF;

        apu
    }

    /* ------------------------------- I/O ---------------------------------- */

    fn powered(&self) -> bool {
        self.nr52 & 0x80 != 0
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        if !self.powered() && addr != 0xFF26 {
            return 0xFF;
        }
        match addr {
            // PCM read‑backs (CGB)
            0xFF76 => self.pcm12,
            0xFF77 => self.pcm34,
            // Wave RAM
            0xFF30..=0xFF3F => {
                if self.ch3.enabled && self.ch3.dac_on {
                    0xFF
                } else {
                    self.wave_ram[(addr - 0xFF30) as usize]
                }
            }
            // NR52 – status
            0xFF26 => {
                let ch_flags = (self.ch4.enabled as u8) << 3
                    | (self.ch3.enabled as u8) << 2
                    | (self.ch2.enabled as u8) << 1
                    | (self.ch1.enabled as u8);
                (self.nr52 & 0x80) | ch_flags | 0x70
            }
            // Other audio regs
            0xFF10..=0xFF3F => {
                let i = (addr - 0xFF10) as usize;
                self.regs[i] | Self::read_mask(addr)
            }
            _ => 0xFF,
        }
    }

    pub fn read_pcm(&self, addr: u16) -> u8 {
        match addr {
            0xFF76 => self.pcm12,
            0xFF77 => self.pcm34,
            _ => 0xFF,
        }
    }

    pub fn write_reg(&mut self, addr: u16, data: u8) {
        // NR52 power control -------------------------------------------------
        if addr == 0xFF26 {
            let prev_on = self.powered();
            if data & 0x80 == 0 && prev_on {
                // Power‑off – clear everything except NR52 & wave RAM
                *self = Self::new(); // safe because the interface is unchanged
                self.nr52 = 0; // keep powered‑off state
            } else if data & 0x80 != 0 && !prev_on {
                // Power‑on – restore reset values
                *self = Self::new();
            }
            return;
        }
        if !self.powered() {
            return; // while off, only NR52 & wave‑RAM are writable (Wave RAM handled below)
        }

        // Wave RAM -----------------------------------------------------------
        if (0xFF30..=0xFF3F).contains(&addr) {
            if !(self.ch3.enabled && self.ch3.dac_on) {
                let idx = (addr - 0xFF30) as usize;
                self.wave_ram[idx] = data;
            }
            return;
        }

        // Shadow register file (for reads)
        if (0xFF10..=0xFF3F).contains(&addr) {
            self.regs[(addr - 0xFF10) as usize] = data;
        }

        match addr {
            /* ---------------- Channel 1 – square + sweep ----------------- */
            0xFF10 => {
                // NR10 sweep
                if let Some(sw) = &mut self.ch1.sweep {
                    sw.load(data);
                }
            }
            0xFF11 => {
                // NR11 duty/length
                self.ch1.duty = data >> 6;
                self.ch1.length = 64 - (data & 0x3F);
            }
            0xFF12 => {
                // NR12 envelope
                self.ch1.envelope.load(data);
                if !dac_enabled(data) {
                    self.ch1.enabled = false;
                }
            }
            0xFF13 => self.ch1.frequency = (self.ch1.frequency & 0x700) | data as u16,
            0xFF14 => {
                self.ch1.length_en = data & 0x40 != 0;
                self.ch1.frequency = (self.ch1.frequency & 0xFF) | (((data & 0x07) as u16) << 8);
                if data & 0x80 != 0 {
                    self.trigger_ch1();
                }
            }

            /* ---------------- Channel 2 – simple square ------------------ */
            0xFF16 => {
                self.ch2.duty = data >> 6;
                self.ch2.length = 64 - (data & 0x3F);
            }
            0xFF17 => {
                self.ch2.envelope.load(data);
                if !dac_enabled(data) {
                    self.ch2.enabled = false;
                }
            }
            0xFF18 => self.ch2.frequency = (self.ch2.frequency & 0x700) | data as u16,
            0xFF19 => {
                self.ch2.length_en = data & 0x40 != 0;
                self.ch2.frequency = (self.ch2.frequency & 0xFF) | (((data & 0x07) as u16) << 8);
                if data & 0x80 != 0 {
                    trigger_square_internal(&mut self.ch2, self.regs[0x17 - 0x10], 64);
                }
            }

            /* ---------------- Channel 3 – wave --------------------------- */
            0xFF1A => self.ch3.dac_on = data & 0x80 != 0,
            0xFF1B => self.ch3.length = 256 - data as u16,
            0xFF1C => self.ch3.volume_code = (data >> 5) & 0x03,
            0xFF1D => self.ch3.frequency = (self.ch3.frequency & 0x700) | data as u16,
            0xFF1E => {
                self.ch3.length_en = data & 0x40 != 0;
                self.ch3.frequency = (self.ch3.frequency & 0xFF) | (((data & 0x07) as u16) << 8);
                if data & 0x80 != 0 {
                    self.trigger_wave();
                }
            }

            /* ---------------- Channel 4 – noise -------------------------- */
            0xFF20 => self.ch4.length = 64 - (data & 0x3F),
            0xFF21 => {
                self.ch4.envelope.load(data);
                if !dac_enabled(data) {
                    self.ch4.enabled = false;
                }
            }
            0xFF22 => {
                self.ch4.clock_shift = data >> 4;
                self.ch4.width7 = data & 0x08 != 0;
                self.ch4.divisor_code = data & 0x07;
            }
            0xFF23 => {
                self.ch4.length_en = data & 0x40 != 0;
                if data & 0x80 != 0 {
                    self.trigger_noise();
                }
            }

            /* ---------------- Mixer & volume ----------------------------- */
            0xFF24 => self.nr50 = data,
            0xFF25 => self.nr51 = data,

            _ => {}
        }
    }

    /* ---------------------------- Triggers -------------------------------- */

    fn trigger_ch1(&mut self) {
        let sweep = self.ch1.sweep.as_mut().unwrap();
        sweep.shadow = self.ch1.frequency;
        sweep.timer = if sweep.period == 0 { 8 } else { sweep.period };
        sweep.enabled = sweep.period != 0 || sweep.shift != 0;
        sweep.ever_negated = false;

        trigger_square_internal(&mut self.ch1, self.regs[0x12 - 0x10], 64);
    }
    fn trigger_wave(&mut self) {
        self.ch3.enabled = self.ch3.dac_on;
        self.ch3.pos = 0;
        self.ch3.timer = wave_period(self.ch3.frequency);
        if self.ch3.length == 0 {
            self.ch3.length = 256;
        }
    }
    fn trigger_noise(&mut self) {
        self.ch4.enabled = dac_enabled(self.regs[0x21 - 0x10]);
        self.ch4.lfsr = 0x7FFF;
        self.ch4.timer = noise_period(self.ch4.clock_shift, self.ch4.divisor_code);
        if self.ch4.length == 0 {
            self.ch4.length = 64;
        }
        self.ch4.envelope.timer = if self.ch4.envelope.period == 0 {
            8
        } else {
            self.ch4.envelope.period
        };
    }

    /* ------------------------------ Ticking ------------------------------- */

    /// Advance 1 M‑cycle (4 CPU ticks) – called from `Timer::step`, `Cpu::tick`, **etc.**
    pub fn tick(&mut self, _div_prev: u16, div_now: u16, double_speed: bool) {
        // Frame sequencer clock (512 Hz)
        let bit = if double_speed { 6 } else { 5 };
        let new_bit = (div_now >> bit) & 1 != 0;
        if self.div_prev_bit && !new_bit {
            self.step_frame_sequencer();
        }
        self.div_prev_bit = new_bit;

        // 1 MHz pipeline latch for CH1/CH2
        self.ch1.latched = self.ch1.stage1;
        self.ch1.stage1 = self.ch1.output_now();
        self.ch2.latched = self.ch2.stage1;
        self.ch2.stage1 = self.ch2.output_now();

        // Update CGB PCM read‑backs each M‑cycle
        self.pcm12 = (self.ch2.latched << 4) | self.ch1.latched;
        self.pcm34 = (self.ch4.output_now() << 4) | self.ch3.output_now();
    }

    fn step_frame_sequencer(&mut self) {
        match self.sequencer_step {
            0 | 2 | 4 | 6 => {
                self.ch1.clock_length();
                self.ch2.clock_length();
                self.ch3.clock_length();
                self.ch4.clock_length();
            }
            _ => {}
        }
        if self.sequencer_step == 2 || self.sequencer_step == 6 {
            if let Some(sw) = &mut self.ch1.sweep {
                sw.clock(&mut self.ch1.frequency, &mut self.ch1.enabled);
            }
        }
        if self.sequencer_step == 7 {
            self.ch1.envelope.clock();
            self.ch2.envelope.clock();
            self.ch4.envelope.clock();
        }
        self.sequencer_step = (self.sequencer_step + 1) & 7;
    }

    /// Advance the actual generator timers (`cycles` CPU cycles, not M‑cycles)
    pub fn step(&mut self, cycles: u16) {
        let c = cycles as u32;
        self.ch1.clock_timer(c);
        self.ch2.clock_timer(c);
        self.ch3.clock_timer(c, &self.wave_ram);
        self.ch4.clock_timer(c);

        // sample down‑sampling (simple nearest)
        self.sample_timer += c;
        let cps = CPU_CLOCK_HZ / self.sample_rate;
        while self.sample_timer >= cps {
            self.sample_timer -= cps;
            let (l, r) = self.mix();
            self.push_sample(l);
            self.push_sample(r);
        }
    }

    /* ---------------------- Mixing & high‑pass filter --------------------- */

    #[inline]
    fn mix(&mut self) -> (i16, i16) {
        let c1 = (self.ch1.latched as i16) - 8;
        let c2 = (self.ch2.latched as i16) - 8;
        let c3 = (self.ch3.output_now() as i16) - 8;
        let c4 = (self.ch4.output_now() as i16) - 8;
        let vin = 0; // no external VIN line attached

        let mut left = 0i16;
        let mut right = 0i16;
        // NR51 panning
        if self.nr51 & 0x01 != 0 {
            right += c1;
        }
        if self.nr51 & 0x02 != 0 {
            right += c2;
        }
        if self.nr51 & 0x04 != 0 {
            right += c3;
        }
        if self.nr51 & 0x08 != 0 {
            right += c4;
        }

        if self.nr51 & 0x10 != 0 {
            left += c1;
        }
        if self.nr51 & 0x20 != 0 {
            left += c2;
        }
        if self.nr51 & 0x40 != 0 {
            left += c3;
        }
        if self.nr51 & 0x80 != 0 {
            left += c4;
        }

        // VIN mixing (bits 3 & 7 of NR50)
        if self.nr50 & 0x08 != 0 {
            right += vin;
        }
        if self.nr50 & 0x80 != 0 {
            left += vin;
        }

        // Master volumes (0‑7) – plus 1 as analogue circuit never mutes fully
        let rv = ((self.nr50 & 0x07) + 1) as i16;
        let lv = (((self.nr50 >> 4) & 0x07) + 1) as i16;
        left *= lv * VOLUME_SCALER;
        right *= rv * VOLUME_SCALER;

        self.dc_block(left, right)
    }

    #[inline]
    fn dc_block(&mut self, l: i16, r: i16) -> (i16, i16) {
        // single‑pole HPF y[n] = x[n] - x[n-1] + R·y[n-1]
        const R: f32 = 0.999_94;
        let lin = l as f32;
        let rin = r as f32;
        let lout = lin - self.hpf_l_in + R * self.hpf_l_out;
        let rout = rin - self.hpf_r_in + R * self.hpf_r_out;
        self.hpf_l_in = lin;
        self.hpf_r_in = rin;
        self.hpf_l_out = lout;
        self.hpf_r_out = rout;
        (lout as i16, rout as i16)
    }

    #[inline]
    fn push_sample(&mut self, s: i16) {
        if self.samples.len() >= MAX_SAMPLES {
            let excess = self.samples.len() + 1 - MAX_SAMPLES;
            self.samples.drain(..excess);
        }
        self.samples.push_back(s);
    }

    /* --------------------------- Public helpers --------------------------- */

    pub fn pop_sample(&mut self) -> Option<i16> {
        self.samples.pop_front()
    }
    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate.max(8000);
    }

    pub fn set_speed(&mut self, _factor: f32) {
        // TODO: implement audio rate scaling with emulator speed
    }

    pub fn sequencer_step(&self) -> u8 {
        self.sequencer_step
    }

    /// Convenience for the debugger
    pub fn ch1_frequency(&self) -> u16 {
        self.ch1.frequency
    }

    #[inline]
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
}
