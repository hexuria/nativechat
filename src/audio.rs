use crate::services::gemini_client::GeminiLiveClient;
use base64::{Engine as _, engine::general_purpose};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

pub struct AudioInput {
    _stream: cpal::Stream,
}

impl AudioInput {
    pub fn new(
        amplitude: Arc<AtomicU32>,
        gemini_client: Option<GeminiLiveClient>,
    ) -> anyhow::Result<Self> {
        println!(
            "[AudioInput] Creating new instance. Has client: {}",
            gemini_client.is_some()
        );
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

        // Try to get a config that supports 16kHz, otherwise use default
        let config = device.default_input_config()?;

        // Simple check if we can set sample rate to 16000 (Gemini requirement)
        // In a robust app, we'd check supported ranges. For now, we'll try to use default and resample if needed.
        // Actually, let's just use the default and do a naive resample/decimate if it's 48k or 44.1k.
        // Or just send it as is if Gemini supports other rates? Docs say 16kHz.

        // Let's stick to the default config and handle conversion in the callback.

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                run::<f32>(&device, &config.into(), amplitude, gemini_client)
            }
            cpal::SampleFormat::I16 => {
                run::<i16>(&device, &config.into(), amplitude, gemini_client)
            }
            cpal::SampleFormat::U16 => {
                run::<u16>(&device, &config.into(), amplitude, gemini_client)
            }
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        }?;

        Ok(Self { _stream: stream })
    }
}

fn run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    amplitude: Arc<AtomicU32>,
    gemini_client: Option<GeminiLiveClient>,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: From<T>,
{
    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);
    let sample_rate = config.sample_rate.0;
    let channels = config.channels as usize;

    // Buffer for resampling/accumulating
    // We want to send chunks of ~100ms. 16000Hz * 0.1s = 1600 samples.
    let chunk_size = 1600;
    // Use a RefCell or Mutex for the buffer since the closure is FnMut (actually cpal requires FnMut but build_input_stream takes FnMut? No, it takes FnMut)
    // Wait, cpal callback is FnMut. So we can mutate captured variables if they are mut.
    // But we need to move them into the closure.
    let mut buffer: Vec<i16> = Vec::with_capacity(chunk_size);

    println!(
        "[AudioInput] Starting stream. Has client: {}",
        gemini_client.is_some()
    );
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &_| {
            // println!("[AudioInput] Callback fired. Data len: {}", data.len());

            // 1. Calculate Amplitude for Viz
            let mut sum_sq = 0.0;
            for &sample in data {
                let sample: f32 = f32::from(sample);
                let val = if std::mem::size_of::<T>() == 2 {
                    sample / 32768.0
                } else {
                    sample
                };
                sum_sq += val * val;
            }
            let rms = (sum_sq / data.len() as f32).sqrt();
            let boosted = (rms * 5.0).min(1.0);
            amplitude.store(boosted.to_bits(), Ordering::Relaxed);

            // 2. Process Audio
            if let Some(client) = &gemini_client {
                // Convert to Mono 16kHz PCM 16-bit
                let mut mono_samples = Vec::with_capacity(data.len() / channels);
                for frame in data.chunks(channels) {
                    let sample: f32 = f32::from(frame[0]); // Take first channel
                    let val = if std::mem::size_of::<T>() == 2 {
                        sample / 32768.0
                    } else {
                        sample
                    };
                    mono_samples.push(val);
                }

                // Resample to 16000Hz
                let ratio = sample_rate as f32 / 16000.0;
                let target_len = (mono_samples.len() as f32 / ratio) as usize;

                for i in 0..target_len {
                    let src_idx = (i as f32 * ratio) as usize;
                    if src_idx < mono_samples.len() {
                        let s = mono_samples[src_idx].clamp(-1.0, 1.0);
                        let val = (s * 32767.0) as i16;
                        buffer.push(val);
                    }
                }

                // Send if buffer is full
                if buffer.len() >= chunk_size {
                    let mut pcm_bytes = Vec::with_capacity(buffer.len() * 2);
                    for val in &buffer {
                        pcm_bytes.extend_from_slice(&val.to_le_bytes());
                    }

                    // Base64 encode and send
                    let base64_audio = general_purpose::STANDARD.encode(&pcm_bytes);
                    // println!("[AudioInput] Sending {} bytes of audio", pcm_bytes.len());
                    client.send_audio(base64_audio);

                    buffer.clear();
                }
            }
        },
        err_fn,
        None,
    )?;
    stream.play()?;
    Ok(stream)
}
