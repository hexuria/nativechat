use crate::services::audio_output::{AudioCommand, AudioOutput};
use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
use futures::StreamExt;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32};

#[derive(Clone)]
pub struct TtsService {
    client: Client,
    audio_output: Arc<AudioOutput>,
}

impl TtsService {
    pub fn new(is_ai_speaking: Arc<AtomicBool>, ai_amplitude: Arc<AtomicU32>) -> Result<Self> {
        let audio_output = Arc::new(AudioOutput::new(is_ai_speaking, ai_amplitude)?);
        Ok(Self {
            client: Client::new(),
            audio_output,
        })
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

    /// Extract audio data from a Gemini TTS response JSON
    fn extract_audio_data(response_json: &serde_json::Value) -> Option<Vec<u8>> {
        response_json
            .get("candidates")?
            .get(0)?
            .get("content")?
            .get("parts")?
            .get(0)?
            .get("inlineData")?
            .get("data")?
            .as_str()
            .and_then(|base64_str| general_purpose::STANDARD.decode(base64_str).ok())
    }

    /// Start speaking with streaming - audio plays as chunks arrive.
    /// Returns Ok(true) if audio started (caller should set speaking state),
    /// Returns Ok(false) if nothing to play.
    /// The audio will continue playing in the background.
    pub async fn start_speaking(
        &self,
        text: &str,
        message_id: &str,
        model_id: &str,
        api_key: &str,
    ) -> Result<bool> {
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

        // Use streaming endpoint for better UX with long text
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?key={}&alt=sse",
            model_id, api_key
        );

        let payload = json!({
            "contents": [{
                "parts":[{
                    "text": text
                }]
            }],
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "speechConfig": {
                    "voiceConfig": {
                        "prebuiltVoiceConfig": {
                            "voiceName": "Kore"
                        }
                    }
                }
            }
        });

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Failed to send TTS request")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("TTS API error: {}", error_text));
        }

        // Stream audio chunks as they arrive (SSE format)
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut all_audio_bytes: Vec<u8> = Vec::new();
        let mut audio_started = false;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));

                    // Parse SSE events (same pattern as chat_stream in gemini.rs)
                    loop {
                        let p1 = buffer.find("\n\n");
                        let p2 = buffer.find("\r\n\r\n");

                        let (pos, len) = match (p1, p2) {
                            (Some(i), Some(j)) => {
                                if i < j {
                                    (i, 2)
                                } else {
                                    (j, 4)
                                }
                            }
                            (Some(i), None) => (i, 2),
                            (None, Some(j)) => (j, 4),
                            (None, None) => break,
                        };

                        let event = buffer[..pos].to_string();
                        buffer = buffer[pos + len..].to_string();

                        if let Some(data) = event.strip_prefix("data: ") {
                            if let Ok(resp) = serde_json::from_str::<serde_json::Value>(data) {
                                if let Some(audio_bytes) = Self::extract_audio_data(&resp) {
                                    // Accumulate for caching
                                    all_audio_bytes.extend_from_slice(&audio_bytes);

                                    // Play immediately for low latency
                                    let samples = Self::bytes_to_samples(&audio_bytes);
                                    if !samples.is_empty() {
                                        self.audio_output
                                            .process_command(AudioCommand::Samples(samples));
                                        audio_started = true;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("TTS stream error: {}", e);
                    break;
                }
            }
        }

        // Save complete audio to cache for future playback
        if !all_audio_bytes.is_empty() {
            if let Some(path) = Self::get_cache_path(message_id) {
                let _ = std::fs::write(path, &all_audio_bytes);
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
        self.audio_output.process_command(AudioCommand::Stop);
    }

    pub fn pause(&self) {
        self.audio_output.process_command(AudioCommand::Pause);
    }

    pub fn resume(&self) {
        self.audio_output.process_command(AudioCommand::Resume);
    }
}
