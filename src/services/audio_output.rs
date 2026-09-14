use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

pub enum AudioCommand {
    Samples(Vec<f32>),
    Stop,
    Pause,
    Resume,
}

#[derive(Clone)]
pub struct AudioController {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    pub is_ai_speaking: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    /// Counter for samples actually played (at output sample rate)
    pub samples_played: Arc<AtomicU64>,
    /// Output sample rate (e.g., 48000)
    pub output_sample_rate: u32,
    /// Accumulator for precise resampling
    resample_accumulator: Arc<Mutex<f32>>,
}

impl AudioController {
    pub fn new(
        buffer: Arc<Mutex<VecDeque<f32>>>,
        is_ai_speaking: Arc<AtomicBool>,
        is_paused: Arc<AtomicBool>,
        samples_played: Arc<AtomicU64>,
        output_sample_rate: u32,
    ) -> Self {
        Self {
            buffer,
            is_ai_speaking,
            is_paused,
            samples_played,
            output_sample_rate,
            resample_accumulator: Arc::new(Mutex::new(0.0)),
        }
    }

    pub fn process_command(&self, cmd: AudioCommand) {
        let mut buf = self.buffer.lock().unwrap();
        match cmd {
            AudioCommand::Samples(samples) => {
                // Precise Resampling: Use accumulator to preserve exact duration
                // This handles non-integer ratios (e.g. 24k -> 44.1k) without drift
                let input_rate = 24000.0f32;
                let output_rate = self.output_sample_rate as f32;

                if output_rate >= input_rate {
                    // Upsample or equal rate
                    // If rates match exactly, fast path
                    if (output_rate - input_rate).abs() < 0.1 {
                        buf.extend(samples);
                    } else {
                        let ratio = output_rate / input_rate;
                        let mut acc_guard = self.resample_accumulator.lock().unwrap();

                        for &s in &samples {
                            *acc_guard += ratio;
                            while *acc_guard >= 1.0 {
                                buf.push_back(s);
                                *acc_guard -= 1.0;
                            }
                        }
                    }
                } else {
                    // Downsample: skip samples (shouldn't happen normally)
                    let ratio = input_rate / output_rate;
                    for (i, &s) in samples.iter().enumerate() {
                        if (i as f32 % ratio) < 1.0 {
                            buf.push_back(s);
                        }
                    }
                }
                // Signal that we have data
                self.is_ai_speaking.store(true, Ordering::Relaxed);
            }
            AudioCommand::Stop => {
                buf.clear();
                self.is_ai_speaking.store(false, Ordering::Relaxed);
                self.is_paused.store(false, Ordering::Relaxed);
                // Reset samples played counter
                self.samples_played.store(0, Ordering::Relaxed);
            }
            AudioCommand::Pause => {
                self.is_paused.store(true, Ordering::Relaxed);
            }
            AudioCommand::Resume => {
                self.is_paused.store(false, Ordering::Relaxed);
            }
        }
    }

    /// Get playback position in seconds
    pub fn get_playback_position(&self) -> f32 {
        let samples = self.samples_played.load(Ordering::Relaxed);
        samples as f32 / self.output_sample_rate as f32
    }
}

pub struct AudioOutput {
    /// Keeps the cpal stream thread alive. The stream itself is !Send.
    _hold: std::sync::mpsc::Sender<()>,
    pub controller: AudioController,
    completion_notify: Arc<Notify>,
}

impl AudioOutput {
    pub fn new(is_ai_speaking: Arc<AtomicBool>, ai_amplitude: Arc<AtomicU32>) -> Result<Self> {
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (hold, park) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("audio-out".into())
            .spawn(move || {
                let result = Self::open_stream(is_ai_speaking, ai_amplitude);
                match result {
                    Ok((stream, controller, completion_notify)) => {
                        if let Err(error) = stream.play() {
                            let _ = ready_tx.send(Err(anyhow::anyhow!(error)));
                            return;
                        }
                        let _ = ready_tx.send(Ok((controller, completion_notify)));
                        let _stream = stream;
                        let _ = park.recv();
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            })
            .context("spawn audio-out thread")?;
        let (controller, completion_notify) =
            ready_rx.recv().context("audio-out thread dropped")??;
        Ok(Self {
            _hold: hold,
            controller,
            completion_notify,
        })
    }

    fn open_stream(
        is_ai_speaking: Arc<AtomicBool>,
        ai_amplitude: Arc<AtomicU32>,
    ) -> Result<(cpal::Stream, AudioController, Arc<Notify>)> {
        let host = cpal::default_host();
        let device = host.default_output_device().context("No output device")?;
        let config = device.default_output_config()?;
        let output_sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        println!(
            "[AudioOutput] Device sample rate: {}Hz, channels: {}",
            output_sample_rate, channels
        );

        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let is_paused = Arc::new(AtomicBool::new(false));
        let completion_notify = Arc::new(Notify::new());

        let samples_played = Arc::new(AtomicU64::new(0));

        let controller = AudioController::new(
            buffer.clone(),
            is_ai_speaking.clone(),
            is_paused.clone(),
            samples_played.clone(),
            output_sample_rate,
        );

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

                let mut samples_in_this_callback = 0usize;
                for frame in data.chunks_mut(channels) {
                    if let Some(sample) = buf.pop_front() {
                        for sample_out in frame.iter_mut() {
                            *sample_out = sample;
                        }
                        samples_in_this_callback += 1;
                    } else {
                        for sample_out in frame.iter_mut() {
                            *sample_out = 0.0;
                        }
                    }
                }
                if samples_in_this_callback > 0 {
                    samples_played.fetch_add(samples_in_this_callback as u64, Ordering::Relaxed);
                }

                let mut sum_sq = 0.0;
                for sample in data.iter() {
                    sum_sq += sample * sample;
                }
                let rms = (sum_sq / data.len() as f32).sqrt();

                let compressed = if rms > 0.0 {
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
        Ok((stream, controller, completion_notify))
    }

    pub fn process_command(&self, cmd: AudioCommand) {
        self.controller.process_command(cmd);
    }

    pub async fn wait_until_finished(&self) {
        self.completion_notify.notified().await;
    }
}
