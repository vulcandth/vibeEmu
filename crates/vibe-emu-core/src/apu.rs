//! APU (Audio Processing Unit) implementation for Game Boy / Game Boy Color.
//!
//! This implementation incorporates code and algorithms derived from SameBoy,
//! a highly accurate Game Boy and Game Boy Color emulator.
//!
//! SameBoy is licensed under the MIT (Expat) License:
//!   Copyright (c) 2015-2025 Lior Halphon
//!   https://github.com/LIJI32/SameBoy
//!
//! Specifically, the following components are derived from SameBoy's `Core/apu.c`:
//! - NRX2 "zombie mode" envelope glitch logic (`nrx2_glitch_step` and related functions)
//! - Envelope clock/lock mechanism (`EnvelopeClock` struct and `set_envelope_clock`)
//! - DIV-APU event skip state machine (`SkipDivEvent`)
//! - Sweep calculation and overflow check timing
//! - Various hardware quirk emulation for different CGB revisions

use std::cell::Cell;

use crate::audio_queue::{AudioConsumer, AudioProducer, audio_queue};

use crate::hardware::{CgbRevision, DmgRevision};

/// State machine for skipping DIV-APU events when APU powers on with DIV bit already set.
///
/// When the APU is enabled while the DIV APU bit (bit 12 in normal speed, bit 13 in
/// double speed) is already set, the first falling-edge event is skipped. Additionally,
/// the second event (first "real" event) does not increment the frame sequencer divider,
/// effectively delaying all frame sequencer-based effects by one event.
///
/// This behavior is derived from SameBoy's `skip_div_event` handling.
/// See: https://github.com/LIJI32/SameBoy/blob/master/Core/apu.c
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SkipDivEvent {
    #[default]
    Inactive,
    Skip,
    Skipped,
}

#[cfg(feature = "apu-trace")]
#[allow(unused_macros)]
macro_rules! apu_trace {
    ($($arg:tt)*) => {
        core_trace!(target: "vibe_emu_core::apu", "{}", format_args!($($arg)*));
    };
}

#[cfg(not(feature = "apu-trace"))]
#[allow(unused_macros)]
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

const NR41_IDX: usize = (0xFF20 - 0xFF10) as usize;
const NR42_IDX: usize = (0xFF21 - 0xFF10) as usize;
const NR43_IDX: usize = (0xFF22 - 0xFF10) as usize;
const NR44_IDX: usize = (0xFF23 - 0xFF10) as usize;

/// Envelope clock/lock state for square and noise channels.
///
/// This mechanism is derived from SameBoy's envelope clock implementation.
/// See: https://github.com/LIJI32/SameBoy/blob/master/Core/apu.c
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

        self.period = new_period;
        self.enabled = self.period != 0 || self.shift != 0;
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
        // Update only the low 8 bits of sample_length
        self.sample_length = (self.sample_length & 0x700) | value as u16;
        if self.just_reloaded {
            self.sample_countdown = Self::sample_countdown_from_length(self.sample_length);
        }
    }

    fn write_frequency_high(&mut self, value: u8) {
        self.frequency = (self.frequency & 0xFF) | (((value & 0x07) as u16) << 8);
        // Update only the high 3 bits of sample_length
        // This preserves the low bits that may have been modified by sweep
        self.sample_length = (self.sample_length & 0xFF) | (((value & 0x07) as u16) << 8);
        if self.just_reloaded {
            self.sample_countdown = Self::sample_countdown_from_length(self.sample_length);
        }
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
    fn clock_sweep(&mut self) -> bool {
        let mut freq_changed = false;
        if let Some(sweep) = self.sweep.as_mut() {
            if !sweep.enabled {
                return false;
            }
            if sweep.timer > 0 {
                sweep.timer -= 1;
            }
            if sweep.timer == 0 {
                sweep.timer = if sweep.period == 0 { 8 } else { sweep.period };
                if sweep.period == 0 {
                    return false;
                }
                let mut new_freq = sweep.calculate();
                if sweep.negate {
                    sweep.neg_used = true;
                }
                if new_freq > 2047 {
                    self.enabled = false;
                    self.active = false;
                    sweep.enabled = false;
                } else if sweep.shift != 0 {
                    sweep.shadow = new_freq;
                    self.frequency = new_freq;
                    freq_changed = true;
                    new_freq = sweep.calculate();
                    if new_freq > 2047 {
                        self.enabled = false;
                        self.active = false;
                        sweep.enabled = false;
                    }
                }
            }
        }
        if freq_changed {
            self.refresh_sample_length();
        }

        freq_changed
    }
}

struct WaveChannel {
    enabled: bool,
    dac_enabled: bool,
    length: u16,
    length_enable: bool,
    frequency: u16,
    timer: i32,
    shift: u8,
    sample_length: u16,
    sample_countdown: i32,
    delay: i32,
    pending_reset: bool,
    did_tick: bool,
    current_sample_index: u8,
    current_sample_byte: u8,
    wave_position: Cell<u8>,
    wave_sample_buffer: u8,
    wave_ram_access_index: Cell<u8>,
    wave_ram_locked: Cell<bool>,
    wave_form_just_read: Cell<bool>,
    sample_suppressed: Cell<bool>,
    bugged_read_countdown: u8,
    bugged_read_index: u8,
    wave_shadow: [u8; 0x10],
    wave_ram_state: u16,
    tick_count: u8,
    out_latched: u8,
    out_stage1: u8,
    out_stage2: u8,
}

impl Default for WaveChannel {
    fn default() -> Self {
        Self {
            enabled: false,
            dac_enabled: false,
            length: 0,
            length_enable: false,
            frequency: 0,
            timer: 0,
            shift: 4,
            sample_length: 0,
            sample_countdown: 0,
            delay: 0,
            pending_reset: false,
            did_tick: false,
            current_sample_index: 0,
            current_sample_byte: 0,
            wave_position: Cell::new(0),
            wave_sample_buffer: 0,
            wave_ram_access_index: Cell::new(0),
            wave_ram_locked: Cell::new(false),
            wave_form_just_read: Cell::new(false),
            sample_suppressed: Cell::new(false),
            bugged_read_countdown: 0,
            bugged_read_index: 0,
            wave_shadow: [0; 0x10],
            wave_ram_state: 0,
            tick_count: 0,
            out_latched: 0,
            out_stage1: 0,
            out_stage2: 0,
        }
    }
}

impl WaveChannel {
    #[inline]
    fn period_from_sample_length(sample_length: u16) -> i32 {
        ((sample_length ^ 0x07FF) as i32) + 1
    }

    fn compute_output(&self) -> u8 {
        if !self.enabled || !self.dac_enabled || self.sample_suppressed.get() {
            return 0;
        }
        if self.shift >= 4 {
            return 0;
        }
        self.wave_sample_buffer >> self.shift
    }

    fn set_pipeline_sample(&mut self, sample: u8) {
        self.out_latched = sample;
        self.out_stage1 = sample;
        self.out_stage2 = sample;
    }

    fn tick_1mhz(&mut self) {
        let sample = self.compute_output();
        self.out_stage2 = self.out_stage1;
        self.out_stage1 = self.out_latched;
        self.out_latched = sample;
    }

    fn current_sample(&self) -> u8 {
        self.out_stage2
    }

    fn step(&mut self, cycles: u32, wave_ram: &[u8; 0x10]) {
        if cycles == 0 {
            return;
        }

        self.tick_count = 0;
        if self.sample_countdown < 0 {
            self.sample_countdown = 0;
        }

        let mut cycles_left = cycles as i32;
        self.did_tick = false;
        self.wave_position.set(self.current_sample_index);
        self.wave_ram_access_index
            .set(self.current_sample_index >> 1);
        self.wave_form_just_read.set(false);

        if self.delay > 0 {
            let consumed = self.delay.min(cycles_left);
            self.delay -= consumed;
            cycles_left -= consumed;
            if cycles_left <= 0 {
                self.timer = self.sample_countdown;
                self.wave_ram_locked.set(self.enabled && self.dac_enabled);
                return;
            }
        }

        if !self.enabled || !self.dac_enabled {
            self.wave_ram_locked.set(false);
            self.sample_suppressed.set(true);
            self.pending_reset = false;
            if self.sample_countdown > 0 {
                let advance = cycles_left.min(self.sample_countdown);
                self.sample_countdown -= advance;
            }
            self.timer = self.sample_countdown;
            return;
        }

        self.wave_ram_locked.set(true);

        while cycles_left > self.sample_countdown {
            cycles_left -= self.sample_countdown + 1;
            self.sample_countdown = WaveChannel::period_from_sample_length(self.sample_length) - 1;
            if self.sample_countdown < 0 {
                self.sample_countdown = 0;
            }
            self.current_sample_index = (self.current_sample_index + 1) & 0x1F;
            self.wave_position.set(self.current_sample_index);
            let byte_index = (self.current_sample_index >> 1) as usize;
            let byte = wave_ram[byte_index];
            self.current_sample_byte = byte;
            self.wave_sample_buffer = if self.current_sample_index & 1 == 0 {
                byte >> 4
            } else {
                byte & 0x0F
            };
            self.wave_ram_access_index.set(byte_index as u8);
            self.wave_form_just_read.set(true);
            self.sample_suppressed.set(false);
            self.pending_reset = false;
            self.did_tick = true;
            self.tick_count = self.tick_count.saturating_add(1);
        }

        if cycles_left > 0 {
            self.sample_countdown -= cycles_left;
            if self.sample_countdown < 0 {
                self.sample_countdown = 0;
            }
            self.wave_form_just_read.set(false);
        }

        self.timer = self.sample_countdown;
    }
    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
                self.sample_suppressed.set(true);
                self.pending_reset = false;
                self.wave_ram_locked.set(false);
                self.set_pipeline_sample(0);
            }
        }
    }

    fn output(&self) -> u8 {
        self.compute_output()
    }

    fn peek_sample(&self) -> u8 {
        self.compute_output()
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
    narrow: bool,
    lfsr: u16,
    timer: i32,
    alignment: i32,
    current_lfsr_sample: bool,
    counter: i32,
    reload_counter: i32,
    counter_countdown: i32,
    delta: i32,
    countdown_reloaded: bool,
    dmg_delayed_start: u8,
    pending_disable: bool,
    pending_reset: bool,
    sample_suppressed: bool,
    volume_countdown: u8,
    current_volume: u8,
    envelope_clock: EnvelopeClock,
    out_latched: u8,
    out_stage1: u8,
    out_stage2: u8,
}

impl NoiseChannel {
    fn period(&self) -> i32 {
        let r = match self.divisor {
            0 => 8,
            _ => (self.divisor as i32) * 16,
        };
        r << self.clock_shift
    }

    fn base_divisor(&self) -> i32 {
        let mut divisor = (self.divisor as i32) << 2;
        if divisor == 0 {
            divisor = 2;
        }
        divisor
    }

    fn advance_lfsr(&mut self) {
        let bit0 = self.lfsr & 1;
        let bit1 = (self.lfsr >> 1) & 1;
        // The Game Boy noise channel feeds back the XNOR of bit 0 and bit 1.
        let bit = (!(bit0 ^ bit1)) & 1;
        self.lfsr >>= 1;
        self.lfsr |= bit << 14;
        if self.narrow {
            self.lfsr = (self.lfsr & !0x40) | (bit << 6);
        }
        self.current_lfsr_sample = self.lfsr & 1 != 0;
    }

    fn compute_output(&self) -> u8 {
        if !self.enabled || !self.dac_enabled || self.sample_suppressed {
            return 0;
        }
        if self.lfsr & 1 != 0 {
            self.current_volume
        } else {
            0
        }
    }

    fn set_pipeline_sample(&mut self, sample: u8) {
        self.out_latched = sample;
        self.out_stage1 = sample;
        self.out_stage2 = sample;
    }

    fn tick_1mhz(&mut self) {
        let sample = self.compute_output();
        self.out_stage2 = self.out_stage1;
        self.out_stage1 = self.out_latched;
        self.out_latched = sample;
    }

    fn current_sample(&self) -> u8 {
        self.out_stage2
    }

    fn output(&self) -> u8 {
        self.compute_output()
    }

    fn peek_sample(&self) -> u8 {
        self.compute_output()
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.pending_disable = true;
                self.sample_suppressed = true;
                self.set_pipeline_sample(0);
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
    sample_rate: u32,
    sample_timer_accum: u64,
    audio_out: Option<AudioProducer>,
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
    dmg_revision: DmgRevision,
    /// State machine for skipping DIV-APU events when APU powers on with DIV bit set.
    skip_div_event: SkipDivEvent,

    // ── Sweep state ──
    /// 128Hz sweep countdown (0-7), incremented when (div_divider & 3) == 3
    sweep_countdown: u8,
    /// 1 MHz countdown for delayed sweep calculation
    sweep_calc_countdown: u8,
    /// Reload timer for sweep calculation countdown (handles glitches)
    sweep_calc_reload_timer: u8,
    /// Shadow copy of frequency used for sweep calculations
    sweep_shadow_freq: u16,
    /// The delta (addend) to apply during sweep calculation
    sweep_addend: u16,
    /// Completed addend from last calculation
    sweep_completed_addend: u16,
    /// True if sweep shift is zero (no actual shifting)
    sweep_unshifted: bool,
    /// True if an instant calculation was already done this trigger
    sweep_instant_calc_done: bool,
    /// Hold period after channel 1 restart (delays sweep shadow reload)
    ch1_restart_hold: u8,
    /// True if a negate calculation has been used since last trigger
    sweep_neg_used: bool,
}

/// Lightweight snapshot of APU state for test diagnostics.
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct ApuDebugState {
    pub nr52: u8,
    pub ch1_enabled: bool,
    pub ch1_active: bool,
    pub ch1_dac_enabled: bool,
    pub ch1_length: u8,
    pub ch1_length_enable: bool,
    pub ch2_enabled: bool,
    pub ch2_active: bool,
    pub ch2_dac_enabled: bool,
    pub ch2_length: u8,
    pub ch2_length_enable: bool,
    pub ch3_enabled: bool,
    pub ch3_dac_enabled: bool,
    pub ch3_length: u16,
    pub ch3_length_enable: bool,
    pub ch4_enabled: bool,
    pub ch4_dac_enabled: bool,
    pub ch4_length: u8,
    pub ch4_length_enable: bool,
    pub sequencer_step: u8,
}

impl Apu {
    /// Returns a snapshot of internal APU state useful for integration-test diagnostics.
    #[doc(hidden)]
    pub fn debug_state(&self) -> ApuDebugState {
        ApuDebugState {
            nr52: self.nr52,
            ch1_enabled: self.ch1.enabled,
            ch1_active: self.ch1.active,
            ch1_dac_enabled: self.ch1.dac_enabled,
            ch1_length: self.ch1.length,
            ch1_length_enable: self.ch1.length_enable,
            ch2_enabled: self.ch2.enabled,
            ch2_active: self.ch2.active,
            ch2_dac_enabled: self.ch2.dac_enabled,
            ch2_length: self.ch2.length,
            ch2_length_enable: self.ch2.length_enable,
            ch3_enabled: self.ch3.enabled,
            ch3_dac_enabled: self.ch3.dac_enabled,
            ch3_length: self.ch3.length,
            ch3_length_enable: self.ch3.length_enable,
            ch4_enabled: self.ch4.enabled,
            ch4_dac_enabled: self.ch4.dac_enabled,
            ch4_length: self.ch4.length,
            ch4_length_enable: self.ch4.length_enable,
            sequencer_step: self.sequencer.step,
        }
    }

    // Keep <= AUDIO_LATENCY_MS of stereo frames in the queue
    fn max_frames_for_rate(rate: u32) -> usize {
        ((rate as usize * AUDIO_LATENCY_MS as usize) / 1000).max(1)
    }

    fn calc_hp_coef(rate: u32) -> f32 {
        0.999_958_f32.powf(4_194_304.0 / rate as f32)
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed_factor = speed;
    }

    fn tracking_audio(&self) -> bool {
        (self.speed_factor - 1.0).abs() < f32::EPSILON
    }

    pub fn queued_frames(&self) -> usize {
        self.audio_out.as_ref().map(|q| q.len()).unwrap_or(0)
    }

    pub fn max_queue_capacity(&self) -> usize {
        self.audio_out
            .as_ref()
            .map(|q| q.capacity_frames())
            .unwrap_or(0)
    }

    /// Enable lock-free audio output and return a consumer handle that can be
    /// drained by the audio backend.
    pub fn enable_output(&mut self, sample_rate: u32) -> AudioConsumer {
        self.set_sample_rate(sample_rate);
        let capacity_frames = Self::max_frames_for_rate(sample_rate);
        let (producer, consumer) = audio_queue(capacity_frames);
        self.audio_out = Some(producer);
        consumer
    }

    /// Disable audio output.
    pub fn disable_output(&mut self) {
        self.audio_out = None;
    }

    pub fn push_samples(&mut self, left: i16, right: i16) {
        if !self.tracking_audio() {
            return;
        }
        if let Some(out) = &self.audio_out {
            let _ = out.push_stereo(left, right);
        }
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

    fn wave_current_byte_index(&self) -> usize {
        (self.ch3.current_sample_index >> 1) as usize
    }

    fn wave_update_output(&mut self, byte_index: usize, byte: u8) -> bool {
        if self.wave_current_byte_index() != byte_index {
            return false;
        }
        self.ch3.current_sample_byte = byte;
        let nibble = if self.ch3.current_sample_index & 1 == 0 {
            byte >> 4
        } else {
            byte & 0x0F
        };
        self.ch3.wave_sample_buffer = nibble;
        if self.ch3.enabled && self.ch3.dac_enabled {
            let sample = self.ch3.compute_output();
            self.ch3.set_pipeline_sample(sample);
        } else {
            self.ch3.set_pipeline_sample(0);
        }
        true
    }

    fn commit_pending_wave_byte(&mut self, byte_index: usize) -> bool {
        let value = self.ch3.wave_shadow[byte_index];
        self.wave_ram[byte_index] = value;
        self.ch3.wave_shadow[byte_index] = value;
        self.ch3.wave_ram_state &= !(1 << byte_index);
        self.wave_update_output(byte_index, value)
    }

    fn flush_wave_shadow(&mut self) -> bool {
        let mut changed = false;
        while self.ch3.wave_ram_state != 0 {
            let byte_index = self.ch3.wave_ram_state.trailing_zeros() as usize;
            changed |= self.commit_pending_wave_byte(byte_index);
        }
        changed
    }

    fn apply_pending_wave_commits(&mut self) -> bool {
        let mut changed = false;
        if self.ch3.tick_count == 0 {
            return false;
        }
        let final_index = self.ch3.current_sample_index;
        for step in 0..self.ch3.tick_count {
            let finished_sample = final_index.wrapping_sub(step + 1) & 0x1F;
            if finished_sample & 1 == 1 {
                let byte_index = (finished_sample >> 1) as usize;
                if (self.ch3.wave_ram_state & (1 << byte_index)) != 0 {
                    changed |= self.commit_pending_wave_byte(byte_index);
                }
            }
        }
        self.ch3.tick_count = 0;
        changed
    }

    fn finish_bugged_read(&mut self) {
        let byte_index = self.ch3.bugged_read_index as usize;
        if (self.ch3.wave_ram_state & (1 << byte_index)) != 0 {
            self.commit_pending_wave_byte(byte_index);
        }
        let byte = self.wave_ram[byte_index];
        self.wave_update_output(byte_index, byte);
        self.ch3.sample_suppressed.set(false);
        self.ch3.bugged_read_countdown = 0;
        self.refresh_pcm_regs();
    }

    fn advance_bugged_read(&mut self, ticks: u32) {
        if self.ch3.bugged_read_countdown == 0 || ticks == 0 {
            return;
        }
        let countdown = u32::from(self.ch3.bugged_read_countdown);
        if ticks >= countdown {
            self.finish_bugged_read();
        } else {
            self.ch3.bugged_read_countdown -= ticks as u8;
        }
    }

    fn wave_cpu_read_locked(&mut self, _: usize) -> u8 {
        let just_read = self.ch3.wave_form_just_read.get();
        let nibble = self.ch3.wave_sample_buffer & 0x0F;
        let latched = (nibble << 4) | nibble;
        let old_model = !self.cgb_mode
            || matches!(
                self.cgb_revision,
                CgbRevision::Rev0 | CgbRevision::RevA | CgbRevision::RevB | CgbRevision::RevC
            );
        self.ch3.wave_form_just_read.set(false);
        self.ch3.bugged_read_index = self.wave_current_byte_index() as u8;
        self.ch3.bugged_read_countdown = 2;
        self.ch3.sample_suppressed.set(true);
        if old_model && just_read {
            latched
        } else {
            0xFF
        }
    }

    fn wave_cpu_read(&mut self, index: usize) -> u8 {
        self.ch3.wave_ram_access_index.set(index as u8);
        self.ch3.wave_position.set(self.ch3.current_sample_index);
        let locked = self.ch3.enabled && self.ch3.dac_enabled;
        self.ch3.wave_ram_locked.set(locked);
        if locked {
            self.wave_cpu_read_locked(index)
        } else {
            let changed = self.flush_wave_shadow();
            if changed {
                self.refresh_pcm_regs();
            }
            self.ch3.wave_form_just_read.set(true);
            self.wave_ram[index]
        }
    }

    fn wave_cpu_write(&mut self, index: usize, value: u8) {
        self.ch3.wave_ram_access_index.set(index as u8);
        self.ch3.wave_position.set(self.ch3.current_sample_index);
        let locked = self.ch3.enabled && self.ch3.dac_enabled;
        self.ch3.wave_ram_locked.set(locked);
        if locked {
            let target = self.wave_current_byte_index();
            self.ch3.wave_shadow[target] = value;
            self.ch3.wave_ram_state |= 1 << target;
            self.ch3.bugged_read_index = target as u8;
            self.ch3.bugged_read_countdown = 2;
            self.ch3.sample_suppressed.set(true);
            self.ch3.wave_form_just_read.set(false);
        } else {
            let mut changed = self.flush_wave_shadow();
            self.wave_ram[index] = value;
            self.ch3.wave_shadow[index] = value;
            changed |= self.wave_update_output(index, value);
            if changed {
                self.refresh_pcm_regs();
            }
        }
    }

    /// Returns the pending wave RAM write mask (one bit per byte) for debugging and tests.
    pub fn wave_pending_mask(&self) -> u16 {
        self.ch3.wave_ram_state
    }

    /// Returns the staged shadow byte for the given wave RAM index (used for testing).
    pub fn wave_shadow_byte(&self, index: usize) -> u8 {
        self.ch3.wave_shadow[index]
    }

    fn power_off(&mut self) {
        let nr41 = self.regs[NR41_IDX];
        self.ch1 = SquareChannel::new(true);
        self.ch2 = SquareChannel::new(false);
        self.ch3 = WaveChannel::default();
        self.ch4 = NoiseChannel::default();
        self.regs.fill(0);
        self.regs[NR41_IDX] = nr41;
        self.nr50 = 0;
        self.nr51 = 0;
        self.disable_output();
        self.sample_timer_accum = 0;
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
        self.div_divider = 0;
        self.skip_div_event = SkipDivEvent::Inactive;
    }

    /// Update envelope clock state, handling lock conditions.
    ///
    /// Derived from SameBoy's `set_envelope_clock` function.
    /// See: https://github.com/LIJI32/SameBoy/blob/master/Core/apu.c
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
    ///
    /// This "zombie mode" implementation is derived from SameBoy's `_nrx2_glitch` function.
    /// The complex envelope behavior when NRX2 is written while the channel is active
    /// was reverse-engineered and documented by LIJI32 in SameBoy.
    /// See: https://github.com/LIJI32/SameBoy/blob/master/Core/apu.c
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

    fn nrx2_glitch_noise_step(&mut self, new_val: u8, old_val: u8) {
        let lock = &mut self.ch4_env_clock;
        if lock.clock {
            self.ch4.volume_countdown = new_val & 0x07;
        }

        let mut should_tick = (new_val & 0x07) != 0 && (old_val & 0x07) == 0 && !lock.locked;
        let should_invert = (new_val ^ old_val) & 0x08 != 0;

        if (new_val & 0x0F) == 0x08 && (old_val & 0x0F) == 0x08 && !lock.locked {
            should_tick = true;
        }

        if should_invert {
            if new_val & 0x08 != 0 {
                if (old_val & 0x07) == 0 && !lock.locked {
                    self.ch4.current_volume ^= 0x0F;
                } else {
                    self.ch4.current_volume =
                        (0x0E_u8.wrapping_sub(self.ch4.current_volume)) & 0x0F;
                }
                should_tick = false;
            } else {
                self.ch4.current_volume = (0x10_u8.wrapping_sub(self.ch4.current_volume)) & 0x0F;
            }
        }

        if should_tick {
            if new_val & 0x08 != 0 {
                self.ch4.current_volume = (self.ch4.current_volume + 1) & 0x0F;
            } else {
                self.ch4.current_volume = self.ch4.current_volume.wrapping_sub(1) & 0x0F;
            }
        } else if (new_val & 0x07) == 0 && lock.clock {
            Apu::set_envelope_clock(lock, false, false, 0);
        }

        self.ch4.envelope.volume = self.ch4.current_volume;
    }

    fn apply_nrx2_glitch_noise(&mut self, old_val: u8, new_val: u8) {
        if self.is_pre_de_revision() {
            self.nrx2_glitch_noise_step(0xFF, old_val);
            self.nrx2_glitch_noise_step(new_val, 0xFF);
        } else {
            self.nrx2_glitch_noise_step(new_val, old_val);
        }
        self.ch4.envelope.initial = new_val >> 4;
        self.ch4.envelope.period = new_val & 0x07;
        self.ch4.envelope.add = new_val & 0x08 != 0;
    }
    fn new_internal() -> Self {
        let mut apu = Self {
            ch1: SquareChannel::new(true),
            ch2: SquareChannel::new(false),
            ch3: WaveChannel::default(),
            ch4: NoiseChannel::default(),
            wave_ram: [0; 0x10],
            regs: POWER_ON_REGS,
            nr50: 0x77,
            nr51: 0xF3,
            nr52: 0xF1,
            sequencer: FrameSequencer::new(),
            sample_rate: 44_100,
            sample_timer_accum: 0,
            audio_out: None,
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
            dmg_revision: DmgRevision::default(),
            ch1_env_clock: EnvelopeClock::default(),
            ch2_env_clock: EnvelopeClock::default(),
            ch4_env_clock: EnvelopeClock::default(),
            div_divider: 0,
            ch1_env_countdown: 0,
            ch2_env_countdown: 0,
            skip_div_event: SkipDivEvent::Inactive,
            // Sweep state initialization
            sweep_countdown: 0,
            sweep_calc_countdown: 0,
            sweep_calc_reload_timer: 0,
            sweep_shadow_freq: 0,
            sweep_addend: 0,
            sweep_completed_addend: 0,
            sweep_unshifted: false,
            sweep_instant_calc_done: false,
            ch1_restart_hold: 0,
            sweep_neg_used: false,
        };

        // Apply power-on register defaults (boot ROM may be skipped).
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
        Self::new_with_revisions(false, DmgRevision::default(), CgbRevision::default())
    }

    pub fn new_with_mode(cgb: bool) -> Self {
        Self::new_with_revisions(cgb, DmgRevision::default(), CgbRevision::default())
    }

    pub fn new_with_config(cgb: bool, revision: CgbRevision) -> Self {
        Self::new_with_revisions(cgb, DmgRevision::default(), revision)
    }

    pub fn new_with_revisions(cgb: bool, dmg_revision: DmgRevision, revision: CgbRevision) -> Self {
        let mut apu = Self::new_internal();
        apu.cgb_mode = cgb;
        apu.cgb_revision = revision;
        apu.dmg_revision = dmg_revision;
        apu.hp_coef = Apu::calc_hp_coef(apu.sample_rate);
        apu
    }

    pub fn read_reg(&mut self, addr: u16) -> u8 {
        if addr == 0xFF26 {
            // Process any pending sweep calculation before reading channel status
            // This is called before reading NR52
            // Only process if reload_timer has expired (== 0)
            if self.sweep_instant_calc_done
                && self.sweep_calc_countdown == 0
                && self.sweep_calc_reload_timer == 0
            {
                self.sweep_calculation_done();
                self.sweep_instant_calc_done = false;
            }

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
            let index = (addr - 0xFF30) as usize;
            return self.wave_cpu_read(index);
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

    pub fn on_div_reset(&mut self, prev_div: u16, double_speed: bool) {
        // APU frame sequencer is clocked by DIV bit 4 in single-speed and DIV
        // bit 5 in double-speed. Our `prev_div` is the internal 16-bit divider
        // (DIV register is the upper 8 bits), so these correspond to bits 12/13.
        let bit = if double_speed { 13 } else { 12 };
        let prev_bit = (prev_div >> bit) & 1;
        if prev_bit == 1 {
            self.handle_div_event();
        }
    }

    fn handle_div_rising_edge(&mut self) {
        // Rising edge (secondary event): if countdown is zero, raise clock and reload countdown from period.
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
        if self.ch4.enabled && self.ch4.volume_countdown == 0 {
            let nrx2 = self.regs[NR42_IDX]; // NR42
            if nrx2 & 0x07 != 0 {
                Apu::set_envelope_clock(
                    &mut self.ch4_env_clock,
                    true,
                    nrx2 & 0x08 != 0,
                    self.ch4.envelope.volume,
                );
                self.ch4.volume_countdown = nrx2 & 0x07;
            }
        }
    }

    /// Tick the DIV-driven APU frame sequencer.
    ///
    /// `div_prev`/`div_now` are in the CPU divider domain (the same internal
    /// 16-bit divider that backs rDIV). This is important during STOP-triggered
    /// CGB speed switching, where DIV/TIMA can be frozen while the APU's dot
    /// clock continues.
    pub fn tick_frame_sequencer(&mut self, div_prev: u16, div_now: u16, double_speed: bool) {
        if self.nr52 & 0x80 == 0 {
            return;
        }

        let bit = if double_speed { 13 } else { 12 };
        let steps = div_now.wrapping_sub(div_prev);
        if steps == 0 {
            return;
        }

        let mut div = div_prev;
        for _ in 0..steps {
            let prev_bit = (div >> bit) & 1;
            div = div.wrapping_add(1);
            let curr_bit = (div >> bit) & 1;

            if prev_bit == 1 && curr_bit == 0 {
                self.handle_div_event();
            }

            if prev_bit == 0 && curr_bit == 1 {
                self.handle_div_rising_edge();
            }
        }
    }

    /// Write an APU register with knowledge of the current DIV state.
    /// The `div` parameter should be the 16-bit internal divider value (timer.div).
    /// This is needed for NR52 writes where the APU needs to know if the DIV
    /// APU bit is already set when powering on.
    pub fn write_reg_with_div(&mut self, addr: u16, val: u8, div: u16, double_speed: bool) {
        if addr == 0xFF26 && val & 0x80 != 0 && self.nr52 & 0x80 == 0 {
            // APU is being powered on - check if the DIV APU bit is already set.
            // In normal speed, bit 12 drives the frame sequencer.
            // In double speed, bit 13 drives the frame sequencer.
            let bit = if double_speed { 13 } else { 12 };
            if (div >> bit) & 1 == 1 {
                // The DIV APU bit is already set, so we should skip the first
                // falling edge event that would otherwise occur.
                self.skip_div_event = SkipDivEvent::Skip;
                // When skip mechanism is active, div_divider starts at 1
                // so that after the Skipped event (which doesn't increment),
                // the length counter check (div_divider & 1) == 1 fails.
                self.div_divider = 1;
            }
        }
        self.write_reg(addr, val);
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        if self.nr52 & 0x80 == 0
            && addr != 0xFF26
            && addr != 0xFF20
            && !(0xFF30..=0xFF3F).contains(&addr)
        {
            return;
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
                // Handle NR10 glitch during ongoing sweep calculation
                if self.sweep_calc_countdown > 0 || self.sweep_calc_reload_timer > 0 {
                    // Glitch handling for writes during calculation
                }

                let old_negate = if matches!(
                    self.cgb_revision,
                    CgbRevision::Rev0 | CgbRevision::RevA | CgbRevision::RevB | CgbRevision::RevC
                ) {
                    true // On old CGB revisions, treat old_negate as true
                } else {
                    old_val & 0x08 != 0
                };
                let new_negate = val & 0x08 != 0;

                // If sweep went from negate to non-negate and a calculation was done,
                // disable the channel (APU bug)
                if old_negate && !new_negate {
                    let overflow = self.sweep_shadow_freq
                        .wrapping_add(self.sweep_completed_addend)
                        .wrapping_add(1) // +1 for negate mode
                        > 0x07FF;
                    if overflow {
                        self.ch1.enabled = false;
                        self.ch1.active = false;
                    }
                }

                // Update the legacy sweep struct parameters only (for debug access)
                if let Some(s) = self.ch1.sweep.as_mut() {
                    s.period = (val >> 4) & 0x07;
                    s.negate = val & 0x08 != 0;
                    s.shift = val & 0x07;
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
                let length_enable = val & 0x40 != 0;
                self.ch1.write_frequency_high(val);
                let triggered = val & 0x80 != 0;
                if triggered && !self.cgb_mode {
                    self.extra_length_clock_square(prev, length_enable, false, 1);
                    self.trigger_square(1, prev);
                } else {
                    if triggered {
                        self.trigger_square(1, prev);
                    }
                    self.extra_length_clock_square(prev, length_enable, triggered, 1);
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
                let length_enable = val & 0x40 != 0;
                self.ch2.write_frequency_high(val);
                let triggered = val & 0x80 != 0;
                if triggered && !self.cgb_mode {
                    self.extra_length_clock_square(prev, length_enable, false, 2);
                    self.trigger_square(2, prev);
                } else {
                    if triggered {
                        self.trigger_square(2, prev);
                    }
                    self.extra_length_clock_square(prev, length_enable, triggered, 2);
                }
            }
            0xFF1A => {
                self.ch3.dac_enabled = val & 0x80 != 0;
                if !self.ch3.dac_enabled {
                    self.ch3.enabled = false;
                    self.ch3.wave_ram_locked.set(false);
                    self.ch3.sample_suppressed.set(true);
                    self.ch3.pending_reset = false;
                    self.ch3.set_pipeline_sample(0);
                    self.refresh_pcm_regs();
                }
            }
            0xFF1B => self.ch3.length = 256 - val as u16,
            0xFF1C => {
                let shift_table = [4, 0, 1, 2];
                self.ch3.shift = shift_table[((val >> 5) & 0x03) as usize];
                if self.ch3.enabled && self.ch3.dac_enabled {
                    let sample = self.ch3.compute_output();
                    self.ch3.set_pipeline_sample(sample);
                    self.refresh_pcm_regs();
                }
            }
            0xFF1D => {
                self.ch3.frequency = (self.ch3.frequency & 0x700) | val as u16;
                self.ch3.sample_length = (self.ch3.sample_length & 0x700) | val as u16;
                if self.ch3.bugged_read_countdown == 1 {
                    let mut countdown =
                        WaveChannel::period_from_sample_length(self.ch3.sample_length) - 1;
                    if countdown < 0 {
                        countdown = 0;
                    }
                    self.ch3.sample_countdown = countdown;
                }
            }
            0xFF1E => {
                let prev = self.ch3.length_enable;
                let length_enable = val & 0x40 != 0;
                self.ch3.frequency = (self.ch3.frequency & 0xFF) | (((val & 0x07) as u16) << 8);
                self.ch3.sample_length =
                    (self.ch3.sample_length & 0xFF) | (((val & 0x07) as u16) << 8);
                let triggered = val & 0x80 != 0;
                if triggered && !self.cgb_mode {
                    self.extra_length_clock_wave(prev, length_enable, false);
                    let was_enabled = self.ch3.enabled && self.ch3.dac_enabled;
                    self.trigger_wave(was_enabled, prev, length_enable);
                    self.refresh_pcm_regs();
                } else {
                    if triggered {
                        let was_enabled = self.ch3.enabled && self.ch3.dac_enabled;
                        self.trigger_wave(was_enabled, prev, length_enable);
                        self.refresh_pcm_regs();
                    }
                    self.extra_length_clock_wave(prev, length_enable, triggered);
                }
            }
            0xFF20 => {
                self.ch4.length = 64 - (val & 0x3F);
                #[cfg(feature = "apu-trace")]
                self.trace_noise_state("NR41", Some(val));
            }
            0xFF21 => {
                let new_dac = val & 0xF8 != 0;
                if self.ch4.enabled {
                    self.apply_nrx2_glitch_noise(old_val, val);
                } else {
                    self.ch4.envelope.reset(val);
                    self.ch4.current_volume = self.ch4.envelope.volume & 0x0F;
                    self.ch4.volume_countdown = val & 0x07;
                }
                self.ch4.dac_enabled = new_dac;
                if !self.ch4.dac_enabled {
                    self.ch4.enabled = false;
                    self.ch4.sample_suppressed = true;
                    self.ch4.pending_disable = false;
                    self.ch4.pending_reset = false;
                    self.ch4.dmg_delayed_start = 0;
                    self.ch4_env_clock = EnvelopeClock::default();
                    self.ch4.set_pipeline_sample(0);
                }
                #[cfg(feature = "apu-trace")]
                self.trace_noise_state("NR42", Some(val));
                self.refresh_pcm_regs();
            }
            0xFF22 => {
                let prev_shift = (old_val >> 4) & 0x0F;
                let new_shift = (val >> 4) & 0x0F;
                self.ch4.clock_shift = val >> 4;
                self.ch4.narrow = val & 0x08 != 0;
                self.ch4.divisor = val & 0x07;

                let effective = self.effective_noise_counter();
                let old_bit = ((effective >> prev_shift) & 1) != 0;
                let new_bit = ((effective >> new_shift) & 1) != 0;

                if self.ch4.countdown_reloaded {
                    let base = self.ch4.base_divisor();
                    let offset = self.noise_alignment_offset(base);
                    self.ch4.counter_countdown = (base + offset).max(1);
                    self.ch4.delta = 0;
                }

                if new_bit && (!old_bit || self.is_pre_de_revision()) {
                    if self.is_pre_de_revision() {
                        let saved = self.ch4.narrow;
                        self.ch4.narrow = true;
                        self.ch4.advance_lfsr();
                        self.ch4.narrow = saved;
                    } else {
                        self.ch4.advance_lfsr();
                    }
                }
                #[cfg(feature = "apu-trace")]
                self.trace_noise_state("NR43", Some(val));
            }
            0xFF23 => {
                let prev = self.ch4.length_enable;
                let length_enable = val & 0x40 != 0;
                let triggered = val & 0x80 != 0;
                if triggered && !self.cgb_mode {
                    self.extra_length_clock_noise(prev, length_enable, false);
                    self.trigger_noise(prev, length_enable);
                    self.refresh_pcm_regs();
                } else {
                    if triggered {
                        self.trigger_noise(prev, length_enable);
                        self.refresh_pcm_regs();
                    }
                    self.extra_length_clock_noise(prev, length_enable, triggered);
                }
                #[cfg(feature = "apu-trace")]
                self.trace_noise_state("NR44", Some(val));
            }
            0xFF24 => self.nr50 = val,
            0xFF25 => self.nr51 = val,
            0xFF26 => {
                if val & 0x80 == 0 {
                    self.nr52 &= !0x80;
                    self.power_off();
                } else {
                    if self.nr52 & 0x80 == 0 {
                        // On 0->1 transition, reset internal timing/pipelines to match hardware startup state.
                        self.lf_div = 1;
                        self.ch1.out_latched = 0;
                        self.ch1.out_stage1 = 0;
                        self.ch1.out_stage2 = 0;
                        self.ch2.out_latched = 0;
                        self.ch2.out_stage1 = 0;
                        self.ch2.out_stage2 = 0;
                        self.ch3.set_pipeline_sample(0);
                        self.ch4.set_pipeline_sample(0);
                        self.ch4.sample_suppressed = true;
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
                let index = (addr - 0xFF30) as usize;
                self.wave_cpu_write(index, val);
            }
            _ => {}
        }
    }

    fn trigger_square(&mut self, idx: u8, prev_length_enable: bool) {
        let reg_idx = if idx == 1 { 0x04 } else { 0x09 };
        let value = self.regs[reg_idx];
        let length_enable = value & 0x40 != 0;

        let freq_updated = false;
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

            // Don't call refresh_sample_length - sample_length has already been updated
            // by NR13/NR14 writes. This preserves the swept frequency's low bits.
            // Only update countdown if just_reloaded is set.
            if ch.just_reloaded {
                ch.sample_countdown = SquareChannel::sample_countdown_from_length(ch.sample_length);
            }
            ch.did_tick = false;

            let force_unsurpressed = false;
            let mut extra_delay = 0;

            if !was_active {
                // CGB startup delay is one 2 MHz tick shorter in normal speed but
                // matches DMG timing when running in double-speed mode.
                let base_delay = if self.cgb_mode {
                    if self.double_speed { 6 } else { 5 }
                } else {
                    6
                };
                let mut delay = base_delay - lf_div;
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

            if ch.length == 0 {
                ch.length = 64;
                if ch.length_enable
                    && (!prev_length_enable || (!self.cgb_mode && (self.sequencer.step & 1) != 0))
                {
                    ch.length = 63;
                }
            }
        }

        // Handle sweep initialization for channel 1 (at APU level)
        if idx == 1 {
            let nr10 = self.regs[0x00];
            let shift = nr10 & 0x07;
            let was_active = self.ch1.active;

            // Reset sweep state
            self.sweep_instant_calc_done = false;
            self.sweep_shadow_freq = 0;
            self.sweep_completed_addend = 0;
            self.sweep_neg_used = false;

            if shift != 0 {
                // APU bug: if shift is nonzero, overflow check also occurs on trigger
                self.sweep_calc_countdown = shift;

                // Reload timer depends on lf_div and CGB revision
                let base_timer = if (self.lf_div & 1) != (if self.double_speed { 1 } else { 0 })
                    && matches!(
                        self.cgb_revision,
                        CgbRevision::Rev0
                            | CgbRevision::RevA
                            | CgbRevision::RevB
                            | CgbRevision::RevC
                    ) {
                    3
                } else {
                    2
                };
                self.sweep_calc_reload_timer = if was_active {
                    base_timer
                } else {
                    base_timer + 1
                };
                self.sweep_unshifted = false;

                // Calculate initial addend
                self.sweep_addend = self.ch1.sample_length >> shift;
            } else {
                self.sweep_addend = 0;
            }

            // These are set unconditionally
            let cgb_not_d = self.cgb_mode && self.cgb_revision != CgbRevision::RevD;
            self.ch1_restart_hold = 2 - (self.lf_div & 1) + if cgb_not_d { 2 } else { 0 };
            self.sweep_countdown = ((nr10 >> 4) & 7) ^ 7;
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
    fn trigger_wave(&mut self, was_enabled: bool, prev_length_enable: bool, length_enable: bool) {
        let prev_sample = self.ch3.compute_output();
        let retrigger_bug = !self.cgb_mode && was_enabled && self.ch3.sample_countdown == 0;
        if retrigger_bug {
            // DMG hardware copies upcoming wave RAM bytes into the first slot when retriggered on the read edge.
            let byte_index = (self.ch3.current_sample_index >> 1) as usize;
            if byte_index < 4 {
                let value = self.wave_ram[byte_index];
                self.wave_ram[0] = value;
                if self.ch3.wave_ram_state & 1 == 0 {
                    self.ch3.wave_shadow[0] = value;
                }
            } else {
                let base = byte_index & !0x03;
                for i in 0..4 {
                    let value = self.wave_ram[base + i];
                    self.wave_ram[i] = value;
                    if self.ch3.wave_ram_state & (1 << i) == 0 {
                        self.ch3.wave_shadow[i] = value;
                    }
                }
            }
        }

        let countdown = WaveChannel::period_from_sample_length(self.ch3.sample_length) + 2;
        let first_byte = self.wave_ram[0];

        self.ch3.enabled = self.ch3.dac_enabled;
        self.ch3.current_sample_index = 0;
        if !was_enabled || self.ch3.sample_countdown == 0 {
            self.ch3.current_sample_byte = first_byte;
            self.ch3.wave_sample_buffer = (first_byte >> 4) & 0x0F;
        }
        self.ch3.wave_position.set(0);
        self.ch3.wave_ram_access_index.set(0);
        self.ch3.sample_countdown = countdown.max(0);
        self.ch3.timer = self.ch3.sample_countdown;
        self.ch3.delay = 0;
        self.ch3.did_tick = false;
        self.ch3
            .wave_ram_locked
            .set(self.ch3.enabled && self.ch3.dac_enabled);
        self.ch3.wave_form_just_read.set(false);
        self.ch3.bugged_read_countdown = 0;
        self.ch3.bugged_read_index = 0;
        self.ch3.tick_count = 0;
        self.ch3.pending_reset = true;

        if self.ch3.dac_enabled {
            if was_enabled {
                self.ch3.sample_suppressed.set(false);
                self.ch3.set_pipeline_sample(prev_sample);
            } else {
                self.ch3.sample_suppressed.set(true);
                self.ch3.set_pipeline_sample(0);
            }
        } else {
            self.ch3.sample_suppressed.set(true);
            self.ch3.set_pipeline_sample(0);
        }

        self.ch3.length_enable = length_enable;
        if self.ch3.length == 0 {
            self.ch3.length = 256;
            if self.ch3.length_enable
                && (!prev_length_enable || (!self.cgb_mode && (self.sequencer.step & 1) != 0))
            {
                self.ch3.length = 255;
            }
        }
    }

    fn trigger_noise(&mut self, prev_length_enable: bool, length_enable: bool) {
        self.ch4.length_enable = length_enable;
        if self.ch4.length == 0 {
            self.ch4.length = 64;
            if self.ch4.length_enable
                && (!prev_length_enable || (!self.cgb_mode && (self.sequencer.step & 1) != 0))
            {
                self.ch4.length = 63;
            }
        }
        self.ch4_env_clock.locked = false;
        self.ch4_env_clock.clock = false;
        self.ch4.volume_countdown = self.regs[NR42_IDX] & 0x07;
        self.ch4.pending_reset = true;
        self.ch4.pending_disable = false;

        if !self.cgb_mode && (self.ch4.alignment & 3) != 0 {
            self.ch4.dmg_delayed_start = 6;
            self.ch4.enabled = false;
            self.ch4.sample_suppressed = true;
            self.ch4.set_pipeline_sample(0);
        } else {
            self.start_noise_channel(false);
        }

        #[cfg(feature = "apu-trace")]
        self.trace_noise_state("trigger", None);
    }

    fn handle_div_event(&mut self) {
        // Skip mechanism for when APU was powered on while the DIV APU bit was already set.
        // Uses a 3-state machine:
        // - Skip: First event is completely skipped, transitions to Skipped
        // - Skipped: Frame sequencer advances but length/sweep/envelope are NOT clocked,
        //            div_divider is NOT incremented, transitions to Inactive
        // - Inactive: Normal operation
        if self.skip_div_event == SkipDivEvent::Skip {
            self.skip_div_event = SkipDivEvent::Skipped;
            return;
        }

        // In Skipped state, transition to Inactive but
        // still process the frame sequencer. Just don't increment div_divider.
        let was_skipped = self.skip_div_event == SkipDivEvent::Skipped;
        if was_skipped {
            self.skip_div_event = SkipDivEvent::Inactive;
        }

        let step = self.sequencer.advance();
        self.clock_frame_sequencer(step);

        if !was_skipped {
            self.div_divider = self.div_divider.wrapping_add(1);
        }

        if (self.div_divider & 7) == 7 {
            if !self.ch1_env_clock.clock {
                self.ch1_env_countdown = self.ch1_env_countdown.wrapping_sub(1) & 7;
            }
            if !self.ch2_env_clock.clock {
                self.ch2_env_countdown = self.ch2_env_countdown.wrapping_sub(1) & 7;
            }
            if !self.ch4_env_clock.clock {
                self.ch4.volume_countdown = self.ch4.volume_countdown.wrapping_sub(1) & 7;
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
                let nr42 = self.regs[NR42_IDX];
                if nr42 & 7 != 0 {
                    if nr42 & 8 != 0 {
                        self.ch4.current_volume = (self.ch4.current_volume + 1) & 0x0F;
                    } else {
                        self.ch4.current_volume = (self.ch4.current_volume.wrapping_sub(1)) & 0x0F;
                    }
                    self.ch4.envelope.volume = self.ch4.current_volume;
                }
            }
        }

        // Sweep is clocked when (div_divider & 3) == 3 (128 Hz rate)
        if (self.div_divider & 3) == 3 {
            self.sweep_countdown = self.sweep_countdown.wrapping_add(1) & 7;
            self.trigger_sweep_calculation();
        }
    }

    /// Called when sweep calculation countdown reaches zero.
    /// Performs the actual frequency calculation and overflow check.
    ///
    /// The timing and behavior of sweep calculations, including the "APU bug" where
    /// the frequency is checked after adding the delta twice, is derived from SameBoy.
    /// See: https://github.com/LIJI32/SameBoy/blob/master/Core/apu.c
    fn sweep_calculation_done(&mut self) {
        let nr10 = self.regs[0x00];
        let negate = nr10 & 0x08 != 0;

        // APU bug: sweep frequency is checked after adding the sweep delta twice
        if self.ch1_restart_hold == 0 {
            self.sweep_shadow_freq = self.ch1.sample_length;
        }

        // Calculate the addend
        if negate {
            self.sweep_addend ^= 0x07FF;
        }

        // Overflow check
        let sum = self.sweep_shadow_freq.wrapping_add(self.sweep_addend);
        if sum > 0x07FF && !negate {
            self.ch1.enabled = false;
            self.ch1.active = false;
        }

        self.sweep_completed_addend = self.sweep_addend;
    }

    /// Called when (div_divider & 3) == 3 to potentially trigger a sweep calculation.
    fn trigger_sweep_calculation(&mut self) {
        let nr10 = self.regs[0x00];
        let period = (nr10 >> 4) & 0x07;
        let shift = nr10 & 0x07;

        if period != 0 && self.sweep_countdown == 7 {
            if shift != 0 {
                // Update frequency from shadow + addend
                let negate_add = if nr10 & 0x08 != 0 { 1 } else { 0 };
                let new_freq = self
                    .sweep_addend
                    .wrapping_add(self.sweep_shadow_freq)
                    .wrapping_add(negate_add);
                self.ch1.sample_length = new_freq & 0x07FF;
            }

            if self.ch1_restart_hold == 0 {
                self.sweep_addend = self.ch1.sample_length >> shift;
            }

            // Recalculation and overflow check only occurs after a delay
            self.sweep_calc_countdown = shift;
            self.sweep_calc_reload_timer = 1 + (self.lf_div & 1);
            self.sweep_unshifted = shift == 0;

            if self.sweep_calc_countdown == 0 {
                self.sweep_instant_calc_done = true;
            }

            // Reset countdown for next sweep period
            self.sweep_countdown = period ^ 7;
        }
    }

    /// Tick sweep-related countdowns. Called during APU run with 1 MHz cycles.
    fn tick_sweep(&mut self, cycles: u8) {
        if self.sweep_calc_reload_timer > 0 {
            if self.sweep_calc_reload_timer > cycles {
                self.sweep_calc_reload_timer -= cycles;
                return;
            }

            // Reload timer expired
            if self.sweep_calc_countdown == 0 && self.sweep_instant_calc_done {
                self.sweep_calculation_done();
            }
            self.sweep_instant_calc_done = false;
            let remaining = cycles - self.sweep_calc_reload_timer;
            self.sweep_calc_reload_timer = 0;

            // Now tick the calculation countdown
            if self.sweep_calc_countdown > 0
                && (self.regs[0x00] & 0x07 != 0 || self.sweep_unshifted)
            {
                if self.sweep_calc_countdown > remaining {
                    self.sweep_calc_countdown -= remaining;
                } else {
                    self.sweep_calc_countdown = 0;
                    self.sweep_calculation_done();
                }
            }
        } else if self.sweep_calc_countdown > 0
            && (self.regs[0x00] & 0x07 != 0 || self.sweep_unshifted)
        {
            if self.sweep_calc_countdown > cycles {
                self.sweep_calc_countdown -= cycles;
            } else {
                self.sweep_calc_countdown = 0;
                self.sweep_calculation_done();
            }
        }

        // Tick channel 1 restart hold
        if self.ch1_restart_hold > 0 {
            if self.ch1_restart_hold > cycles {
                self.ch1_restart_hold -= cycles;
            } else {
                self.ch1_restart_hold = 0;
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
        // Note: Sweep is now clocked via div_divider in handle_div_event
        if step == 7 {
            // No action here; envelope countdown scheduling is tied to DIV edges below.
        }
    }

    /// Tick the APU once per machine step in the dot clock domain.
    ///
    /// This advances the 1 MHz staging pipeline and PCM registers. The
    /// frame sequencer is clocked separately via `tick_frame_sequencer`.
    pub fn tick(&mut self, div_prev: u16, div_now: u16, double_speed: bool) {
        // Store the current CPU speed so trigger_square can select the
        // correct initial delay when a channel is triggered.
        self.double_speed = double_speed;
        // Derive how many dot cycles elapsed in this step from the divider.
        // This lets callers tick the APU in larger chunks or single-dot stalls.
        let ticks = div_now.wrapping_sub(div_prev);
        if ticks == 0 {
            return;
        }
        for _ in 0..ticks {
            // Advance the 1 MHz sample pipeline for pulse and wave channels.
            self.ch1.tick_1mhz();
            self.ch2.tick_1mhz();
            self.ch3.tick_1mhz();
            self.ch4.tick_1mhz();

            // Update PCM12/PCM34 after each dot tick.
            self.refresh_pcm_regs();

            // Tick sweep calculation countdown at 1 MHz (once per 4 dots in single-speed,
            // or once per 2 dots in double-speed).
            let divisor: u64 = if double_speed { 2 } else { 4 };
            if self.lf_div_counter.is_multiple_of(divisor) {
                self.tick_sweep(1);
            }

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

    fn clock_wave_channel_2mhz(&mut self, cycles: i32) {
        if cycles <= 0 {
            return;
        }

        let ticks = cycles as u32;
        self.ch3.step(ticks, &self.wave_ram);
        let mut changed = self.apply_pending_wave_commits();
        if !self.ch3.enabled || !self.ch3.dac_enabled {
            changed |= self.flush_wave_shadow();
        }
        if changed {
            self.refresh_pcm_regs();
        }
        self.advance_bugged_read(ticks);
    }

    fn clock_noise_channel_2mhz(&mut self, mut cycles: i32) {
        if cycles <= 0 {
            return;
        }

        self.ch4.alignment = self.ch4.alignment.wrapping_add(cycles);

        // Handle the DMG delayed-start quirk by consuming the requested ticks before
        // the channel actually begins running.
        if self.ch4.dmg_delayed_start > 0 {
            let delay = i32::from(self.ch4.dmg_delayed_start);
            if delay > cycles {
                self.ch4.dmg_delayed_start = (delay - cycles) as u8;
            } else {
                cycles -= delay;
                self.ch4.dmg_delayed_start = 0;
                self.start_noise_channel(true);
            }
        }

        if cycles <= 0 {
            return;
        }

        let should_step = self.ch4.enabled || !self.cgb_mode;
        if !should_step {
            return;
        }

        if self.ch4.counter_countdown <= 0 {
            let base = self.ch4.base_divisor();
            self.ch4.counter_countdown = base.max(1);
        }

        let mut cycles_left = cycles;
        let width_shift = (self.regs[NR43_IDX] >> 4) & 0x0F;
        let mut stepped = false;

        while cycles_left >= self.ch4.counter_countdown {
            cycles_left -= self.ch4.counter_countdown;

            let divisor = self.ch4.base_divisor();
            let mut next = divisor + self.ch4.delta;
            if next <= 0 {
                next = 1;
            }
            self.ch4.counter_countdown = next;
            self.ch4.delta = 0;

            let old_bit = ((self.ch4.counter >> width_shift) & 1) != 0;
            self.ch4.counter = (self.ch4.counter + 1) & 0x3FFF;
            let new_bit = ((self.ch4.counter >> width_shift) & 1) != 0;

            if new_bit && !old_bit {
                self.ch4.advance_lfsr();
            }

            stepped = true;

            if self.ch4.pending_disable {
                self.ch4.pending_disable = false;
                self.ch4.enabled = false;
                self.ch4.sample_suppressed = true;
                self.ch4.set_pipeline_sample(0);
                if self.cgb_mode {
                    break;
                }
            }
        }

        if cycles_left > 0 {
            self.ch4.counter_countdown -= cycles_left;
            self.ch4.countdown_reloaded = false;
        } else {
            self.ch4.countdown_reloaded = true;
        }

        self.ch4.timer = self.ch4.counter_countdown;

        if self.ch4.pending_disable {
            self.ch4.pending_disable = false;
            self.ch4.enabled = false;
            self.ch4.sample_suppressed = true;
            self.ch4.set_pipeline_sample(0);
        }

        if stepped && self.ch4.sample_suppressed && self.ch4.enabled && self.ch4.dac_enabled {
            self.ch4.sample_suppressed = false;
        }
    }

    fn is_pre_de_revision(&self) -> bool {
        !self.cgb_mode
            || matches!(
                self.cgb_revision,
                CgbRevision::Rev0 | CgbRevision::RevA | CgbRevision::RevB | CgbRevision::RevC
            )
    }

    fn noise_alignment_offset(&self, divisor: i32) -> i32 {
        if divisor == 2 {
            return 0;
        }
        const PRE_DE_TABLE: [i32; 4] = [2, 1, 4, 3];
        const DE_TABLE: [i32; 4] = [2, 1, 0, 3];
        let index = (self.ch4.alignment & 3) as usize;
        if self.is_pre_de_revision() {
            PRE_DE_TABLE[index]
        } else {
            DE_TABLE[index]
        }
    }

    fn effective_noise_counter(&self) -> u16 {
        let mut counter = (self.ch4.counter & 0x3FFF) as u16;
        let nr43 = self.regs[NR43_IDX];
        if !self.cgb_mode {
            if counter & 0x8 != 0 {
                counter |= 0xE;
            }
            if counter & 0x80 != 0 {
                counter |= 0xFF;
            }
            if counter & 0x100 != 0 {
                counter |= 0x1;
            }
            if counter & 0x200 != 0 {
                counter |= 0x2;
            }
            if counter & 0x400 != 0 {
                counter |= 0x4;
            }
            if counter & 0x800 != 0 {
                if nr43 & 0x08 != 0 {
                    counter |= 0x400;
                }
                counter |= 0x8;
            }
            if counter & 0x1000 != 0 {
                counter |= 0x10;
            }
            if counter & 0x2000 != 0 {
                counter |= 0x20;
            }
            return counter;
        }

        match self.cgb_revision {
            CgbRevision::RevB => {
                if counter & 0x8 != 0 {
                    counter |= 0xE;
                }
                if counter & 0x80 != 0 {
                    counter |= 0xFF;
                }
                if counter & 0x100 != 0 {
                    counter |= 0x1;
                }
                if counter & 0x200 != 0 {
                    counter |= 0x2;
                }
                if counter & 0x400 != 0 {
                    counter |= 0x4;
                }
                if counter & 0x800 != 0 {
                    counter |= 0x408;
                }
                if counter & 0x1000 != 0 {
                    counter |= 0x10;
                }
                if counter & 0x2000 != 0 {
                    counter |= 0x20;
                }
            }
            CgbRevision::RevD => {
                let mask = if nr43 & 0x08 != 0 { 0x40 } else { 0x80 };
                if counter & mask != 0 {
                    counter |= 0xFF;
                }
                if counter & 0x100 != 0 {
                    counter |= 0x1;
                }
                if counter & 0x200 != 0 {
                    counter |= 0x2;
                }
                if counter & 0x400 != 0 {
                    counter |= 0x4;
                }
                if counter & 0x800 != 0 {
                    counter |= 0x8;
                }
                if counter & 0x1000 != 0 {
                    counter |= 0x10;
                }
            }
            CgbRevision::RevE => {
                let mask = if nr43 & 0x08 != 0 { 0x40 } else { 0x80 };
                if counter & mask != 0 {
                    counter |= 0xFF;
                }
                if counter & 0x1000 != 0 {
                    counter |= 0x10;
                }
            }
            _ => {
                if counter & 0x8 != 0 {
                    counter |= 0xE;
                }
                if counter & 0x80 != 0 {
                    counter |= 0xFF;
                }
                if counter & 0x100 != 0 {
                    counter |= 0x1;
                }
                if counter & 0x200 != 0 {
                    counter |= 0x2;
                }
                if counter & 0x400 != 0 {
                    counter |= 0x4;
                }
                if counter & 0x800 != 0 {
                    if nr43 & 0x08 != 0 {
                        counter |= 0x400;
                    }
                    counter |= 0x8;
                }
                if counter & 0x1000 != 0 {
                    counter |= 0x10;
                }
                if counter & 0x2000 != 0 {
                    counter |= 0x20;
                }
            }
        }

        counter
    }

    fn start_noise_channel(&mut self, from_delay: bool) {
        let was_active = self.ch4.enabled && self.ch4.dac_enabled && !self.ch4.sample_suppressed;

        self.ch4.pending_reset = false;
        self.ch4.pending_disable = false;
        self.ch4.dmg_delayed_start = 0;

        if !self.ch4.dac_enabled {
            self.ch4.enabled = false;
            self.ch4.sample_suppressed = true;
            self.ch4.set_pipeline_sample(0);
            return;
        }

        let base = self.ch4.base_divisor();
        let mut countdown = base + 4;
        self.ch4.delta = 0;

        if base == 2 {
            if self.is_pre_de_revision() {
                countdown += i32::from(self.lf_div & 1);
                if !self.double_speed {
                    countdown -= 1;
                }
            } else {
                countdown += 1 - i32::from(self.lf_div & 1);
            }
        } else {
            countdown += self.noise_alignment_offset(base);
            if ((self.ch4.alignment + 1) & 3) < 2 {
                if self.ch4.divisor == 1 {
                    countdown -= 2;
                    self.ch4.delta = 2;
                } else {
                    countdown -= 4;
                }
            }
        }

        if self.is_pre_de_revision() {
            let nr43 = self.regs[NR43_IDX];
            if self.double_speed {
                if (nr43 & 0xF0) == 0 && (nr43 & 0x07) != 0 {
                    countdown -= 1;
                } else if (nr43 & 0xF0) != 0 && (nr43 & 0x07) == 0 {
                    countdown += 1;
                }
            } else {
                countdown -= 2;
            }
        }

        if countdown <= 0 {
            countdown = 1;
        }

        self.ch4.counter_countdown = countdown;
        self.ch4.timer = countdown;
        self.ch4.countdown_reloaded = true;
        self.ch4.reload_counter = countdown;
        self.ch4.counter &= 0x3FFF;

        self.ch4.lfsr = 0;
        self.ch4.current_lfsr_sample = false;

        self.ch4.envelope.volume = self.ch4.envelope.initial & 0x0F;
        let mut env_timer = if self.ch4.envelope.period == 0 {
            8
        } else {
            self.ch4.envelope.period
        };
        if (self.sequencer.step + 1) & 7 == 7 {
            env_timer = env_timer.wrapping_add(1);
        }
        self.ch4.envelope.timer = env_timer;
        self.ch4.current_volume = self.ch4.envelope.volume;
        self.ch4.volume_countdown = self.regs[NR42_IDX] & 0x07;
        let retrigger_sample = if was_active && (self.ch4.lfsr & 1) != 0 {
            self.ch4.current_volume
        } else {
            0
        };

        if self.ch4.length == 0 {
            self.ch4.length = 64;
        }

        self.ch4.enabled = true;
        if was_active {
            self.ch4.sample_suppressed = false;
            self.ch4.set_pipeline_sample(retrigger_sample);
        } else {
            self.ch4.sample_suppressed = !from_delay;
            self.ch4.set_pipeline_sample(0);
        }
    }

    fn cgb_early_length_bug(&self) -> bool {
        self.cgb_mode
            && matches!(
                self.cgb_revision,
                CgbRevision::Rev0 | CgbRevision::RevA | CgbRevision::RevB
            )
    }

    fn extra_length_clock_square(
        &mut self,
        prev_length_enable: bool,
        new_length_enable: bool,
        triggered: bool,
        idx: u8,
    ) {
        let bugged = self.cgb_early_length_bug();
        let should_clock = {
            let ch = if idx == 1 {
                &mut self.ch1
            } else {
                &mut self.ch2
            };
            let tick = !prev_length_enable
                && (new_length_enable || bugged)
                && ch.length > 0
                && (self.div_divider & 1) != 0;
            if tick {
                ch.length = ch.length.saturating_sub(1);
                if ch.length == 0 {
                    if triggered {
                        ch.length = 63;
                        ch.enabled = ch.dac_enabled;
                        ch.active = ch.enabled;
                        ch.sample_surpressed = false;
                    } else {
                        ch.enabled = false;
                        ch.active = false;
                    }
                }
            }
            tick
        };
        if idx == 1 {
            self.ch1.length_enable = new_length_enable;
        } else {
            self.ch2.length_enable = new_length_enable;
        }
        if should_clock {
            self.refresh_pcm_regs();
        }
    }

    fn extra_length_clock_wave(
        &mut self,
        prev_length_enable: bool,
        new_length_enable: bool,
        triggered: bool,
    ) {
        let bugged = self.cgb_early_length_bug();
        let should_clock = {
            let tick = !prev_length_enable
                && (new_length_enable || bugged)
                && self.ch3.length > 0
                && (self.div_divider & 1) != 0;
            if tick {
                self.ch3.length = self.ch3.length.saturating_sub(1);
                if self.ch3.length == 0 {
                    if triggered {
                        self.ch3.length = 255;
                        self.ch3.enabled = self.ch3.dac_enabled;
                    } else {
                        self.ch3.enabled = false;
                        self.ch3.sample_suppressed.set(true);
                        self.ch3.set_pipeline_sample(0);
                    }
                }
            }
            tick
        };
        self.ch3.length_enable = new_length_enable;
        if should_clock {
            self.refresh_pcm_regs();
        }
    }

    fn extra_length_clock_noise(
        &mut self,
        prev_length_enable: bool,
        new_length_enable: bool,
        triggered: bool,
    ) {
        let bugged = self.cgb_early_length_bug();
        let should_clock = {
            let tick = !prev_length_enable
                && (new_length_enable || bugged)
                && self.ch4.length > 0
                && (self.div_divider & 1) != 0;
            if tick {
                self.ch4.length = self.ch4.length.saturating_sub(1);
                if self.ch4.length == 0 {
                    if triggered {
                        self.ch4.length = 63;
                        self.ch4.enabled = self.ch4.dac_enabled;
                        self.ch4.sample_suppressed = false;
                        self.ch4.pending_disable = false;
                    } else {
                        self.ch4.enabled = false;
                        self.ch4.sample_suppressed = true;
                        self.ch4.pending_disable = true;
                    }
                }
            }
            tick
        };
        self.ch4.length_enable = new_length_enable;
        if should_clock {
            self.refresh_pcm_regs();
        }
    }

    fn refresh_pcm_regs(&mut self) {
        let mut samples = [0u8; 4];
        let mut active = [false; 4];

        let ch1_sample = self.ch1.peek_sample() & 0x0F;
        let ch1_active = self.ch1.enabled && self.ch1.dac_enabled && !self.ch1.sample_surpressed;
        samples[0] = ch1_sample;
        active[0] = ch1_active;

        let ch2_sample = self.ch2.peek_sample() & 0x0F;
        let ch2_active = self.ch2.enabled && self.ch2.dac_enabled && !self.ch2.sample_surpressed;
        samples[1] = ch2_sample;
        active[1] = ch2_active;

        let ch3_sample = self.ch3.peek_sample() & 0x0F;
        let ch3_active =
            self.ch3.enabled && self.ch3.dac_enabled && !self.ch3.sample_suppressed.get();
        samples[2] = ch3_sample;
        active[2] = ch3_active;

        let ch4_sample = self.ch4.peek_sample() & 0x0F;
        let ch4_active = self.ch4.enabled && self.ch4.dac_enabled && !self.ch4.sample_suppressed;
        samples[3] = ch4_sample;
        active[3] = ch4_active;

        let mut mask = [0xFFu8; 2];
        if self.cgb_revision.has_pcm_mask_glitch() {
            mask = [0, 0];
            if active[0] && samples[0] > 0 {
                mask[0] |= 0x0F;
            }
            if active[1] && samples[1] > 0 {
                mask[0] |= 0xF0;
            }
            if active[2] {
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
    }

    /// Mirror the current channel 1 frequency into NR13/NR14.
    fn update_ch1_freq_regs(&mut self) {
        let freq = self.ch1.frequency;
        self.regs[0x03] = (freq & 0xFF) as u8;
        self.regs[0x04] = (self.regs[0x04] & !0x07) | ((freq >> 8) as u8 & 0x07);
    }

    pub fn step(&mut self, cycles: u16) {
        let rate = self.sample_rate as u64;
        let sample_period = CPU_CLOCK_HZ as u64;
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
                self.clock_wave_channel_2mhz(ticks_2mhz);
                self.clock_noise_channel_2mhz(ticks_2mhz);
                // Ensure PCM registers reflect any edge/suppression changes from 2 MHz domain
                self.refresh_pcm_regs();
            }
        }
        for _ in 0..cycles {
            self.cpu_cycles = self.cpu_cycles.wrapping_add(1);
            #[cfg(feature = "apu-trace")]
            self.trace_noise_state("step", None);
            self.sample_timer_accum += rate;
            if self.sample_timer_accum >= sample_period {
                self.sample_timer_accum -= sample_period;
                let (left, right) = self.mix_output();
                self.push_samples(left, right);
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
        let out3 = self.ch3.current_sample();
        let out4 = self.ch4.current_sample();

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
        self.sample_timer_accum = 0;
        self.hp_coef = Apu::calc_hp_coef(rate);
        // Queue sizing is handled by `enable_output()`.
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
        self.ch3.current_sample_index
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

    /// Whether channel 4 is currently using the 7-bit ("narrow") LFSR mode.
    pub fn ch4_narrow(&self) -> bool {
        self.ch4.narrow
    }

    /// Current reload counter value mirrored from NR43.
    pub fn ch4_reload_counter(&self) -> i32 {
        self.ch4.reload_counter
    }

    /// Current effective counter value for the noise LFSR.
    pub fn ch4_counter(&self) -> i32 {
        self.ch4.counter
    }

    /// Countdown until the next counter reload.
    pub fn ch4_counter_countdown(&self) -> i32 {
        self.ch4.counter_countdown
    }
}

#[cfg(feature = "apu-trace")]
impl Apu {
    fn trace_noise_state(&self, event: &str, reg_value: Option<u8>) {
        let noise = &self.ch4;
        let env = &noise.envelope;
        let env_clock = &self.ch4_env_clock;
        apu_trace!(
            "noise event={} reg={:?} cycle={} enabled={} dac={} length={} length_enable={} envelope{{initial={}, volume={}, timer={}, period={}, add={}}} volume_countdown={} current_volume={} envelope_clock{{clock={}, locked={}, should_lock={}}} clock_shift={} divisor={} narrow={} lfsr=0x{:04X} current_lfsr_sample={} timer={} sample_countdown={} delay={} alignment={} counter={} reload_counter={} counter_countdown={} dmg_delayed_start={} pending_disable={} pending_reset={} sample_suppressed={} current_sample={} lf_div={}",
            event,
            reg_value,
            self.cpu_cycles,
            noise.enabled,
            noise.dac_enabled,
            noise.length,
            noise.length_enable,
            env.initial,
            env.volume,
            env.timer,
            env.period,
            env.add,
            noise.volume_countdown,
            noise.current_volume,
            env_clock.clock,
            env_clock.locked,
            env_clock.should_lock,
            noise.clock_shift,
            noise.divisor,
            noise.narrow,
            noise.lfsr,
            noise.current_lfsr_sample,
            noise.timer,
            noise.sample_countdown,
            noise.delay,
            noise.alignment,
            noise.counter,
            noise.reload_counter,
            noise.counter_countdown,
            noise.dmg_delayed_start,
            noise.pending_disable,
            noise.pending_reset,
            noise.sample_suppressed,
            noise.peek_sample(),
            self.lf_div,
        );
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
