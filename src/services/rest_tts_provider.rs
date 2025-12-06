use super::tts_provider::{AudioChunk, TtsProvider};
use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use futures::StreamExt;
use reqwest::Client;
use serde_json::json;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct RestTtsProvider {
    client: Client,
}

impl RestTtsProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
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
}

#[async_trait]
impl TtsProvider for RestTtsProvider {
    async fn stream_audio(
        &self,
        text: &str,
        model_id: &str,
        api_key: &str,
    ) -> Result<mpsc::Receiver<Result<AudioChunk>>> {
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

        let (tx, rx) = mpsc::channel(100);
        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Parse SSE events
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
                                        let samples = Self::bytes_to_samples(&audio_bytes);
                                        if !samples.is_empty() {
                                            if tx.send(Ok(AudioChunk { samples })).await.is_err() {
                                                return; // Receiver dropped
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("Stream error: {}", e))).await;
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    fn name(&self) -> &'static str {
        "REST (SSE)"
    }
}
