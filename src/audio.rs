use crate::apu::ApuAudioBus;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Start audio playback using `cpal` and stream samples produced by the APU.
///
/// Returns the active [`cpal::Stream`] if successful.
pub fn start_stream(bus: ApuAudioBus) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let supported = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("no supported output config: {e}");
            return None;
        }
    };
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    bus.set_sample_rate(config.sample_rate.0);
    let channels = config.channels as usize;
    let err_fn = |err| eprintln!("cpal stream error: {err}");

    let bus_i16 = bus.clone();
    let bus_u16 = bus.clone();
    let bus_f32 = bus;

    let stream = match sample_format {
        cpal::SampleFormat::I16 => {
            let bus = bus_i16;
            device
                .build_output_stream(
                    &config,
                    move |data: &mut [i16], _| {
                        for frame in data.chunks_mut(channels) {
                            let left = bus.pop().unwrap_or(0);
                            let right = bus.pop().unwrap_or(0);
                            frame[0] = left;
                            if channels > 1 {
                                frame[1] = right;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .unwrap()
        }
        cpal::SampleFormat::U16 => {
            let bus = bus_u16;
            device
                .build_output_stream(
                    &config,
                    move |data: &mut [u16], _| {
                        for frame in data.chunks_mut(channels) {
                            let left = bus.pop().unwrap_or(0);
                            let right = bus.pop().unwrap_or(0);
                            frame[0] = (left as i32 + 32768) as u16;
                            if channels > 1 {
                                frame[1] = (right as i32 + 32768) as u16;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .unwrap()
        }
        cpal::SampleFormat::F32 => {
            let bus = bus_f32;
            device
                .build_output_stream(
                    &config,
                    move |data: &mut [f32], _| {
                        for frame in data.chunks_mut(channels) {
                            let left = bus.pop().unwrap_or(0) as f32 / 32768.0;
                            let right = bus.pop().unwrap_or(0) as f32 / 32768.0;
                            frame[0] = left;
                            if channels > 1 {
                                frame[1] = right;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .unwrap()
        }
        _ => panic!("Unsupported sample format"),
    };

    stream.play().expect("failed to play stream");
    Some(stream)
}
