use crate::services::audio_output::{AudioCommand, AudioOutput};
use crate::services::live_tts_provider::LiveTtsProvider;
use crate::services::macos_tts_bridge::{MacTtsBridge, TtsEvent};
use crate::services::rest_tts_provider::RestTtsProvider;
use crate::services::tts_provider::TtsProvider;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
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
    pub native_paused: Arc<AtomicBool>,
    pub ai_paused: Arc<AtomicBool>,

    last_word_index: Arc<AtomicUsize>,
    last_word_length: Arc<AtomicUsize>,
    generation: Arc<AtomicU64>,
}

impl TtsService {
    pub fn new(is_ai_speaking: Arc<AtomicBool>, ai_amplitude: Arc<AtomicU32>) -> Result<Self> {
        let audio_output = Arc::new(AudioOutput::new(is_ai_speaking, ai_amplitude)?);

        let native_completion = Arc::new(Notify::new());
        let last_word_index = Arc::new(AtomicUsize::new(0));
        let last_word_length = Arc::new(AtomicUsize::new(0));
        let generation = Arc::new(AtomicU64::new(0));

        let native_provider = if cfg!(target_os = "macos") {
            let bridge = Arc::new(MacTtsBridge::new());
            let completion = native_completion.clone();
            let index = last_word_index.clone();
            let len = last_word_length.clone();

            bridge.set_callback(move |event| match event {
                TtsEvent::Start => {
                    println!("[TTS Service] Native TTS Started");
                    index.store(0, Ordering::SeqCst);
                    len.store(0, Ordering::SeqCst);
                }
                TtsEvent::Word { start, length } => {
                    index.store(start, Ordering::SeqCst);
                    len.store(length, Ordering::SeqCst);
                }
                TtsEvent::Finish => {
                    println!("[TTS Service] Native TTS Finished");
                    completion.notify_waiters();
                    // Reset on finish
                    len.store(0, Ordering::SeqCst);
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
            ai_paused: Arc::new(AtomicBool::new(false)),
            last_word_index,
            last_word_length,
            generation,
        })
    }

    /// Check if a model should use the Live API (streaming WebSocket)
    pub fn get_active_word_range(&self) -> Option<std::ops::Range<usize>> {
        let start = self.last_word_index.load(Ordering::SeqCst);
        let len = self.last_word_length.load(Ordering::SeqCst);
        if len > 0 {
            Some(start..start + len)
        } else {
            None
        }
    }

    pub fn is_live_api_model(model_id: &str) -> bool {
        let id_lower = model_id.to_lowercase();
        id_lower.contains("native-audio") || id_lower.contains("gemini-2.0")
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

    pub fn start_speaking_native(&self, text: &str, _message_id: &str) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            // Stop any previous speech
            bridge.stop();

            // Reset state
            // *self.active_mode.lock().unwrap() = TtsMode::Native;

            // Don't kill the task, just ensure it's paused so Native can take over cleanly
            // self.generation.fetch_add(1, Ordering::SeqCst);
            // self.cancelled.store(true, Ordering::SeqCst); // Don't cancel, just pause!
            self.ai_paused.store(true, Ordering::SeqCst);

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
        voice: &Option<String>,
    ) -> anyhow::Result<bool> {
        // Increment generation to invalidate previous tasks
        let current_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;

        // Reset cancellation flag for new request
        self.cancelled.store(false, Ordering::SeqCst);
        self.ai_paused.store(false, Ordering::SeqCst);
        // Reset highlighting state to prevent cross-talk
        self.cancelled.store(false, Ordering::SeqCst);
        self.ai_paused.store(false, Ordering::SeqCst);
        // Reset highlighting state to prevent cross-talk
        self.last_word_index.store(0, Ordering::SeqCst);
        self.last_word_length.store(0, Ordering::SeqCst);

        // Reset audio output state to clear any previous stuck signals or data
        self.audio_output.process_command(AudioCommand::Stop);

        // **NATIVE TTS FAST PATH**
        if model_id == "native" {
            return Ok(self.start_speaking_native(text, message_id));
        }

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

                        let duration_secs = samples.len() as f32 / 24000.0;
                        let duration = Some(Duration::from_secs_f32(duration_secs));

                        self.audio_output
                            .process_command(AudioCommand::Samples(samples.clone()));

                        // Spawn simulation for cached playback
                        self.spawn_simulation_task(
                            text.to_string(),
                            current_gen,
                            duration,
                            Some(samples),
                        );

                        return Ok(true);
                    }
                }
            } else {
                println!("[TTS Service] Cache MISS for {}", message_id);
            }
        }

        // Use the appropriate provider based on model_id
        let provider = self.get_provider(model_id);
        let mut rx = provider
            .stream_audio(text, model_id, api_key, voice)
            .await?;

        // Process first chunk to confirm audio started
        let mut first_samples = Vec::new();
        let mut audio_started = false;

        if let Some(result) = rx.recv().await {
            match result {
                Ok(chunk) => {
                    if !chunk.samples.is_empty() {
                        // Play immediately
                        self.audio_output
                            .process_command(AudioCommand::Samples(chunk.samples.clone()));
                        first_samples = chunk.samples;
                        audio_started = true;
                    }
                }
                Err(e) => {
                    eprintln!("TTS stream error on first chunk: {}", e);
                    return Ok(false);
                }
            }
        } else {
            return Ok(false);
        }

        if !audio_started {
            return Ok(false);
        }

        // Spawn task for the rest
        let audio_controller = self.audio_output.controller.clone(); // Use controller which is Send
        let cancelled = self.cancelled.clone();
        let generation = self.generation.clone();
        // Spawn simulated highlighting task
        self.spawn_simulation_task(text.to_string(), current_gen, None, None);

        let message_id_owned = message_id.to_string();
        tokio::spawn(async move {
            let mut all_samples = first_samples;
            let mut was_cancelled = false;

            while let Some(result) = rx.recv().await {
                if cancelled.load(Ordering::SeqCst)
                    || generation.load(Ordering::SeqCst) != current_gen
                {
                    was_cancelled = true;
                    break;
                }

                match result {
                    Ok(chunk) => {
                        if !chunk.samples.is_empty() {
                            // Play immediately
                            if !cancelled.load(Ordering::SeqCst)
                                && generation.load(Ordering::SeqCst) == current_gen
                            {
                                audio_controller
                                    .process_command(AudioCommand::Samples(chunk.samples.clone()));

                                all_samples.extend(chunk.samples);
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
                // Double check generation before writing cache
                if generation.load(Ordering::SeqCst) != current_gen {
                    return;
                }

                // Convert back to i16 PCM bytes for storage
                let mut bytes = Vec::with_capacity(all_samples.len() * 2);
                for sample in all_samples {
                    let s = (sample * 32768.0).clamp(-32768.0, 32767.0) as i16;
                    bytes.extend_from_slice(&s.to_le_bytes());
                }

                if let Some(path) = Self::get_cache_path(&message_id_owned) {
                    let _ = std::fs::write(path, &bytes);
                }
            }
        });

        Ok(true)
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
        // We know we sent samples, so wait for them to finish.
        // If we check is_ai_speaking here, it might race with the audio thread starting.
        self.audio_output.wait_until_finished().await;
    }

    pub fn stop(&self) -> Result<()> {
        self.cancelled.store(true, Ordering::SeqCst);
        self.audio_output.process_command(AudioCommand::Stop);
        Ok(())
    }

    pub fn pause(&self) {
        self.audio_output.process_command(AudioCommand::Pause);
        self.ai_paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.audio_output.process_command(AudioCommand::Resume);
        self.ai_paused.store(false, Ordering::SeqCst);
    }

    pub fn is_native_active(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            if let Some(_bridge) = &self.native_provider {
                return !self.native_paused.load(Ordering::SeqCst);
            }
        }
        false
    }

    fn spawn_simulation_task(
        &self,
        text: String,
        current_gen: u64,
        duration: Option<Duration>,
        samples: Option<Vec<f32>>,
    ) {
        let sim_text = text;
        let sim_last_word_index = self.last_word_index.clone();
        let sim_last_word_length = self.last_word_length.clone();
        let sim_cancelled = self.cancelled.clone();
        let sim_generation = self.generation.clone();
        let sim_paused = self.ai_paused.clone();

        tokio::spawn(async move {
            let sim_len = sim_text.len();
            let mut chars_per_sec = 15.0; // Fallback
            let mut active_chars_per_sec = 15.0;

            // Heuristic fallback variables
            let mut pause_time_per_comma = 0.2;
            let mut pause_time_per_period = 0.5;

            // Energy-Based Timing (if samples available)
            let mut use_energy_timing = false;
            let mut window_size_samples = 2400; // 100ms at 24kHz
            let mut silent_threshold = 0.01; // RMS threshold

            // Pre-calculate energy profile if samples exist
            let mut energy_profile = Vec::new();

            if let Some(audio_samples) = samples {
                use_energy_timing = true;
                // Calculate total active time (time where RMS > threshold)
                let mut active_window_count = 0;

                for chunk in audio_samples.chunks(window_size_samples) {
                    let mut sum_sq = 0.0;
                    for &s in chunk {
                        sum_sq += s * s;
                    }
                    let rms = (sum_sq / chunk.len() as f32).sqrt();
                    let is_active = rms > silent_threshold;
                    energy_profile.push(is_active);
                    if is_active {
                        active_window_count += 1;
                    }
                }

                let active_time =
                    (active_window_count as f32 * window_size_samples as f32) / 24000.0;
                let active_time = active_time.max(0.1);

                active_chars_per_sec = sim_len as f32 / active_time;

                println!(
                    "[TTS Service] Energy Timing: Active for {:.2}s. Speed: {:.2} cps (during speech)",
                    active_time, active_chars_per_sec
                );
            } else if let Some(dur) = duration {
                // FALLBACK: Punctuation-based "Smart Timing"
                if dur.as_secs_f32() > 0.0 {
                    let commas = sim_text.chars().filter(|&c| c == ',' || c == ';').count();
                    let periods = sim_text
                        .chars()
                        .filter(|&c| c == '.' || c == '!' || c == '?')
                        .count();

                    let estimated_pause_time = (commas as f32 * pause_time_per_comma)
                        + (periods as f32 * pause_time_per_period);
                    let effective_speech_time = (dur.as_secs_f32() - estimated_pause_time).max(0.1);

                    chars_per_sec = sim_len as f32 / effective_speech_time;
                    active_chars_per_sec = chars_per_sec;
                }
            }

            let tick_rate = 50; // 50ms ticks
            let mut current_char_index = 0.0;
            let mut pause_remaining = 0.0;
            let mut last_processed_idx = 0usize;
            let mut elapsed_ms = 0u64;

            // Initial delay
            tokio::time::sleep(Duration::from_millis(500)).await;

            loop {
                if sim_cancelled.load(Ordering::SeqCst)
                    || sim_generation.load(Ordering::SeqCst) != current_gen
                {
                    break;
                }

                if sim_paused.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(tick_rate)).await;
                    continue;
                }

                // --- Energy check ---
                if use_energy_timing {
                    // Map elapsed time to window index
                    // 100ms windows = 2400 samples @ 24kHz
                    // tick_rate = 50ms.
                    let current_window_idx =
                        (elapsed_ms as usize * 24000 / 1000) / window_size_samples;

                    if current_window_idx < energy_profile.len() {
                        if !energy_profile[current_window_idx] {
                            // SILENCE: Do not advance index
                            tokio::time::sleep(Duration::from_millis(tick_rate)).await;
                            elapsed_ms += tick_rate;
                            continue;
                        }
                    }
                }
                // --------------------

                tokio::time::sleep(Duration::from_millis(tick_rate)).await;
                elapsed_ms += tick_rate;

                if !use_energy_timing && pause_remaining > 0.0 {
                    pause_remaining -= tick_rate as f32 / 1000.0;
                    continue;
                }

                // Update index
                let step = active_chars_per_sec * (tick_rate as f32 / 1000.0);
                current_char_index += step;

                let idx = current_char_index as usize;

                if idx >= sim_len {
                    sim_last_word_index.store(sim_len, Ordering::SeqCst);
                    sim_last_word_length.store(0, Ordering::SeqCst);
                    break;
                }

                // Punctuation check (ONLY if not using energy timing)
                if !use_energy_timing && idx > last_processed_idx {
                    for i in last_processed_idx..idx {
                        if i < sim_len {
                            let c = sim_text.as_bytes()[i] as char;
                            if c == ',' || c == ';' {
                                pause_remaining = pause_time_per_comma;
                            } else if c == '.' || c == '!' || c == '?' {
                                pause_remaining = pause_time_per_period;
                            }
                        }
                    }
                    last_processed_idx = idx;
                }

                // Snap to word boundaries
                let start_idx = sim_text[..idx]
                    .rfind(char::is_whitespace)
                    .map(|i| i + 1)
                    .unwrap_or(0);

                let end_offset = sim_text[idx..]
                    .find(char::is_whitespace)
                    .unwrap_or(sim_len - idx);
                let end_idx = idx + end_offset;

                let word_len = end_idx - start_idx;

                if word_len > 0 {
                    sim_last_word_index.store(start_idx, Ordering::SeqCst);
                    sim_last_word_length.store(word_len, Ordering::SeqCst);
                }
            }
        });
    }
}
