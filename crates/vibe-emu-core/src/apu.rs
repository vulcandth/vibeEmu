use std::collections::VecDeque;

use crate::hardware::CgbRevision;

#[cfg(feature = "apu-trace")]
macro_rules! apu_trace {
    ($($arg:tt)*) => {
        println!($($arg)*);
    };
}
#[cfg(not(feature = "apu-trace"))]
macro_rules! apu_trace {
    ($($arg:tt)*) => {};
}
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

// Duty table for pulse channels (CH1, CH2). Each entry is an 8-step
// waveform. Index (0..3) corresponds to duty selector in NRx1:
// 0 -> 00000001 (12.5%)
// 1 -> 10000001 (25%)
// 2 -> 10000111 (50%)
// 3 -> 01111110 (75%)
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5% -> 00000001
    [1, 0, 0, 0, 0, 0, 0, 1], // 25%   -> 10000001
    [1, 0, 0, 0, 0, 1, 1, 1], // 50%   -> 10000111
    [0, 1, 1, 1, 1, 1, 1, 0], // 75%   -> 01111110
];

#[derive(Default, Clone, Copy)]
struct EnvelopeClock {
    clock: bool,
    locked: bool,
    should_lock: bool,
}

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
    active: bool,
    length: u8,
    length_enable: bool,
    duty: u8,
    /// Next duty value written via NRx1; becomes effective only after the
    /// current sample finishes (at the next duty edge).
    duty_next: u8,
    duty_pos: u8,
    pending_reset: bool,
    frequency: u16,
    timer: i32,
    envelope: Envelope,
    sweep: Option<Sweep>,
    sample_length: u16,
    sample_countdown: i32,
    delay: i32,
    sample_surpressed: bool,
    just_reloaded: bool,
    did_tick: bool,
    out_latched: u8,
    out_stage1: u8,
    out_stage2: u8,
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

    fn write_duty(&mut self, duty: u8) {
        self.duty_next = duty & 0x03;
        // If the channel isn't active yet, update immediately so the
        // upcoming trigger uses the new duty right away.
        if !self.active {
            self.duty = self.duty_next;
        }
    }

    fn sample_countdown_from_length(length: u16) -> i32 {
        (((length ^ 0x07FF) as i32) * 2 + 1).max(1)
    }

    fn refresh_sample_length(&mut self) {
        self.sample_length = self.frequency & 0x07FF;
        if self.just_reloaded {
            self.sample_countdown = Self::sample_countdown_from_length(self.sample_length);
        }
    }

    fn write_frequency_low(&mut self, value: u8) {
        self.frequency = (self.frequency & 0x700) | value as u16;
        self.refresh_sample_length();
    }

    fn write_frequency_high(&mut self, value: u8) {
        self.frequency = (self.frequency & 0xFF) | (((value & 0x07) as u16) << 8);
        self.refresh_sample_length();
    }

    fn reset_sample_timing(&mut self) {
        self.sample_length = self.frequency & 0x07FF;
        self.sample_countdown = Self::sample_countdown_from_length(self.sample_length);
        self.delay = 0;
        self.just_reloaded = true;
        self.did_tick = false;
        self.sample_surpressed = true;
    }

    fn clock_2mhz(&mut self, mut cycles_left: i32) {
        if !self.enabled || !self.dac_enabled {
            self.just_reloaded = false;
            return;
        }
        if self.delay > 0 {
            // Delay is accounted for in initial sample_countdown at trigger.
            self.delay = 0;
        }
        while cycles_left > self.sample_countdown {
            // Advance to the next sample boundary
            let advance_2mhz = self.sample_countdown + 1;
            cycles_left -= advance_2mhz;
            // At each duty edge, reload the CPU-period timer to the current period.
            // Do not subtract any additional partial CPU cycles here; timer changes only on edges.
            self.timer = self.period();
            self.sample_countdown = SquareChannel::sample_countdown_from_length(self.sample_length);
            self.duty_pos = (self.duty_pos + 1) & 7;
            // Apply any pending duty change only after finishing the current sample.
            self.duty = self.duty_next;
            self.sample_surpressed = false;
            self.pending_reset = false;
            self.did_tick = true;
        }
        // Consume any remaining 2 MHz ticks (no boundary crossing); timer remains unchanged
        if cycles_left > 0 {
            self.sample_countdown -= cycles_left;
            if self.sample_countdown < 0 {
                self.sample_countdown = 0;
            }
        }
        self.just_reloaded = cycles_left == 0;
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
        if self.sample_surpressed {
            return 0;
        }
        let level = DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        level * self.envelope.volume
    }

    fn output(&mut self) -> u8 {
        self.compute_output()
    }

    /// Shift the 1 MHz staging pipeline by one step.
    ///
    /// `out_latched` captures the most recent duty output, `out_stage1` reflects the
    /// intermediate step, and `out_stage2` is the latched value consumed by the mixer
    /// (third-stage output produced by the pipeline).
    fn tick_1mhz(&mut self) {
        let sample = self.compute_output();
        self.out_stage2 = self.out_stage1;
        self.out_stage1 = self.out_latched;
        self.out_latched = sample;
        // Shift the 1 MHz staging pipeline by one step.
        // `out_latched` captures the most recent duty output, `out_stage1` reflects the
        // intermediate step, and `out_stage2` is the latched value consumed by the mixer
        // (third-stage output produced by the pipeline).
    }

    fn current_sample(&self) -> u8 {
        self.out_stage2
    }

    fn peek_sample(&self) -> u8 {
        // The PCM read path is not gated by the channel's internal `pending_reset` flag.
        // Visibility is controlled only by DAC/enabled state and `sample_surpressed`.
        if !self.enabled || !self.dac_enabled || self.sample_surpressed {
            return 0;
        }
        let level = DUTY_TABLE[self.duty as usize][self.duty_pos as usize];
        level * self.envelope.volume
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
                self.active = false;
            }
        }
    }
    fn clock_sweep(&mut self) {
        let mut freq_changed = false;
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
                    freq_changed = true;
                    new_freq = sweep.calculate();
                    if new_freq > 2047 {
                        self.enabled = false;
                        sweep.enabled = false;
                    }
                }
            }
        }
        if freq_changed {
            self.refresh_sample_length();
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
    pcm_samples: [u8; 4],
    pcm_active: [bool; 4],
    pcm_mask: [u8; 2],
    speed_factor: f32,
    hp_coef: f32,
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
    /// Parity bit that toggles with 2 MHz ticks to align square channels to 1 MHz.
    /// Used for trigger delay calculations.
    // Minimal envelope clock/lock model (inspired by observed hardware behavior)
    ch1_env_clock: EnvelopeClock,
    ch2_env_clock: EnvelopeClock,
    ch4_env_clock: EnvelopeClock,
    // Divider used for envelope countdown scheduling
    div_divider: u32,
    ch1_env_countdown: u8,
    ch2_env_countdown: u8,
    ch4_env_countdown: u8,
    lf_div: u8,
    /// True when the CPU is in double-speed mode (KEY1 bit 0 set and prepared).
    double_speed: bool,
    ch1_last_env_write_cycle: u64,
    apu_enable_tick: u64,
    /// Accumulates CPU cycles to emit 2 MHz ticks (1 tick per 2 CPU cycles).
    mhz2_residual: i32,
    /// True if running in CGB mode; used for model-specific APU quirks.
    cgb_mode: bool,
    cgb_revision: CgbRevision,
}

impl Apu {
    // Keep <= 40 ms of stereo samples in the queue
    const MAX_SAMPLES: usize = ((44100 * AUDIO_LATENCY_MS as usize) / 1000) * 2;

    fn calc_hp_coef(rate: u32) -> f32 {
        0.999_958_f32.powf(4_194_304.0 / rate as f32)
    }

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
        self.pcm_samples = [0; 4];
        self.pcm_active = [false; 4];
        self.pcm_mask = [0xFF; 2];
        self.speed_factor = 1.0;
        self.hp_coef = Apu::calc_hp_coef(self.sample_rate);
        self.hp_prev_input_left = 0.0;
        self.hp_prev_output_left = 0.0;
        self.hp_prev_input_right = 0.0;
        self.hp_prev_output_right = 0.0;
        self.pcm12 = 0;
        self.pcm34 = 0;
        self.ch1_last_env_write_cycle = 0;
        self.apu_enable_tick = 0;
        self.mhz2_residual = 0;
        self.lf_div = 1;
        self.ch1_env_clock = EnvelopeClock::default();
        self.ch2_env_clock = EnvelopeClock::default();
        self.ch4_env_clock = EnvelopeClock::default();
        self.ch1_env_countdown = 0;
        self.ch2_env_countdown = 0;
        self.ch4_env_countdown = 0;
        self.div_divider = 0;
    }

    #[inline]
    fn set_envelope_clock(clock: &mut EnvelopeClock, value: bool, direction_add: bool, volume: u8) {
        if clock.clock == value {
            return;
        }
        if value {
            clock.clock = true;
            clock.should_lock =
                (volume == 0x0F && direction_add) || (volume == 0x00 && !direction_add);
        } else {
            clock.clock = false;
            if clock.should_lock {
                clock.locked = true;
            }
        }
    }

    /// Core of the NRX2 glitch logic, using the channel's envelope clock/lock state.
    fn nrx2_glitch_step(mut vol: u8, new_v: u8, old_v: u8, lock: &mut EnvelopeClock) -> u8 {
        let old_period = old_v & 0x07;
        let new_period = new_v & 0x07;
        let new_add = new_v & 0x08 != 0;
        // If the envelope clock is currently high, countdown would reload to new period.
        if lock.clock {
            // Our countdown is kept externally; observable effect is handled by secondary event.
        }
        let mut should_tick = (new_period != 0) && (old_period == 0) && !lock.locked;
        let should_invert = (new_v ^ old_v) & 0x08 != 0;

        if (new_v & 0x0F) == 0x08 && (old_v & 0x0F) == 0x08 && !lock.locked {
            should_tick = true;
        }

        if should_invert {
            if new_add {
                if old_period == 0 && !lock.locked {
                    vol ^= 0x0F;
                } else {
                    vol = 0x0Eu8.wrapping_sub(vol) & 0x0F;
                }
                // Prevent ticking after the special inversion
                should_tick = false;
            } else {
                vol = 0x10u8.wrapping_sub(vol) & 0x0F;
            }
        }

        if should_tick {
            if new_add {
                vol = vol.wrapping_add(1);
            } else {
                vol = vol.wrapping_sub(1);
            }
            vol &= 0x0F;
        } else if new_period == 0 && lock.clock {
            // Clear the envelope clock if period becomes 0 while clock is high.
            Apu::set_envelope_clock(lock, false, false, 0);
        }
        vol & 0x0F
    }

    /// Apply the NRX2 write glitch ("zombie mode") to the given current volume for a square channel.
    fn apply_nrx2_glitch_square(&mut self, ch: u8, vol: u8, old_val: u8, new_val: u8) -> u8 {
        let lock = if ch == 1 {
            &mut self.ch1_env_clock
        } else {
            &mut self.ch2_env_clock
        };
        // On pre CGB-D models (DMG and up to CGB-C) the glitch behaves as if two writes happen via 0xFF
        let is_old_model = !self.cgb_mode
            || matches!(
                self.cgb_revision,
                CgbRevision::Rev0 | CgbRevision::RevA | CgbRevision::RevB | CgbRevision::RevC
            );
        if is_old_model {
            let v1 = Apu::nrx2_glitch_step(vol, 0xFF, old_val, lock);
            Apu::nrx2_glitch_step(v1, new_val, 0xFF, lock)
        } else {
            Apu::nrx2_glitch_step(vol, new_val, old_val, lock)
        }
    }
    fn new_internal() -> Self {
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
            sample_rate: 44_100,
            samples: VecDeque::with_capacity(Self::MAX_SAMPLES),
            pcm_samples: [0; 4],
            pcm_active: [false; 4],
            pcm_mask: [0xFF; 2],
            speed_factor: 1.0,
            hp_coef: Apu::calc_hp_coef(44_100),
            hp_prev_input_left: 0.0,
            hp_prev_output_left: 0.0,
            hp_prev_input_right: 0.0,
            hp_prev_output_right: 0.0,
            pcm12: 0,
            pcm34: 0,
            cpu_cycles: 0,
            lf_div_counter: 0,
            lf_div: 1,
            double_speed: false,
            ch1_last_env_write_cycle: 0,
            apu_enable_tick: 0,
            mhz2_residual: 0,
            cgb_mode: false,
            cgb_revision: CgbRevision::default(),
            ch1_env_clock: EnvelopeClock::default(),
            ch2_env_clock: EnvelopeClock::default(),
            ch4_env_clock: EnvelopeClock::default(),
            div_divider: 0,
            ch1_env_countdown: 0,
            ch2_env_countdown: 0,
            ch4_env_countdown: 0,
        };

        // Initialize channels to power-on register defaults
        apu.ch1.duty = 2;
        apu.ch1.duty_next = 2;
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

    pub fn new() -> Self {
        Self::new_with_config(false, CgbRevision::default())
    }

    pub fn new_with_mode(cgb: bool) -> Self {
        Self::new_with_config(cgb, CgbRevision::default())
    }

    pub fn new_with_config(cgb: bool, revision: CgbRevision) -> Self {
        let mut apu = Self::new_internal();
        apu.cgb_mode = cgb;
        apu.cgb_revision = revision;
        apu.hp_coef = Apu::calc_hp_coef(apu.sample_rate);
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
        if !self.cgb_mode || self.nr52 & 0x80 == 0 {
            return 0xFF;
        }
        match addr {
            0xFF76 => self.pcm12,
            0xFF77 => self.pcm34,
            _ => 0xFF,
        }
    }

    pub fn pcm_mask(&self) -> [u8; 2] {
        self.pcm_mask
    }

    pub fn pcm_samples(&self) -> [u8; 4] {
        self.pcm_samples
    }

    pub fn lf_div_phase(&self) -> u8 {
        (self.lf_div_counter & 0x3) as u8
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
                        self.ch1.active = false;
                    }
                }
            }
            0xFF11 => {
                self.ch1.write_duty(val >> 6);
                self.ch1.length = 64 - (val & 0x3F);
                self.refresh_pcm_regs();
            }
            0xFF12 => {
                if self.ch1.enabled {
                    // Apply NRX2 glitch to current volume and update envelope params without resetting volume
                    let clock_high = self.ch1_env_clock.clock;
                    let new_vol =
                        self.apply_nrx2_glitch_square(1, self.ch1.envelope.volume, old_val, val);
                    self.ch1.envelope.volume = new_vol & 0x0F;
                    self.ch1.envelope.initial = val >> 4;
                    self.ch1.envelope.period = val & 0x07;
                    self.ch1.envelope.add = val & 0x08 != 0;
                    if clock_high {
                        self.ch1_env_countdown = self.ch1.envelope.period & 7;
                    }
                } else {
                    self.ch1.envelope.reset(val);
                    // When disabled, clear envelope lock state
                    self.ch1_env_clock = EnvelopeClock::default();
                }
                self.ch1.dac_enabled = val & 0xF8 != 0;
                if !self.ch1.dac_enabled {
                    self.ch1.enabled = false;
                    self.ch1.active = false;
                    self.ch1_env_clock = EnvelopeClock::default();
                }
                self.ch1_last_env_write_cycle = self.cpu_cycles;
                self.refresh_pcm_regs();
            }
            0xFF13 => self.ch1.write_frequency_low(val),
            0xFF14 => {
                let prev = self.ch1.length_enable;
                self.ch1.length_enable = val & 0x40 != 0;
                if !prev && self.ch1.length_enable {
                    let next_step = (self.sequencer.step + 1) & 7;
                    Apu::maybe_extra_len_clock(&mut self.ch1, next_step);
                }
                self.ch1.write_frequency_high(val);
                if val & 0x80 != 0 {
                    self.trigger_square(1);
                }
            }
            0xFF16 => {
                self.ch2.write_duty(val >> 6);
                self.ch2.length = 64 - (val & 0x3F);
                self.refresh_pcm_regs();
            }
            0xFF17 => {
                if self.ch2.enabled {
                    let clock_high = self.ch2_env_clock.clock;
                    let new_vol =
                        self.apply_nrx2_glitch_square(2, self.ch2.envelope.volume, old_val, val);
                    self.ch2.envelope.volume = new_vol & 0x0F;
                    self.ch2.envelope.initial = val >> 4;
                    self.ch2.envelope.period = val & 0x07;
                    self.ch2.envelope.add = val & 0x08 != 0;
                    if clock_high {
                        self.ch2_env_countdown = self.ch2.envelope.period & 7;
                    }
                } else {
                    self.ch2.envelope.reset(val);
                    self.ch2_env_clock = EnvelopeClock::default();
                }
                self.ch2.dac_enabled = val & 0xF8 != 0;
                if !self.ch2.dac_enabled {
                    self.ch2.enabled = false;
                    self.ch2.active = false;
                    self.ch2_env_clock = EnvelopeClock::default();
                }
                self.refresh_pcm_regs();
            }
            0xFF18 => self.ch2.write_frequency_low(val),
            0xFF19 => {
                let prev = self.ch2.length_enable;
                self.ch2.length_enable = val & 0x40 != 0;
                if !prev && self.ch2.length_enable {
                    let next_step = (self.sequencer.step + 1) & 7;
                    Apu::maybe_extra_len_clock(&mut self.ch2, next_step);
                }
                self.ch2.write_frequency_high(val);
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
                        // APU is transitioning from disabled to enabled. Initialize internal timing and staging.
                        // Set lf_div = 1 on init and ensure square staging is reset.
                        self.lf_div = 1;
                        self.ch1.out_latched = 0;
                        self.ch1.out_stage1 = 0;
                        self.ch1.out_stage2 = 0;
                        self.ch2.out_latched = 0;
                        self.ch2.out_stage1 = 0;
                        self.ch2.out_stage2 = 0;
                        self.cpu_cycles = 0;
                        self.sequencer.step = 0;
                        self.apu_enable_tick = 0;
                    }
                    self.nr52 |= 0x80;
                    if self.apu_enable_tick == 0 {
                        self.apu_enable_tick = self.lf_div_counter;
                    }
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
        let reg_idx = if idx == 1 { 0x04 } else { 0x09 };
        let value = self.regs[reg_idx];
        let length_enable = value & 0x40 != 0;

        let mut freq_updated = false;
        let de_window = self.cgb_mode && self.cgb_revision.supports_de_window();
        {
            let ch = if idx == 1 {
                &mut self.ch1
            } else {
                &mut self.ch2
            };

            let prev_sample_length = ch.sample_length;
            let prev_delay = ch.delay;
            let prev_countdown = ch.sample_countdown;
            let prev_just_reloaded = ch.just_reloaded;
            let was_active = ch.active;
            // Apply any pending duty change before computing initial output when triggering
            ch.duty = ch.duty_next;
            let lf_div = (self.lf_div & 0x1) as i32;

            apu_trace!(
                "sq{} trigger was_active={} freq={} duty_pos={} length={} lf_div={}",
                idx,
                was_active,
                ch.frequency,
                ch.duty_pos,
                ch.length,
                lf_div
            );

            ch.refresh_sample_length();
            ch.did_tick = false;

            let mut force_unsurpressed = false;
            let mut extra_delay = 0;

            if !was_active {
                if de_window && (value & 0x04) == 0 {
                    let window = ((prev_countdown - prev_delay) / 2) & 0x400;
                    if window & 0x400 == 0 {
                        ch.duty_pos = (ch.duty_pos + 1) & 7;
                        force_unsurpressed = true;
                    }
                }

                let mut delay = 6 - lf_div;
                if delay < 0 {
                    delay = 0;
                }
                ch.delay = delay;
                ch.sample_countdown = ((ch.sample_length ^ 0x07FF) as i32) * 2 + ch.delay;
                ch.sample_surpressed = ch.dac_enabled && !force_unsurpressed;
            } else {
                if de_window {
                    if !prev_just_reloaded && (value & 0x04) == 0 {
                        let window = ((prev_countdown - 1 - prev_delay) / 2) & 0x400;
                        if window & 0x400 == 0 {
                            ch.duty_pos = (ch.duty_pos + 1) & 7;
                            ch.sample_surpressed = false;
                        }
                    } else if ch.sample_length == 0x07FF
                        && prev_sample_length != 0x07FF
                        && ch.sample_surpressed
                    {
                        extra_delay += 2;
                    }
                }

                let mut delay = 4 - lf_div + extra_delay;
                if delay < 0 {
                    delay = 0;
                }
                ch.delay = delay;
                ch.sample_countdown = ((ch.sample_length ^ 0x07FF) as i32) * 2 + ch.delay;
            }

            ch.pending_reset = true;

            if ch.dac_enabled {
                let level = DUTY_TABLE[ch.duty as usize][ch.duty_pos as usize];
                let sample = level * ch.envelope.volume;
                if was_active || force_unsurpressed {
                    ch.out_latched = sample;
                    ch.out_stage1 = sample;
                    ch.out_stage2 = sample;
                } else if !was_active {
                    ch.out_latched = 0;
                    ch.out_stage1 = 0;
                    ch.out_stage2 = 0;
                }
            } else {
                ch.out_latched = 0;
                ch.out_stage1 = 0;
                ch.out_stage2 = 0;
            }

            let mut new_timer = ch.period();
            if was_active {
                let low_bits = ch.timer & 0x3;
                new_timer = (new_timer & !0x3) | low_bits;
                ch.sample_surpressed = false;
            }
            if new_timer <= 0 {
                new_timer = 1;
            }
            ch.timer = new_timer;

            ch.enabled = ch.dac_enabled;
            ch.active = ch.enabled;
            if was_active {
                ch.sample_surpressed = false;
            }

            // Clear envelope clock locks on trigger
            if idx == 1 {
                self.ch1_env_clock.locked = false;
                self.ch1_env_clock.clock = false;
                self.ch1_env_countdown = self.regs[0x02] & 0x07; // NR12 period
            } else {
                self.ch2_env_clock.locked = false;
                self.ch2_env_clock.clock = false;
                self.ch2_env_countdown = self.regs[0x07] & 0x07; // NR22 period
            }
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
            ch.length_enable = length_enable;

            if idx == 1
                && let Some(s) = ch.sweep.as_mut()
            {
                s.reload(ch.frequency);
                if s.shift != 0 {
                    let new_freq = s.calculate();
                    if new_freq > 2047 {
                        ch.enabled = false;
                        ch.active = false;
                        s.enabled = false;
                    } else {
                        if s.negate {
                            s.neg_used = true;
                        }
                        s.shadow = new_freq;
                        ch.frequency = new_freq;
                        ch.refresh_sample_length();
                        freq_updated = true;
                    }
                }
            }

            if ch.length == 0 {
                ch.length = 64;
            }
            if ch.length == 64 && length_enable {
                let upcoming = self.sequencer.step;
                if matches!(upcoming, 0 | 2 | 4 | 6) {
                    ch.length = 63;
                }
            }
        }

        if idx == 1 && freq_updated {
            self.update_ch1_freq_regs();
        }
        if idx == 1 {
            self.ch1.length_enable = length_enable;
        } else {
            self.ch2.length_enable = length_enable;
        }
        self.refresh_pcm_regs();
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
        self.ch4_env_clock.locked = false;
        self.ch4_env_clock.clock = false;
        self.ch4_env_countdown = self.regs[0x12] & 0x07; // NR42 period
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
            // No action here; envelope countdown scheduling is tied to DIV edges below.
        }
    }

    /// Tick the APU once per CPU cycle. `div_prev` is the DIV value at the
    /// beginning of the current machine step. In normal speed a machine step
    /// spans four CPU cycles; in double-speed it spans two.
    pub fn tick(&mut self, div_prev: u16, _div_now: u16, double_speed: bool) {
        // Store the current CPU speed so trigger_square can select the
        // correct initial delay when a channel is triggered.
        self.double_speed = double_speed;
        // Double-speed mode halves the CPU cycles per M-cycle, but we still emit
        // one 1 MHz stage tick every two CPU cycles so the staging pipeline aligns to the 1 MHz domain.
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
                // Falling edge (DIV event): advance frame sequencer, decrement envelope countdowns every 8 DIV events,
                // and if any envelope clock is high, tick once now.
                let step = self.sequencer.advance();
                self.clock_frame_sequencer(step);

                self.div_divider = self.div_divider.wrapping_add(1);
                if (self.div_divider & 7) == 7 {
                    if !self.ch1_env_clock.clock {
                        self.ch1_env_countdown = self.ch1_env_countdown.wrapping_sub(1) & 7;
                    }
                    if !self.ch2_env_clock.clock {
                        self.ch2_env_countdown = self.ch2_env_countdown.wrapping_sub(1) & 7;
                    }
                    if !self.ch4_env_clock.clock {
                        self.ch4_env_countdown = self.ch4_env_countdown.wrapping_sub(1) & 7;
                    }
                }

                // Tick envelopes if their clock is high; clear the clock and honor lock state.
                if self.ch1_env_clock.clock {
                    Apu::set_envelope_clock(&mut self.ch1_env_clock, false, false, 0);
                    if !self.ch1_env_clock.locked {
                        let nr12 = self.regs[0x02];
                        if nr12 & 7 != 0 {
                            if nr12 & 8 != 0 {
                                self.ch1.envelope.volume = (self.ch1.envelope.volume + 1) & 0x0F;
                            } else {
                                self.ch1.envelope.volume =
                                    (self.ch1.envelope.volume.wrapping_sub(1)) & 0x0F;
                            }
                        }
                    }
                }
                if self.ch2_env_clock.clock {
                    Apu::set_envelope_clock(&mut self.ch2_env_clock, false, false, 0);
                    if !self.ch2_env_clock.locked {
                        let nr22 = self.regs[0x07];
                        if nr22 & 7 != 0 {
                            if nr22 & 8 != 0 {
                                self.ch2.envelope.volume = (self.ch2.envelope.volume + 1) & 0x0F;
                            } else {
                                self.ch2.envelope.volume =
                                    (self.ch2.envelope.volume.wrapping_sub(1)) & 0x0F;
                            }
                        }
                    }
                }
                if self.ch4_env_clock.clock {
                    Apu::set_envelope_clock(&mut self.ch4_env_clock, false, false, 0);
                    if !self.ch4_env_clock.locked {
                        let nr42 = self.regs[0x12];
                        if nr42 & 7 != 0 {
                            if nr42 & 8 != 0 {
                                self.ch4.envelope.volume = (self.ch4.envelope.volume + 1) & 0x0F;
                            } else {
                                self.ch4.envelope.volume =
                                    (self.ch4.envelope.volume.wrapping_sub(1)) & 0x0F;
                            }
                        }
                    }
                }
            }

            if prev_bit == 0 && curr_bit == 1 {
                // Rising edge (secondary event): if countdown is zero, raise clock and reload countdown from period
                if self.ch1.active && self.ch1_env_countdown == 0 {
                    let nrx2 = self.regs[0x02]; // NR12
                    if nrx2 & 0x07 != 0 {
                        Apu::set_envelope_clock(
                            &mut self.ch1_env_clock,
                            true,
                            nrx2 & 0x08 != 0,
                            self.ch1.envelope.volume,
                        );
                        self.ch1_env_countdown = nrx2 & 0x07;
                    }
                }
                if self.ch2.active && self.ch2_env_countdown == 0 {
                    let nrx2 = self.regs[0x07]; // NR22
                    if nrx2 & 0x07 != 0 {
                        Apu::set_envelope_clock(
                            &mut self.ch2_env_clock,
                            true,
                            nrx2 & 0x08 != 0,
                            self.ch2.envelope.volume,
                        );
                        self.ch2_env_countdown = nrx2 & 0x07;
                    }
                }
                if self.ch4.enabled && self.ch4_env_countdown == 0 {
                    let nrx2 = self.regs[0x12]; // NR42
                    if nrx2 & 0x07 != 0 {
                        Apu::set_envelope_clock(
                            &mut self.ch4_env_clock,
                            true,
                            nrx2 & 0x08 != 0,
                            self.ch4.envelope.volume,
                        );
                        self.ch4_env_countdown = nrx2 & 0x07;
                    }
                }
            }

            // Update PCM12/PCM34 after each 1 MHz tick.
            self.refresh_pcm_regs();
            self.lf_div_counter = self.lf_div_counter.wrapping_add(1);
        }
        // cpu_cycles remains a CPU cycle counter for timers and IRQs.
        self.cpu_cycles = self.cpu_cycles.wrapping_add(1);
    }

    fn clock_square_channels_2mhz(&mut self, cycles: i32) {
        let clock_channel = |ch: &mut SquareChannel| {
            ch.clock_2mhz(cycles);
        };
        clock_channel(&mut self.ch1);
        clock_channel(&mut self.ch2);
    }

    fn maybe_extra_len_clock(ch: &mut SquareChannel, upcoming_step: u8) {
        if !matches!(upcoming_step, 0 | 2 | 4 | 6) && ch.length > 0 {
            ch.clock_length();
        }
    }

    /// Update FF76/FF77 to reflect the current channel outputs.
    fn refresh_pcm_regs(&mut self) {
        let mut samples = [0u8; 4];
        let mut active = [false; 4];

        samples[0] = self.ch1.peek_sample() & 0x0F;
        active[0] = self.ch1.enabled && self.ch1.dac_enabled;

        samples[1] = self.ch2.peek_sample() & 0x0F;
        active[1] = self.ch2.enabled && self.ch2.dac_enabled;

        samples[2] = self.ch3.peek_sample() & 0x0F;
        active[2] = self.ch3.enabled && self.ch3.dac_enabled;

        samples[3] = self.ch4.peek_sample() & 0x0F;
        active[3] = self.ch4.enabled && self.ch4.dac_enabled;

        let mut mask = [0xFFu8; 2];
        if self.cgb_revision.has_pcm_mask_glitch() {
            mask = [0, 0];
            if active[0] && samples[0] > 0 {
                mask[0] |= 0x0F;
            }
            if active[1] && samples[1] > 0 {
                mask[0] |= 0xF0;
            }
            if active[2] && samples[2] > 0 {
                mask[1] |= 0x0F;
            }
            if active[3] && samples[3] > 0 {
                mask[1] |= 0xF0;
            }
        }

        self.pcm_samples = samples;
        self.pcm_active = active;
        self.pcm_mask = mask;

        let ch1 = if active[0] { samples[0] } else { 0 };
        let ch2 = if active[1] { samples[1] } else { 0 };
        let ch3 = if active[2] { samples[2] } else { 0 };
        let ch4 = if active[3] { samples[3] } else { 0 };

        self.pcm12 = ((ch2 << 4) | ch1) & mask[0];
        self.pcm34 = ((ch4 << 4) | ch3) & mask[1];
        apu_trace!("pcm regs ch1={} ch2={} ch3={} ch4={}", ch1, ch2, ch3, ch4);
    }

    /// Mirror the current channel 1 frequency into NR13/NR14.
    fn update_ch1_freq_regs(&mut self) {
        let freq = self.ch1.frequency;
        self.regs[0x03] = (freq & 0xFF) as u8;
        self.regs[0x04] = (self.regs[0x04] & !0x07) | ((freq >> 8) as u8 & 0x07);
    }

    pub fn step(&mut self, cycles: u16) {
        let cps = CPU_CLOCK_HZ / self.sample_rate;
        // Advance square channels at 2 MHz: 1 tick per 2 CPU cycles (accumulated)
        self.mhz2_residual += cycles as i32;
        let ticks_2mhz = self.mhz2_residual / 2;
        self.mhz2_residual -= ticks_2mhz * 2;
        if ticks_2mhz > 0 {
            // Only advance the APU's 2 MHz domain (and lf_div parity) when the APU is enabled (NR52 bit 7 set).
            // This keeps internal clocks effectively gated while the APU is disabled.
            if self.nr52 & 0x80 != 0 {
                // Toggle parity of lf_div: xor with odd count of 2 MHz ticks
                if (ticks_2mhz & 1) != 0 {
                    self.lf_div ^= 1;
                }
                self.clock_square_channels_2mhz(ticks_2mhz);
                // Ensure PCM registers reflect any edge/suppression changes from 2 MHz domain
                self.refresh_pcm_regs();
            }
        }
        for _ in 0..cycles {
            self.cpu_cycles = self.cpu_cycles.wrapping_add(1);
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
        let dacs_on = self.ch1.dac_enabled
            || self.ch2.dac_enabled
            || self.ch3.dac_enabled
            || self.ch4.dac_enabled;

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

        if !dacs_on {
            self.hp_prev_input_left = 0.0;
            self.hp_prev_output_left = 0.0;
            self.hp_prev_input_right = 0.0;
            self.hp_prev_output_right = 0.0;
            (0, 0)
        } else {
            self.dc_block(left_sample, right_sample)
        }
    }

    fn dc_block(&mut self, left: i16, right: i16) -> (i16, i16) {
        let r = self.hp_coef;
        let left_in = left as f32;
        let right_in = right as f32;
        let left_out = left_in - self.hp_prev_input_left + r * self.hp_prev_output_left;
        let right_out = right_in - self.hp_prev_input_right + r * self.hp_prev_output_right;
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
        self.hp_coef = Apu::calc_hp_coef(rate);
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

    #[test]
    fn dc_filter_reset_when_all_dacs_off() {
        let mut apu = Apu::new();
        apu.nr50 = 0x00;
        apu.nr51 = 0x11;
        apu.ch1.enabled = true;
        apu.ch1.dac_enabled = true;
        apu.ch1.out_latched = 15;
        let _ = apu.mix_output();

        apu.ch1.dac_enabled = false;
        apu.ch2.dac_enabled = false;
        apu.ch3.dac_enabled = false;
        apu.ch4.dac_enabled = false;
        let (l, r) = apu.mix_output();
        assert_eq!(l, 0);
        assert_eq!(r, 0);
    }

    #[test]
    fn dc_filter_active_when_dac_on() {
        let mut apu = Apu::new();
        apu.nr50 = 0x00;
        apu.nr51 = 0x11;
        apu.ch1.enabled = true;
        apu.ch1.dac_enabled = true;
        apu.ch1.out_latched = 15;
        let (first, _) = apu.mix_output();
        apu.ch1.out_latched = 15;
        let (second, _) = apu.mix_output();
        assert!(second.abs() < first.abs());
    }
}
