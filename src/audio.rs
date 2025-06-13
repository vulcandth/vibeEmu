use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam::queue::ArrayQueue;
use std::sync::Arc;

pub struct AudioOutput {
    queue: Option<Arc<ArrayQueue<f32>>>,
    sample_rate: u32,
    #[allow(dead_code)]
    channels: u16,
}

impl AudioOutput {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = match host.default_output_device() {
            Some(d) => d,
            None => {
                eprintln!("No output audio device available. Audio disabled.");
                return Self {
                    queue: None,
                    sample_rate: 44100,
                    channels: 2,
                };
            }
        };

        let config = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to get default output config: {e}");
                match device
                    .supported_output_configs()
                    .ok()
                    .and_then(|mut cfgs| cfgs.next())
                {
                    Some(range) => range.with_max_sample_rate(),
                    None => {
                        eprintln!("No supported audio configs. Audio disabled.");
                        return Self {
                            queue: None,
                            sample_rate: 44100,
                            channels: 2,
                        };
                    }
                }
            }
        };

        let sample_format = config.sample_format();
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        let config: cpal::StreamConfig = config.into();

        let queue = Arc::new(ArrayQueue::<f32>::new(8192));
        let queue_clone = queue.clone();
        let err_fn = |err| eprintln!("Audio stream error: {err}");
        let stream_result = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    write_samples(data, channels, &queue_clone);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config,
                move |data: &mut [i16], _| {
                    write_samples(data, channels, &queue_clone);
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_output_stream(
                &config,
                move |data: &mut [u16], _| {
                    write_samples(data, channels, &queue_clone);
                },
                err_fn,
                None,
            ),
            _ => unreachable!(),
        };

        let stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to build audio stream: {e}");
                return Self {
                    queue: None,
                    sample_rate,
                    channels,
                };
            }
        };

        if let Err(e) = stream.play() {
            eprintln!("Failed to play audio stream: {e}");
            return Self {
                queue: None,
                sample_rate,
                channels,
            };
        }

        // Stream will run until process exit.
        std::mem::forget(stream);
        Self {
            queue: Some(queue),
            sample_rate,
            channels,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.queue.is_some()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[allow(dead_code)]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn push_sample(&self, left: f32, right: f32) {
        if let Some(q) = &self.queue {
            let _ = q.push(left);
            let _ = q.push(right);
        }
    }
}

fn write_samples<T>(output: &mut [T], channels: u16, queue: &ArrayQueue<f32>)
where
    T: cpal::Sample + cpal::FromSample<f32>,
{
    let ch = channels as usize;
    for frame in output.chunks_mut(ch) {
        let left = queue.pop().unwrap_or(0.0);
        let mut right = left;
        if ch > 1 {
            right = queue.pop().unwrap_or(0.0);
        }
        frame[0] = cpal::Sample::from_sample(left);
        if ch > 1 {
            frame[1] = cpal::Sample::from_sample(right);
            for i in 2..ch {
                frame[i] = cpal::Sample::from_sample(0.0);
            }
        }
    }
}
