use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Audio samples from TTS provider
pub struct AudioChunk {
    /// Normalized f32 samples (mono, 24kHz usually)
    pub samples: Vec<f32>,
}

/// Trait for TTS providers - REST or WebSocket based
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Stream audio chunks for the given text
    /// Returns a channel receiver that yields audio chunks as they arrive
    async fn stream_audio(
        &self,
        text: &str,
        model_id: &str,
        api_key: &str,
        voice: &Option<String>,
    ) -> Result<mpsc::Receiver<Result<AudioChunk>>>;

    /// Provider name for logging/debugging
    fn name(&self) -> &'static str;
}
