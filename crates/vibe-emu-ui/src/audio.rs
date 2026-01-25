use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{error, info, warn};
use vibe_emu_core::apu::Apu;

/// Build an audio stream using `cpal` and hook it up to the APU sample queue.
///
/// If `autoplay` is true the stream starts immediately; otherwise the caller is
/// responsible for invoking [`cpal::Stream::play`] once any warm-up work
/// completes. Returns the configured stream on success.
pub fn start_stream(apu: &mut Apu, autoplay: bool) -> Option<cpal::Stream> {
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
                    let left = left as f32 / 32768.0;
                    let right = right as f32 / 32768.0;
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
