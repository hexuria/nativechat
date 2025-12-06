use super::live_tts_provider::LiveTtsProvider;
use super::rest_tts_provider::RestTtsProvider;
use super::tts_provider::TtsProvider;
use crate::services::audio_output::{AudioCommand, AudioOutput};
use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Clone)]
pub struct TtsService {
    live_provider: Arc<LiveTtsProvider>,
    rest_provider: Arc<RestTtsProvider>,
    audio_output: Arc<AudioOutput>,
    /// Used to cancel ongoing streaming when stop() is called
    cancelled: Arc<AtomicBool>,
}

impl TtsService {
    pub fn new(is_ai_speaking: Arc<AtomicBool>, ai_amplitude: Arc<AtomicU32>) -> Result<Self> {
        let audio_output = Arc::new(AudioOutput::new(is_ai_speaking, ai_amplitude)?);

        Ok(Self {
            live_provider: Arc::new(LiveTtsProvider::new()),
            rest_provider: Arc::new(RestTtsProvider::new()),
            audio_output,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Check if a model should use the Live API (streaming WebSocket)
    fn is_live_api_model(model_id: &str) -> bool {
        model_id.to_lowercase().contains("native-audio")
    }

    /// Get the appropriate provider for the given model
    fn get_provider(&self, model_id: &str) -> Arc<dyn TtsProvider> {
        if Self::is_live_api_model(model_id) {
            self.live_provider.clone()
        } else {
            self.rest_provider.clone()
        }
    }

    fn get_cache_path(message_id: &str) -> Option<std::path::PathBuf> {
        let cache_dir = dirs::cache_dir()?;
        let app_cache_dir = cache_dir.join("nativechat").join("tts_cache");
        std::fs::create_dir_all(&app_cache_dir).ok()?;
        Some(app_cache_dir.join(format!("{}.bin", message_id)))
    }

    /// Check if audio is cached for a message
    pub fn is_cached(message_id: &str) -> bool {
        Self::get_cache_path(message_id)
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Convert PCM bytes to f32 samples
    fn bytes_to_samples(bytes: &[u8]) -> Vec<f32> {
        let mut samples = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
            samples.push(sample);
        }
        samples
    }

    /// Start speaking with streaming - audio plays as chunks arrive.
    /// Returns Ok(true) as soon as audio starts playing (first chunk received),
    /// Returns Ok(false) if nothing to play.
    /// The audio continues playing in the background.
    pub async fn start_speaking(
        &self,
        text: &str,
        message_id: &str,
        model_id: &str,
        api_key: &str,
    ) -> Result<bool> {
        // Reset cancellation flag for new request
        self.cancelled.store(false, Ordering::SeqCst);

        // Check cache first - if cached, play immediately without network request
        if let Some(path) = Self::get_cache_path(message_id) {
            if path.exists() {
                if let Ok(bytes) = std::fs::read(&path) {
                    let samples = Self::bytes_to_samples(&bytes);
                    if !samples.is_empty() {
                        self.audio_output
                            .process_command(AudioCommand::Samples(samples));
                        return Ok(true);
                    }
                }
            }
        }

        // Use the appropriate provider based on model_id
        let provider = self.get_provider(model_id);
        let mut rx = provider.stream_audio(text, model_id, api_key).await?;

        let mut audio_started = false;
        let mut all_samples: Vec<f32> = Vec::new();
        let mut was_cancelled = false;

        // Process audio chunks as they arrive
        while let Some(result) = rx.recv().await {
            // Check cancellation
            if self.cancelled.load(Ordering::SeqCst) {
                was_cancelled = true;
                break;
            }

            match result {
                Ok(chunk) => {
                    if !chunk.samples.is_empty() {
                        // Play immediately
                        if !self.cancelled.load(Ordering::SeqCst) {
                            self.audio_output
                                .process_command(AudioCommand::Samples(chunk.samples.clone()));

                            all_samples.extend(chunk.samples);
                            audio_started = true;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("TTS stream error: {}", e);
                    // Error means stream broken, don't cache partial audio
                    was_cancelled = true;
                    break;
                }
            }
        }

        // Only save complete audio to cache (not cancelled or errored)
        if !was_cancelled && !all_samples.is_empty() {
            // Convert back to i16 PCM bytes for storage
            let mut bytes = Vec::with_capacity(all_samples.len() * 2);
            for sample in all_samples {
                let s = (sample * 32768.0).clamp(-32768.0, 32767.0) as i16;
                bytes.extend_from_slice(&s.to_le_bytes());
            }

            if let Some(path) = Self::get_cache_path(message_id) {
                let _ = std::fs::write(path, &bytes);
            }
        }

        Ok(audio_started)
    }

    /// Wait for audio playback to complete
    pub async fn wait_until_finished(&self) {
        self.audio_output.wait_until_finished().await;
    }

    /// Legacy speak method - queues audio and waits for completion
    pub async fn speak(
        &self,
        text: &str,
        message_id: &str,
        model_id: &str,
        api_key: &str,
    ) -> Result<()> {
        if self
            .start_speaking(text, message_id, model_id, api_key)
            .await?
        {
            self.wait_until_finished().await;
        }
        Ok(())
    }

    pub fn stop(&self) {
        // Signal cancellation to any ongoing streaming
        self.cancelled.store(true, Ordering::SeqCst);
        self.audio_output.process_command(AudioCommand::Stop);
    }

    pub fn pause(&self) {
        self.audio_output.process_command(AudioCommand::Pause);
    }

    pub fn resume(&self) {
        self.audio_output.process_command(AudioCommand::Resume);
    }
}
