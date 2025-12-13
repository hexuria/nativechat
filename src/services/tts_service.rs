use crate::services::audio_output::{AudioCommand, AudioOutput};
use crate::services::live_tts_provider::LiveTtsProvider;
use crate::services::macos_tts_bridge::{MacTtsBridge, TtsEvent};
use crate::services::rest_tts_provider::RestTtsProvider;
use crate::services::tts_provider::TtsProvider;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Clone, Copy, PartialEq)]
enum TtsMode {
    Native,
    Streaming,
    None,
}

#[derive(Clone)]
pub struct TtsService {
    live_provider: Arc<LiveTtsProvider>,
    rest_provider: Arc<RestTtsProvider>,
    audio_output: Arc<AudioOutput>,
    /// Used to cancel ongoing streaming when stop() is called
    cancelled: Arc<AtomicBool>,
    #[cfg(target_os = "macos")]
    native_provider: Option<Arc<MacTtsBridge>>,
    active_mode: Arc<Mutex<TtsMode>>,
    native_completion_notify: Arc<Notify>,
    native_paused: Arc<AtomicBool>,
    last_word_index: Arc<AtomicUsize>,
}

impl TtsService {
    pub fn new(is_ai_speaking: Arc<AtomicBool>, ai_amplitude: Arc<AtomicU32>) -> Result<Self> {
        let audio_output = Arc::new(AudioOutput::new(is_ai_speaking, ai_amplitude)?);

        let native_completion = Arc::new(Notify::new());
        let last_word_index = Arc::new(AtomicUsize::new(0));

        let native_provider = if cfg!(target_os = "macos") {
            let bridge = Arc::new(MacTtsBridge::new());
            let completion = native_completion.clone();
            let index = last_word_index.clone();

            bridge.set_callback(move |event| match event {
                TtsEvent::Start => {
                    println!("[TTS Service] Native TTS Started");
                }
                TtsEvent::Word { start, length: _ } => {
                    index.store(start, Ordering::SeqCst);
                }
                TtsEvent::Finish => {
                    println!("[TTS Service] Native TTS Finished");
                    completion.notify_waiters();
                }
            });

            Some(bridge)
        } else {
            None
        };

        Ok(Self {
            live_provider: Arc::new(LiveTtsProvider::new()),
            rest_provider: Arc::new(RestTtsProvider::new()),
            audio_output,
            cancelled: Arc::new(AtomicBool::new(false)),
            native_provider,
            active_mode: Arc::new(Mutex::new(TtsMode::None)),
            native_completion_notify: native_completion,
            native_paused: Arc::new(AtomicBool::new(false)),
            last_word_index,
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

    pub fn get_cache_path(message_id: &str) -> Option<std::path::PathBuf> {
        let cache_dir = dirs::cache_dir()?;
        let app_cache_dir = cache_dir.join("nativechat").join("tts_cache");
        std::fs::create_dir_all(&app_cache_dir).ok()?;
        let path = app_cache_dir.join(format!("{}.bin", message_id));
        // println!("[TTS Service] Cache Path for {}: {:?}", message_id, path);
        Some(path)
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

    pub fn start_speaking_native(&self, text: &str, message_id: &str) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            // Stop any previous speech
            bridge.stop();

            // Reset state
            // *self.active_mode.lock().unwrap() = TtsMode::Native;
            self.native_paused.store(false, Ordering::SeqCst);
            self.last_word_index.store(0, Ordering::SeqCst);

            // Speak immediately
            bridge.speak(text);
            return true;
        }

        println!("[TTS Service] Native TTS requested but not supported/available.");
        false
    }

    pub fn pause_native(&self) {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            bridge.pause();
            self.native_paused.store(true, Ordering::SeqCst);
        }
    }

    pub fn resume_native(&self) {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            bridge.resume();
            self.native_paused.store(false, Ordering::SeqCst);
        }
    }

    pub fn stop_native(&self) {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            bridge.stop();
            self.native_paused.store(false, Ordering::SeqCst);
        }
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

        // Reset audio output state to clear any previous stuck signals or data
        self.audio_output.process_command(AudioCommand::Stop);

        // **NATIVE TTS FAST PATH**
        if model_id == "native" {
            return Ok(self.start_speaking_native(text, message_id));
        }

        // Fallback or Streaming
        // Fallback or Streaming
        // *self.active_mode.lock().unwrap() = TtsMode::Streaming;

        // Check cache first - if cached, play immediately without network request
        if let Some(path) = Self::get_cache_path(message_id) {
            if path.exists() {
                println!("[TTS Service] Cache HIT for {}", message_id);
                if let Ok(bytes) = std::fs::read(&path) {
                    let samples = Self::bytes_to_samples(&bytes);
                    if !samples.is_empty() {
                        println!(
                            "[TTS Service] Playing from cache ({} samples)",
                            samples.len()
                        );
                        self.audio_output
                            .process_command(AudioCommand::Samples(samples));
                        return Ok(true);
                    }
                }
            } else {
                println!("[TTS Service] Cache MISS for {}", message_id);
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

    /// Wait for Native TTS to complete
    pub async fn wait_until_finished_native(&self) {
        #[cfg(target_os = "macos")]
        if self.native_provider.is_some() {
            self.native_completion_notify.notified().await;
        }
    }

    /// Wait for AI TTS to complete
    pub async fn wait_until_finished_ai(&self) {
        // Check if audio is actually playing before waiting
        if !self.audio_output.is_ai_speaking.load(Ordering::Relaxed) {
            return;
        }
        self.audio_output.wait_until_finished().await;
    }

    pub fn stop(&self) -> Result<()> {
        self.cancelled.store(true, Ordering::SeqCst);
        self.audio_output.process_command(AudioCommand::Stop);
        Ok(())
    }

    pub fn pause(&self) {
        self.audio_output.process_command(AudioCommand::Pause);
    }

    pub fn resume(&self) {
        self.audio_output.process_command(AudioCommand::Resume);
    }

    pub fn is_native_active(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            if let Some(bridge) = &self.native_provider {
                return !self.native_paused.load(Ordering::SeqCst);
            }
        }
        false
    }
}
