use crate::services::audio_output::{AudioCommand, AudioOutput};
use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
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

    pub async fn speak(
        &self,
        text: &str,
        message_id: &str,
        model_id: &str,
        api_key: &str,
    ) -> Result<()> {
        // Check cache first
        if let Some(path) = Self::get_cache_path(message_id) {
            if path.exists() {
                if let Ok(bytes) = std::fs::read(&path) {
                    let mut samples = Vec::with_capacity(bytes.len() / 2);
                    for chunk in bytes.chunks_exact(2) {
                        let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
                        samples.push(sample);
                    }
                    if !samples.is_empty() {
                        self.audio_output
                            .process_command(AudioCommand::Samples(samples));
                        self.audio_output.wait_until_finished().await;
                        return Ok(());
                    }
                }
            }
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
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
                            "voiceName": "Kore" // Default voice, maybe make configurable later
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

        let response_json: serde_json::Value = response.json().await?;

        if let Some(candidates) = response_json.get("candidates")
            && let Some(candidate) = candidates.get(0)
            && let Some(content) = candidate.get("content")
            && let Some(parts) = content.get("parts")
            && let Some(part) = parts.get(0)
            && let Some(inline_data) = part.get("inlineData")
            && let Some(data) = inline_data.get("data")
            && let Some(base64_str) = data.as_str()
        {
            let bytes = general_purpose::STANDARD
                .decode(base64_str)
                .context("Failed to decode base64 audio data")?;

            // Save to cache
            if let Some(path) = Self::get_cache_path(message_id) {
                let _ = std::fs::write(path, &bytes);
            }

            // PCM 16-bit LE -> f32
            let mut samples = Vec::with_capacity(bytes.len() / 2);
            for chunk in bytes.chunks_exact(2) {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
                samples.push(sample);
            }

            if !samples.is_empty() {
                self.audio_output
                    .process_command(AudioCommand::Samples(samples));
                self.audio_output.wait_until_finished().await;
            }
        } else {
            return Err(anyhow::anyhow!("Invalid response format from TTS API"));
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
