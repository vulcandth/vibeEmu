use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use vibe_emu_core::apu::Apu;

/// Start audio playback using `cpal` and stream samples produced by the APU.
///
/// Returns the active [`cpal::Stream`] if successful.
pub fn start_stream(apu: Arc<Mutex<Apu>>) -> Option<cpal::Stream> {
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
    {
        let mut a = apu.lock().unwrap();
        a.set_sample_rate(config.sample_rate.0);
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
    Some(stream)
}
