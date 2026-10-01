use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{error, info, warn};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use vibe_emu_core::apu::Apu;

pub struct OutputControls {
    volume: AtomicU8,
    mono: AtomicBool,
    suppressed: AtomicBool,
}

impl Default for OutputControls {
    fn default() -> Self {
        Self {
            volume: AtomicU8::new(100),
            mono: AtomicBool::new(false),
            suppressed: AtomicBool::new(false),
        }
    }
}

impl OutputControls {
    /// Mute presentation while paused or running at an altered speed.
    /// Samples are still consumed and emulated APU state is never changed.
    pub fn suppress(&self, suppressed: bool) {
        self.suppressed.store(suppressed, Ordering::Relaxed);
    }

    pub fn set(&self, volume: u8, mono: bool) {
        self.volume.store(volume.min(100), Ordering::Relaxed);
        self.mono.store(mono, Ordering::Relaxed);
    }

    fn process(&self, left: i16, right: i16) -> (i16, i16) {
        if self.suppressed.load(Ordering::Relaxed) {
            return (0, 0);
        }
        let (mut l, mut r) = (i32::from(left), i32::from(right));
        if self.mono.load(Ordering::Relaxed) {
            l = (l + r) / 2;
            r = l;
        }
        let volume = i32::from(self.volume.load(Ordering::Relaxed));
        ((l * volume / 100) as i16, (r * volume / 100) as i16)
    }
}

/// Build an audio stream using `cpal` and hook it up to the APU sample queue.
///
/// If `autoplay` is true the stream starts immediately; otherwise the caller is
/// responsible for invoking [`cpal::Stream::play`] once any warm-up work
/// completes. Returns the configured stream on success.
pub fn start_stream(
    apu: &mut Apu,
    autoplay: bool,
    sound_enabled: Arc<AtomicBool>,
    controls: Arc<OutputControls>,
) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(device) => device,
        None => {
            error!("no default audio output device available");
            return None;
        }
    };
    let device_name = match device.description() {
        Ok(description) => match description.manufacturer() {
            Some(manufacturer) => {
                format!("{} ({manufacturer})", description.name())
            }
            None => description.name().to_string(),
        },
        Err(_) => "<unknown>".to_string(),
    };
    let supported = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            error!("no supported output config: {e}");
            return None;
        }
    };
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let consumer = apu.enable_output(config.sample_rate);
    let channels = config.channels as usize;
    let buffer_label = match &config.buffer_size {
        cpal::BufferSize::Default => "default".to_string(),
        cpal::BufferSize::Fixed(size) => format!("fixed {size}"),
    };
    info!(
        "Audio stream config: device='{device_name}', format={sample_format:?}, rate={} Hz, channels={}, buffer={buffer_label}",
        config.sample_rate, channels,
    );
    let err_fn = |err| error!("cpal stream error: {err}");

    let stream = match sample_format {
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config,
            move |data: &mut [i16], _| {
                for frame in data.chunks_mut(channels) {
                    let (left, right) = consumer.pop_stereo().unwrap_or((0, 0));
                    let (left, right) = if sound_enabled.load(Ordering::Relaxed) {
                        controls.process(left, right)
                    } else {
                        (0, 0)
                    };
                    frame.fill(0);
                    frame[0] = left;
                    if channels > 1 {
                        frame[1] = right;
                    }
                }
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_output_stream(
            &config,
            move |data: &mut [u16], _| {
                for frame in data.chunks_mut(channels) {
                    let (left, right) = consumer.pop_stereo().unwrap_or((0, 0));
                    let (left, right) = if sound_enabled.load(Ordering::Relaxed) {
                        controls.process(left, right)
                    } else {
                        (0, 0)
                    };
                    frame.fill(32768);
                    frame[0] = (left as i32 + 32768) as u16;
                    if channels > 1 {
                        frame[1] = (right as i32 + 32768) as u16;
                    }
                }
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                for frame in data.chunks_mut(channels) {
                    let (left, right) = consumer.pop_stereo().unwrap_or((0, 0));
                    let (left, right) = if sound_enabled.load(Ordering::Relaxed) {
                        controls.process(left, right)
                    } else {
                        (0, 0)
                    };
                    let left = left as f32 / 32768.0;
                    let right = right as f32 / 32768.0;
                    frame.fill(0.0);
                    frame[0] = left;
                    if channels > 1 {
                        frame[1] = right;
                    }
                }
            },
            err_fn,
            None,
        ),
        other => {
            error!("Unsupported sample format: {other:?}");
            return None;
        }
    };

    let stream = match stream {
        Ok(stream) => stream,
        Err(e) => {
            error!("Failed to build audio output stream: {e}");
            return None;
        }
    };

    if autoplay {
        if let Err(e) = stream.play() {
            warn!("Failed to start audio stream: {e}");
            None
        } else {
            info!("Audio stream started");
            Some(stream)
        }
    } else {
        info!("Audio stream prepared (playback deferred)");
        Some(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_mono_and_suppression_do_not_overflow_or_change_preferences() {
        let controls = OutputControls::default();
        assert_eq!(controls.process(i16::MAX, i16::MIN), (i16::MAX, i16::MIN));
        controls.set(50, false);
        assert_eq!(controls.process(12000, -8000), (6000, -4000));
        controls.set(100, true);
        assert_eq!(controls.process(i16::MAX, i16::MAX), (i16::MAX, i16::MAX));
        assert_eq!(controls.process(12000, -8000), (2000, 2000));
        controls.suppress(true);
        assert_eq!(controls.process(12000, -8000), (0, 0));
        controls.suppress(false);
        assert_eq!(controls.process(12000, -8000), (2000, 2000));
    }
}
