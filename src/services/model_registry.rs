use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Provider {
    Gemini,
    OpenAI,
    Anthropic,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Gemini => write!(f, "Google Gemini"),
            Provider::OpenAI => write!(f, "OpenAI"),
            Provider::Anthropic => write!(f, "Anthropic"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ModelType {
    TextGeneration,
    ImageGeneration,
    TextEmbedding,
    AudioGeneration,
    SpeechRecognition,
    Moderation,
}

impl Default for ModelType {
    fn default() -> Self {
        Self::TextGeneration
    }
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::TextGeneration => write!(f, "Text Generation"),
            ModelType::ImageGeneration => write!(f, "Image Generation"),
            ModelType::TextEmbedding => write!(f, "Text Embedding"),
            ModelType::AudioGeneration => write!(f, "Audio Generation"),
            ModelType::SpeechRecognition => write!(f, "Speech Recognition"),
            ModelType::Moderation => write!(f, "Moderation"),
        }
    }
}

impl ui::SelectItem for Provider {
    type Value = Self;

    fn title(&self) -> gpui::SharedString {
        match self {
            Provider::Gemini => "Google Gemini".into(),
            Provider::OpenAI => "OpenAI".into(),
            Provider::Anthropic => "Anthropic".into(),
        }
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub supports_vision: bool,
    pub supports_function_calling: bool,
    pub supports_web_search: bool,
    pub supports_file_search: bool,
    pub supports_image_generation: bool,
    pub supports_code_interpreter: bool,
    pub supports_computer_use: bool,
    pub supports_streaming: bool,
    pub supports_reasoning: bool,
    pub supports_text_generation: bool,
    pub supports_embedding: bool,
    pub supports_text_to_speech: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            supports_vision: false,
            supports_function_calling: false,
            supports_web_search: false,
            supports_file_search: false,
            supports_image_generation: false,
            supports_code_interpreter: false,
            supports_computer_use: false,
            supports_streaming: true,
            supports_reasoning: false,
            supports_text_generation: true,
            supports_embedding: false,
            supports_text_to_speech: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelProfile {
    pub provider: Provider,
    pub id: String,
    pub display_name: String,
    pub created_at: Option<u64>,
    pub input_token_limit: Option<u32>,
    pub output_token_limit: Option<u32>,
    pub capabilities: ModelCapabilities,
    pub model_type: ModelType,
    pub is_thinking: bool,
}

impl ui::SelectItem for ModelProfile {
    type Value = String;

    fn title(&self) -> gpui::SharedString {
        self.display_name.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }

    fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        self.display_name.to_lowercase().contains(&query) || self.id.to_lowercase().contains(&query)
    }
}

pub struct ModelRegistry {
    client: Client,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Classify model capabilities based on model ID and supported generation methods
    fn classify_model_capabilities(
        model_id: &str,
        provider: &Provider,
        supported_methods: Option<&[String]>,
    ) -> (ModelCapabilities, ModelType) {
        let id_lower = model_id.to_lowercase();

        // Check if it's an embedding model
        let is_embedding = id_lower.contains("embedding")
            || id_lower.contains("embed")
            || (supported_methods.is_some()
                && supported_methods
                    .unwrap()
                    .iter()
                    .any(|m| m.contains("embed")));

        // Check if it's an image generation model
        let is_image_gen = id_lower.contains("dall-e")
            || id_lower.contains("imagen")
            || id_lower.contains("sora")
            || id_lower.contains("gpt-image")
            || id_lower.contains("nano-banana");

        // Check if it's a moderation or transcription model (should be filtered out)
        let is_utility = id_lower.contains("moderation")
            || id_lower.contains("whisper")
            || id_lower.contains("tts-")
            || id_lower.contains("transcribe");

        if is_embedding {
            return (
                ModelCapabilities {
                    supports_text_generation: false,
                    supports_embedding: true,
                    supports_streaming: false,
                    ..Default::default()
                },
                ModelType::TextEmbedding,
            );
        }

        if is_image_gen {
            return (
                ModelCapabilities {
                    supports_text_generation: false,
                    supports_image_generation: true,
                    supports_streaming: false,
                    ..Default::default()
                },
                ModelType::ImageGeneration,
            );
        }

        if is_utility {
            let model_type = if id_lower.contains("whisper") {
                ModelType::SpeechRecognition
            } else if id_lower.contains("tts") {
                ModelType::AudioGeneration
            } else {
                ModelType::Moderation
            };

            let mut caps = ModelCapabilities::default();
            caps.supports_text_generation = false;

            if model_type == ModelType::AudioGeneration {
                caps.supports_text_to_speech = true;
            }

            return (caps, model_type);
        }

        // Default to text generation model with provider-specific features
        let mut caps = ModelCapabilities {
            supports_text_generation: true,
            supports_streaming: true,
            ..Default::default()
        };
        let model_type = ModelType::TextGeneration;

        // Enhanced capabilities based on model name patterns
        match provider {
            Provider::Gemini => {
                // Gemini models with specific capabilities
                if id_lower.contains("gemini") {
                    caps.supports_vision = id_lower.contains("2.0")
                        || id_lower.contains("2.5")
                        || id_lower.contains("1.5");
                    caps.supports_function_calling = true;
                    caps.supports_web_search = true;
                    caps.supports_file_search = true;
                    caps.supports_file_search = true;
                    caps.supports_code_interpreter = true;
                    if id_lower.contains("tts") {
                        caps.supports_text_to_speech = true;
                    }
                }

                // Check for thinking/reasoning models
                if id_lower.contains("thinking")
                    || supported_methods.is_some()
                        && supported_methods
                            .unwrap()
                            .iter()
                            .any(|m| m.to_lowercase().contains("thinking"))
                {
                    caps.supports_reasoning = true;
                }

                // Check for computer use
                if id_lower.contains("computer-use") {
                    caps.supports_computer_use = true;
                }
            }
            Provider::OpenAI => {
                // GPT 4+ models have vision
                if id_lower.starts_with("gpt-4") || id_lower.starts_with("gpt-5") {
                    caps.supports_vision = id_lower.contains("gpt-4o")
                        || id_lower.contains("gpt-4-turbo")
                        || id_lower.contains("gpt-5");
                    caps.supports_function_calling = true;
                    caps.supports_code_interpreter = true;
                }

                // O-series models (reasoning)
                if id_lower.starts_with("o1")
                    || id_lower.starts_with("o3")
                    || id_lower.starts_with("o4")
                {
                    caps.supports_reasoning = true;
                    caps.supports_function_calling = true;
                }

                // Search-enabled models
                if id_lower.contains("search") {
                    caps.supports_web_search = true;
                }
            }
            Provider::Anthropic => {
                // All Claude models support vision and function calling (except very old ones)
                if id_lower.contains("claude") {
                    // Extract version number - models like "claude-3-opus", "claude-opus-4-5", etc
                    // Claude 3+ has vision support
                    let has_version_3_or_higher = id_lower.contains("-3-")
                        || id_lower.contains("-4-")
                        || id_lower.contains("-5-")
                        || id_lower.contains("opus-3")
                        || id_lower.contains("opus-4")
                        || id_lower.contains("opus-5")
                        || id_lower.contains("sonnet-3")
                        || id_lower.contains("sonnet-4")
                        || id_lower.contains("sonnet-5")
                        || id_lower.contains("haiku-3")
                        || id_lower.contains("haiku-4")
                        || id_lower.contains("haiku-5");

                    caps.supports_vision = has_version_3_or_higher;
                    caps.supports_function_calling = true;
                    caps.supports_code_interpreter = true;

                    // Claude 3.5+ Sonnet has computer use
                    if id_lower.contains("sonnet")
                        && (id_lower.contains("-4-")
                            || id_lower.contains("-5-")
                            || id_lower.contains("sonnet-4")
                            || id_lower.contains("sonnet-5"))
                    {
                        caps.supports_computer_use = true;
                    }
                }
            }
        }

        (caps, model_type)
    }

    pub async fn get_all_models(&self, api_keys: &HashMap<Provider, String>) -> Vec<ModelProfile> {
        let mut all_models = Vec::new();

        let gemini_key = api_keys.get(&Provider::Gemini).cloned();
        if let Ok(gemini_models) = self.fetch_gemini_models(gemini_key).await {
            all_models.extend(gemini_models);
        } else {
            eprintln!("Failed to fetch Gemini models");
        }

        let openai_key = api_keys.get(&Provider::OpenAI).cloned();
        if let Ok(openai_models) = self.fetch_openai_models(openai_key).await {
            all_models.extend(openai_models);
        } else {
            eprintln!("Failed to fetch OpenAI models");
        }

        let anthropic_key = api_keys.get(&Provider::Anthropic).cloned();
        if let Ok(anthropic_models) = self.fetch_anthropic_models(anthropic_key).await {
            all_models.extend(anthropic_models);
        } else {
            eprintln!("Failed to fetch Anthropic models");
        }

        all_models
    }

    async fn fetch_gemini_models(&self, api_key: Option<String>) -> Result<Vec<ModelProfile>> {
        let api_key = api_key
            .or_else(|| std::env::var("GEMINI_API_KEY").ok())
            .unwrap_or_default();
        if api_key.is_empty() {
            return Ok(vec![]);
        }

        #[derive(Deserialize)]
        struct GeminiModel {
            name: String,
            #[serde(rename = "displayName")]
            display_name: Option<String>,
            #[serde(rename = "inputTokenLimit")]
            input_token_limit: Option<u32>,
            #[serde(rename = "outputTokenLimit")]
            output_token_limit: Option<u32>,
            #[serde(rename = "supportedGenerationMethods")]
            supported_generation_methods: Option<Vec<String>>,
        }

        #[derive(Deserialize)]
        struct ListModelsResponse {
            models: Option<Vec<GeminiModel>>,
            #[serde(rename = "nextPageToken")]
            next_page_token: Option<String>,
        }

        let mut all_models = Vec::new();
        let mut next_page_token: Option<String> = None;

        loop {
            let mut url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                api_key
            );
            if let Some(token) = &next_page_token {
                url.push_str(&format!("&pageToken={}", token));
            }

            let response: ListModelsResponse = self.client.get(&url).send().await?.json().await?;

            if let Some(models) = response.models {
                let mapped_models: Vec<ModelProfile> = models
                    .into_iter()
                    .filter_map(|m| {
                        // name is like "models/gemini-pro"
                        let id = m.name.replace("models/", "");

                        // Filter out non-generative models
                        if id.to_lowercase().contains("embedding-gecko") {
                            return None;
                        }

                        let (capabilities, model_type) = Self::classify_model_capabilities(
                            &id,
                            &Provider::Gemini,
                            m.supported_generation_methods.as_deref(),
                        );

                        let is_thinking = capabilities.supports_reasoning;

                        Some(ModelProfile {
                            provider: Provider::Gemini,
                            id: id.clone(),
                            display_name: m.display_name.unwrap_or(id),
                            created_at: None,
                            input_token_limit: m.input_token_limit,
                            output_token_limit: m.output_token_limit,
                            capabilities,
                            model_type,
                            is_thinking,
                        })
                    })
                    .collect();
                all_models.extend(mapped_models);
            }

            next_page_token = response.next_page_token;
            if next_page_token.is_none() {
                break;
            }
        }

        Ok(all_models)
    }

    async fn fetch_openai_models(&self, api_key: Option<String>) -> Result<Vec<ModelProfile>> {
        let api_key = api_key
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .unwrap_or_default();
        if api_key.is_empty() {
            return Ok(vec![]);
        }

        #[derive(Deserialize)]
        struct OpenAIModel {
            id: String,
            created: u64,
        }

        #[derive(Deserialize)]
        struct ListModelsResponse {
            data: Vec<OpenAIModel>,
        }

        let response: ListModelsResponse = self
            .client
            .get("https://api.openai.com/v1/models")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await?
            .json()
            .await?;

        let models = response
            .data
            .into_iter()
            .filter_map(|m| {
                let id_lower = m.id.to_lowercase();

                // Filter out utility models (TTS, moderation, transcription)
                // We want to keep TTS models now
                if id_lower.contains("whisper")
                    || id_lower.contains("moderation")
                    || id_lower.contains("transcribe")
                    || id_lower.contains("davinci-002")
                    || id_lower.contains("babbage-002")
                {
                    return None;
                }

                let (capabilities, model_type) =
                    Self::classify_model_capabilities(&m.id, &Provider::OpenAI, None);

                let is_thinking = capabilities.supports_reasoning;

                Some(ModelProfile {
                    provider: Provider::OpenAI,
                    id: m.id.clone(),
                    display_name: m.id, // OpenAI doesn't provide separate display names
                    created_at: Some(m.created),
                    input_token_limit: None, // Not provided in models API
                    output_token_limit: None,
                    capabilities,
                    model_type,
                    is_thinking,
                })
            })
            .collect();

        Ok(models)
    }

    async fn fetch_anthropic_models(&self, api_key: Option<String>) -> Result<Vec<ModelProfile>> {
        let api_key = api_key
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .unwrap_or_default();
        if api_key.is_empty() {
            return Ok(vec![]);
        }

        #[derive(Deserialize)]
        struct AnthropicModel {
            id: String,
            display_name: String,
            created_at: Option<String>,
        }

        #[derive(Deserialize)]
        struct ListModelsResponse {
            data: Vec<AnthropicModel>,
        }

        let response: ListModelsResponse = self
            .client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await?
            .json()
            .await?;

        let models = response
            .data
            .into_iter()
            .map(|m| {
                let (capabilities, model_type) =
                    Self::classify_model_capabilities(&m.id, &Provider::Anthropic, None);

                let is_thinking = capabilities.supports_reasoning;

                ModelProfile {
                    provider: Provider::Anthropic,
                    id: m.id.clone(),
                    display_name: m.display_name,
                    created_at: None, // TODO: Parse timestamp
                    input_token_limit: None,
                    output_token_limit: None,
                    capabilities,
                    model_type,
                    is_thinking,
                }
            })
            .collect();

        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_gemini_text_models() {
        let caps = ModelRegistry::classify_model_capabilities(
            "gemini-2.0-flash",
            &Provider::Gemini,
            Some(&["generateContent".to_string(), "countTokens".to_string()]),
        );

        assert!(caps.0.supports_text_generation);
        assert!(caps.0.supports_vision);
        assert!(caps.0.supports_function_calling);
        assert!(!caps.0.supports_embedding);
        assert!(!caps.0.supports_image_generation);
    }

    #[test]
    fn test_classify_gemini_embedding_models() {
        let caps = ModelRegistry::classify_model_capabilities(
            "text-embedding-004",
            &Provider::Gemini,
            Some(&["embedContent".to_string()]),
        );

        assert!(!caps.0.supports_text_generation);
        assert!(caps.0.supports_embedding);
        assert!(!caps.0.supports_streaming);
    }

    #[test]
    fn test_classify_openai_text_models() {
        let caps = ModelRegistry::classify_model_capabilities("gpt-4o", &Provider::OpenAI, None);

        assert!(caps.0.supports_text_generation);
        assert!(caps.0.supports_vision);
        assert!(caps.0.supports_function_calling);
    }

    #[test]
    fn test_classify_openai_embedding_models() {
        let caps = ModelRegistry::classify_model_capabilities(
            "text-embedding-ada-002",
            &Provider::OpenAI,
            None,
        );

        assert!(!caps.0.supports_text_generation);
        assert!(caps.0.supports_embedding);
    }

    #[test]
    fn test_classify_openai_image_models() {
        let caps = ModelRegistry::classify_model_capabilities("dall-e-3", &Provider::OpenAI, None);

        assert!(!caps.0.supports_text_generation);
        assert!(caps.0.supports_image_generation);
    }

    #[test]
    fn test_classify_anthropic_models() {
        let caps = ModelRegistry::classify_model_capabilities(
            "claude-opus-4-5-20251101",
            &Provider::Anthropic,
            None,
        );

        assert!(caps.0.supports_text_generation);
        assert!(caps.0.supports_vision);
        assert!(caps.0.supports_function_calling);
        assert!(!caps.0.supports_embedding);
    }

    #[test]
    fn test_classify_reasoning_models() {
        let caps = ModelRegistry::classify_model_capabilities("o3-mini", &Provider::OpenAI, None);

        assert!(caps.0.supports_text_generation);
        assert!(caps.0.supports_reasoning);
    }
}
