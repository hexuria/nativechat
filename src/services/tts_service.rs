use crate::services::audio_output::{AudioCommand, AudioOutput};
use crate::services::audio_transcription::{AudioTranscriptionService, WordTiming};
use crate::services::live_tts_provider::LiveTtsProvider;
use crate::services::macos_tts_bridge::{MacTtsBridge, TtsEvent};
use crate::services::rest_tts_provider::RestTtsProvider;
use crate::services::tts_provider::TtsProvider;
use crate::tts_text::is_live_tts_model;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Notify;

#[derive(Clone)]
pub struct TtsService {
    live_provider: Arc<LiveTtsProvider>,
    rest_provider: Arc<RestTtsProvider>,
    audio_output: Arc<AudioOutput>,
    /// Used to cancel ongoing streaming when stop() is called
    cancelled: Arc<AtomicBool>,
    #[cfg(target_os = "macos")]
    native_provider: Option<Arc<MacTtsBridge>>,
    native_completion_notify: Arc<Notify>,
    pub native_paused: Arc<AtomicBool>,
    pub ai_paused: Arc<AtomicBool>,

    last_word_index: Arc<AtomicUsize>,
    last_word_length: Arc<AtomicUsize>,
    generation: Arc<AtomicU64>,
    native_generation: Arc<AtomicU64>,

    /// Transcription service for getting word timestamps
    transcription_service: Arc<AudioTranscriptionService>,
    /// Current word timings (from transcription) - None means use simulation fallback
    word_timings: Arc<RwLock<Option<Vec<WordTiming>>>>,
    /// Playback start time for timestamp-based highlighting
    playback_start_time: Arc<RwLock<Option<std::time::Instant>>>,
}

impl TtsService {
    pub fn new(is_ai_speaking: Arc<AtomicBool>, ai_amplitude: Arc<AtomicU32>) -> Result<Self> {
        let audio_output = Arc::new(AudioOutput::new(is_ai_speaking, ai_amplitude)?);

        let native_completion = Arc::new(Notify::new());
        let last_word_index = Arc::new(AtomicUsize::new(0));
        let last_word_length = Arc::new(AtomicUsize::new(0));
        let generation = Arc::new(AtomicU64::new(0));
        let native_generation = Arc::new(AtomicU64::new(0));

        let native_provider = if cfg!(target_os = "macos") {
            let bridge = Arc::new(MacTtsBridge::new());
            let completion = native_completion.clone();
            let index = last_word_index.clone();
            let len = last_word_length.clone();

            bridge.set_callback(move |event| match event {
                TtsEvent::Start => {
                    index.store(0, Ordering::SeqCst);
                    len.store(0, Ordering::SeqCst);
                }
                TtsEvent::Word { start, length } => {
                    index.store(start, Ordering::SeqCst);
                    len.store(length, Ordering::SeqCst);
                }
                TtsEvent::Finish => {
                    completion.notify_waiters();
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
            native_completion_notify: native_completion,
            native_paused: Arc::new(AtomicBool::new(false)),
            ai_paused: Arc::new(AtomicBool::new(false)),
            last_word_index,
            last_word_length,
            generation,
            native_generation,
            transcription_service: Arc::new(AudioTranscriptionService::new()),
            word_timings: Arc::new(RwLock::new(None)),
            playback_start_time: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the active word range for highlighting
    /// Uses timestamp-based highlighting when word timings are available,
    /// falls back to simulation-based when not
    pub fn get_active_word_range(&self) -> Option<std::ops::Range<usize>> {
        // First, try timestamp-based highlighting
        if let Ok(timings_guard) = self.word_timings.read() {
            if let Some(ref timings) = *timings_guard {
                if let Ok(start_guard) = self.playback_start_time.read() {
                    if let Some(start_time) = *start_guard {
                        // Account for pause time
                        if self.ai_paused.load(Ordering::SeqCst) {
                            // While paused, keep the last word highlighted
                            let start = self.last_word_index.load(Ordering::SeqCst);
                            let len = self.last_word_length.load(Ordering::SeqCst);
                            if len > 0 {
                                return Some(start..start + len);
                            }
                        }

                        // Signal-Driven Timing: Use actual audio samples played for precision
                        // This eliminates drift between wall clock and audio clock
                        // Subtract 120ms to compensate for output buffer and hardware latency
                        let elapsed =
                            (self.audio_output.controller.get_playback_position() - 0.12).max(0.0);

                        // Find the word that should be highlighted at this time
                        for timing in timings.iter() {
                            if elapsed >= timing.start_time && elapsed < timing.end_time {
                                // Update the atomic values for compatibility
                                self.last_word_index
                                    .store(timing.source_range.start, Ordering::SeqCst);
                                self.last_word_length
                                    .store(timing.source_range.len(), Ordering::SeqCst);
                                return Some(timing.source_range.clone());
                            }
                        }

                        // Past all words - return None (finished)
                        if !timings.is_empty() {
                            let last = timings.last().unwrap();
                            if elapsed >= last.end_time {
                                self.last_word_length.store(0, Ordering::SeqCst);
                                return None;
                            }
                        }
                    }
                }
            }
        }

        // Fallback to simulation-based highlighting
        let start = self.last_word_index.load(Ordering::SeqCst);
        let len = self.last_word_length.load(Ordering::SeqCst);
        if len > 0 {
            Some(start..start + len)
        } else {
            None
        }
    }

    pub fn is_live_api_model(model_id: &str) -> bool {
        is_live_tts_model(model_id)
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
            self.native_generation.fetch_add(1, Ordering::SeqCst);
            bridge.stop();
            self.audio_output.process_command(AudioCommand::Pause);
            self.ai_paused.store(true, Ordering::SeqCst);
            self.native_paused.store(false, Ordering::SeqCst);
            self.last_word_index.store(0, Ordering::SeqCst);
            self.last_word_length.store(0, Ordering::SeqCst);
            // macOS provides UTF-16 word ranges into this text
            bridge.speak(text);
            return true;
        }

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
            self.native_generation.fetch_add(1, Ordering::SeqCst);
            bridge.stop();
            self.native_paused.store(false, Ordering::SeqCst);
            self.last_word_length.store(0, Ordering::SeqCst);
            self.native_completion_notify.notify_waiters();
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

        self.cancelled.store(false, Ordering::SeqCst);
        self.ai_paused.store(false, Ordering::SeqCst);
        self.last_word_index.store(0, Ordering::SeqCst);
        self.last_word_length.store(0, Ordering::SeqCst);

        // Clear previous word timings and playback timing
        if let Ok(mut guard) = self.word_timings.write() {
            *guard = None;
        }
        if let Ok(mut guard) = self.playback_start_time.write() {
            *guard = None;
        }

        // Reset audio output state to clear any previous stuck signals or data
        self.audio_output.process_command(AudioCommand::Stop);

        // **NATIVE TTS FAST PATH**
        if model_id == "native" {
            return Ok(self.start_speaking_native(text, message_id));
        }

        // Check cache first - if cached, play immediately without network request
        if let Some(path) = Self::get_cache_path(message_id) {
            if path.exists() {
                if let Ok(bytes) = std::fs::read(&path) {
                    let samples = Self::bytes_to_samples(&bytes);
                    if !samples.is_empty() {
                        let duration_secs = samples.len() as f32 / 24000.0;
                        let duration = Some(Duration::from_secs_f32(duration_secs));

                        // Try to load cached word timings
                        let timings_path = path.with_extension("timings.json");
                        let mut timings_loaded = false;

                        if timings_path.exists() {
                            if let Ok(timings_bytes) = std::fs::read(&timings_path) {
                                if let Ok(timings) =
                                    serde_json::from_slice::<Vec<WordTiming>>(&timings_bytes)
                                {
                                    // Store timings and set playback start time
                                    if let Ok(mut guard) = self.word_timings.write() {
                                        *guard = Some(timings);
                                    }
                                    if let Ok(mut guard) = self.playback_start_time.write() {
                                        *guard = Some(std::time::Instant::now());
                                    }
                                    timings_loaded = true;
                                }
                            }
                        } else {
                            // Spawn background task to transcribe the audio
                            let transcription_service = self.transcription_service.clone();
                            let samples_for_transcription = samples.clone();
                            let text_for_transcription = text.to_string();
                            let api_key_owned = api_key.to_string();
                            let timings_path_owned = timings_path.clone();
                            let word_timings_ref = self.word_timings.clone();
                            let playback_start_time_ref = self.playback_start_time.clone();

                            tokio::spawn(async move {
                                match transcription_service
                                    .transcribe_with_timestamps(
                                        &samples_for_transcription,
                                        &text_for_transcription,
                                        &api_key_owned,
                                    )
                                    .await
                                {
                                    Ok(timings) => {
                                        if !timings.is_empty() {
                                            // Save to cache
                                            if let Ok(json) = serde_json::to_vec(&timings) {
                                                let _ = std::fs::write(&timings_path_owned, json);
                                            }
                                            // Update live timings (in case playback is still happening)
                                            if let Ok(mut guard) = word_timings_ref.write() {
                                                *guard = Some(timings);
                                            }
                                            if let Ok(mut guard) = playback_start_time_ref.write() {
                                                // Reset playback start time since we now have accurate timings
                                                // Note: This might cause a slight jump, but subsequent plays will be accurate
                                                *guard = Some(std::time::Instant::now());
                                            }
                                        }
                                    }
                                    Err(_) => {}
                                }
                            });
                        }

                        self.audio_output
                            .process_command(AudioCommand::Samples(samples.clone()));

                        // Spawn simulation as fallback only if no timings loaded
                        if !timings_loaded {
                            self.spawn_simulation_task(
                                text.to_string(),
                                current_gen,
                                duration,
                                Some(samples),
                            );
                        }

                        return Ok(true);
                    }
                }
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
        let text_owned = text.to_string();
        let api_key_owned = api_key.to_string();
        let transcription_service = self.transcription_service.clone();

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
                for sample in &all_samples {
                    let s = (*sample * 32768.0).clamp(-32768.0, 32767.0) as i16;
                    bytes.extend_from_slice(&s.to_le_bytes());
                }

                if let Some(path) = TtsService::get_cache_path(&message_id_owned) {
                    let _ = std::fs::write(&path, &bytes);

                    // Transcribe audio to get word timings for future playback
                    match transcription_service
                        .transcribe_with_timestamps(&all_samples, &text_owned, &api_key_owned)
                        .await
                    {
                        Ok(timings) => {
                            if !timings.is_empty() {
                                let timings_path = path.with_extension("timings.json");
                                if let Ok(json) = serde_json::to_vec(&timings) {
                                    let _ = std::fs::write(timings_path, json);
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
        });

        Ok(true)
    }

    /// Wait for Native TTS to complete
    pub async fn wait_until_finished_native(&self) {
        #[cfg(target_os = "macos")]
        if let Some(bridge) = &self.native_provider {
            let started = self.native_generation.load(Ordering::SeqCst);
            loop {
                if self.native_generation.load(Ordering::SeqCst) != started {
                    return;
                }
                let speaking = bridge.is_speaking();
                let paused = self.native_paused.load(Ordering::SeqCst);
                if !speaking && !paused {
                    return;
                }
                tokio::select! {
                    _ = self.native_completion_notify.notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            }
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

        // Clear word timings when stopping
        if let Ok(mut guard) = self.word_timings.write() {
            *guard = None;
        }
        if let Ok(mut guard) = self.playback_start_time.write() {
            *guard = None;
        }

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
            if let Some(bridge) = &self.native_provider {
                return bridge.is_speaking() && !self.native_paused.load(Ordering::SeqCst);
            }
        }
        false
    }

    /// Spawn task that updates word highlighting based on proportional word timing.
    /// Uses the actual audio duration and distributes it proportionally across words
    /// based on weighted length (longer words + punctuation take more time).
    fn spawn_simulation_task(
        &self,
        text: String,
        current_gen: u64,
        duration: Option<Duration>,
        samples: Option<Vec<f32>>,
    ) {
        let sim_last_word_index = self.last_word_index.clone();
        let sim_last_word_length = self.last_word_length.clone();
        let sim_cancelled = self.cancelled.clone();
        let sim_generation = self.generation.clone();
        
        // Use audio controller for precise timing
        let samples_played = self.audio_output.controller.samples_played.clone();
        let sample_rate = self.audio_output.controller.output_sample_rate;

        tokio::spawn(async move {
            // Calculate audio duration
            let audio_duration = if let Some(ref s) = samples {
                s.len() as f32 / 24000.0
            } else if let Some(d) = duration {
                d.as_secs_f32()
            } else {
                // Fallback: estimate based on CLEAN text length
                // Use a heuristic that ignores markdown syntax for better accuracy
                let clean_len = text.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).count();
                clean_len as f32 / 15.0
            };

            // Parse words from text and calculate their byte positions
            #[derive(Clone)]
            struct WordInfo {
                start_byte: usize,
                end_byte: usize,
                weight: f32,
            }

            let mut words: Vec<WordInfo> = Vec::new();
            let mut in_word = false;
            let mut word_start = 0;

            for (i, c) in text.char_indices() {
                if c.is_whitespace() {
                    if in_word {
                        // End of word
                        let word_text = &text[word_start..i];
                        
                        // Calculate weight based on SPOKEN content (alphanumeric)
                        // This ignores Markdown symbols like ** or [] which aren't spoken
                        let clean_chars = word_text.chars().filter(|c| c.is_alphanumeric()).count();
                        let mut weight = clean_chars as f32;
                        
                        // Fallback for symbols that might be spoken or just empty
                        if weight == 0.0 && !word_text.is_empty() {
                            weight = 0.5; 
                        }

                        // Add weight for punctuation (pauses)
                        if word_text.contains(',') || word_text.contains(';') {
                            weight += 3.0;
                        }
                        if word_text.contains('.')
                            || word_text.contains('!')
                            || word_text.contains('?')
                        {
                            weight += 5.0; // Slightly longer pause for sentences
                        }
                        // Longer words are spoken slower
                        if clean_chars > 7 {
                            weight += 2.0;
                        }

                        words.push(WordInfo {
                            start_byte: word_start,
                            end_byte: i,
                            weight,
                        });
                        in_word = false;
                    }
                } else {
                    if !in_word {
                        word_start = i;
                        in_word = true;
                    }
                }
            }
            // Handle last word
            if in_word {
                let word_text = &text[word_start..];
                let clean_chars = word_text.chars().filter(|c| c.is_alphanumeric()).count();
                let mut weight = clean_chars as f32;
                
                if weight == 0.0 && !word_text.is_empty() {
                     weight = 0.5;
                }

                if word_text.contains(',') || word_text.contains(';') {
                    weight += 3.0;
                }
                if word_text.contains('.') || word_text.contains('!') || word_text.contains('?') {
                    weight += 5.0;
                }
                if clean_chars > 7 {
                    weight += 2.0;
                }
                words.push(WordInfo {
                    start_byte: word_start,
                    end_byte: text.len(),
                    weight,
                });
            }

            if words.is_empty() {
                return;
            }

            // Calculate total weight and time per unit
            let total_weight: f32 = words.iter().map(|w| w.weight).sum();
            let time_per_unit = audio_duration / total_weight.max(1.0);

            // Calculate start and end times for each word
            let mut word_timings: Vec<(f32, f32, usize, usize)> = Vec::new(); // (start_time, end_time, start_byte, end_byte)
            let mut cumulative_time = 0.0;

            for word in &words {
                let word_duration = word.weight * time_per_unit;
                word_timings.push((
                    cumulative_time,
                    cumulative_time + word_duration,
                    word.start_byte,
                    word.end_byte,
                ));
                cumulative_time += word_duration;
            }

            // Now run the timing loop
            let tick_rate = 30; // 30ms for smoother updates

            loop {
                if sim_cancelled.load(Ordering::SeqCst)
                    || sim_generation.load(Ordering::SeqCst) != current_gen
                {
                    break;
                }

                // Use actual playback position from audio controller
                // This handles pauses, buffering, and hardware latency automatically
                // Subtract a small offset (e.g. 100ms) to sync better with output buffer
                let playback_samples = samples_played.load(Ordering::Relaxed);
                let elapsed = (playback_samples as f32 / sample_rate as f32 - 0.1).max(0.0);

                // Find the current word based on elapsed time
                let mut found_word = false;
                for (word_start, word_end, start_byte, end_byte) in &word_timings {
                    if elapsed >= *word_start && elapsed < *word_end {
                        sim_last_word_index.store(*start_byte, Ordering::SeqCst);
                        sim_last_word_length.store(end_byte - start_byte, Ordering::SeqCst);
                        found_word = true;
                        break;
                    }
                }

                // Past all words - finished
                if !found_word && elapsed >= cumulative_time {
                    // Only finish if we've actually played enough audio
                    if elapsed > 0.0 {
                        sim_last_word_length.store(0, Ordering::SeqCst);
                        break;
                    }
                }

                tokio::time::sleep(Duration::from_millis(tick_rate)).await;
            }

            // Clear highlighting when done
            sim_last_word_length.store(0, Ordering::SeqCst);
        });
    }
}
