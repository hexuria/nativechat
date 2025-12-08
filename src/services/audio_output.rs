use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

pub enum AudioCommand {
    Samples(Vec<f32>),
    Stop,
    Pause,
    Resume,
}

pub struct AudioOutput {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<VecDeque<f32>>>,
    pub is_ai_speaking: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    completion_notify: Arc<Notify>,
}

impl AudioOutput {
    pub fn new(is_ai_speaking: Arc<AtomicBool>, ai_amplitude: Arc<AtomicU32>) -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device().context("No output device")?;
        let config = device.default_output_config()?;
        let _sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let is_paused = Arc::new(AtomicBool::new(false));
        let completion_notify = Arc::new(Notify::new());

        let buffer_clone = buffer.clone();
        let is_paused_clone = is_paused.clone();
        let is_ai_speaking_clone = is_ai_speaking.clone();
        let ai_amplitude_clone = ai_amplitude.clone();
        let completion_notify_clone = completion_notify.clone();

        let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

        let mut was_speaking = false;

        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &_| {
                let mut buf = buffer_clone.lock().unwrap();
                let paused = is_paused_clone.load(Ordering::Relaxed);
                let has_samples = !buf.is_empty();

                is_ai_speaking_clone.store(has_samples, Ordering::Relaxed);

                // Detect completion (transition from speaking to not speaking)
                if was_speaking && !has_samples {
                    completion_notify_clone.notify_one();
                }
                was_speaking = has_samples;

                if paused {
                    for sample_out in data.iter_mut() {
                        *sample_out = 0.0;
                    }
                    ai_amplitude_clone.store(0, Ordering::Relaxed);
                    return;
                }

                for frame in data.chunks_mut(channels) {
                    if let Some(sample) = buf.pop_front() {
                        for sample_out in frame.iter_mut() {
                            *sample_out = sample;
                        }
                    } else {
                        for sample_out in frame.iter_mut() {
                            *sample_out = 0.0;
                        }
                    }
                }

                // Calculate RMS for Visualizer
                let mut sum_sq = 0.0;
                for sample in data.iter() {
                    sum_sq += sample * sample;
                }
                let rms = (sum_sq / data.len() as f32).sqrt();

                // Apply logarithmic scaling for more natural visualization
                // This prevents bars from maxing out too easily
                let compressed = if rms > 0.0 {
                    // Log scaling: log(1 + x*k) / log(1 + k) where k controls sensitivity
                    let k = 10.0;
                    ((1.0 + rms * k).ln() / (1.0 + k).ln()).min(1.0)
                } else {
                    0.0
                };

                if has_samples {
                    ai_amplitude_clone.store(compressed.to_bits(), Ordering::Relaxed);
                } else {
                    ai_amplitude_clone.store(0, Ordering::Relaxed);
                    is_ai_speaking_clone.store(false, Ordering::Relaxed);
                }
            },
            err_fn,
            None,
        )?;
        stream.play()?;

        Ok(Self {
            _stream: stream,
            buffer,
            is_ai_speaking,
            is_paused,
            completion_notify,
        })
    }

    pub fn process_command(&self, cmd: AudioCommand) {
        let mut buf = self.buffer.lock().unwrap();
        match cmd {
            AudioCommand::Samples(samples) => {
                // Simple 2x upsampling (24k -> 48k)
                for &s in &samples {
                    buf.push_back(s);
                    buf.push_back(s);
                }
                // Signal that we have data
                self.is_ai_speaking.store(true, Ordering::Relaxed);
            }
            AudioCommand::Stop => {
                buf.clear();
                self.is_ai_speaking.store(false, Ordering::Relaxed);
                self.is_paused.store(false, Ordering::Relaxed);
            }
            AudioCommand::Pause => {
                self.is_paused.store(true, Ordering::Relaxed);
            }
            AudioCommand::Resume => {
                self.is_paused.store(false, Ordering::Relaxed);
            }
        }
    }

    pub async fn wait_until_finished(&self) {
        self.completion_notify.notified().await;
    }
}
