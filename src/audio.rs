use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

pub struct AudioInput {
    _stream: cpal::Stream,
}

impl AudioInput {
    pub fn new(amplitude: Arc<AtomicU32>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

        let config = device.default_input_config()?;

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => run::<f32>(&device, &config.into(), amplitude),
            cpal::SampleFormat::I16 => run::<i16>(&device, &config.into(), amplitude),
            cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), amplitude),
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        }?;

        Ok(Self { _stream: stream })
    }
}

fn run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    amplitude: Arc<AtomicU32>,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: From<T>,
{
    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &_| {
            let mut sum_sq = 0.0;
            for &sample in data {
                let sample: f32 = f32::from(sample);
                // Normalize if integer
                let val = if std::mem::size_of::<T>() == 2 {
                    sample / 32768.0
                } else {
                    sample
                };
                sum_sq += val * val;
            }
            let rms = (sum_sq / data.len() as f32).sqrt();
            // Boost the signal a bit for better visuals
            let boosted = (rms * 5.0).min(1.0);
            amplitude.store(boosted.to_bits(), Ordering::Relaxed);
        },
        err_fn,
        None,
    )?;
    stream.play()?;
    Ok(stream)
}
