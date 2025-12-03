//! Gemini LLM provider.

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use super::provider::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, ChatStream, LlmProvider, Usage,
};
use crate::error::{AppError, Result};

const GEMINI_API_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

pub struct GeminiProvider {
    client: Client,
    api_key: String,
}

impl GeminiProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    fn convert_messages(&self, messages: &[ChatMessage]) -> Vec<GeminiContent> {
        messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|msg| {
                let role = if msg.role == "assistant" {
                    "model"
                } else {
                    "user"
                };
                let mut parts = vec![GeminiPart::Text {
                    text: msg.content.clone(),
                }];
                if let Some(images) = &msg.images {
                    for img in images {
                        parts.push(GeminiPart::InlineData {
                            inline_data: GeminiInlineData {
                                mime_type: img.mime_type.clone(),
                                data: img.data.clone(),
                            },
                        });
                    }
                }
                GeminiContent {
                    role: role.to_string(),
                    parts,
                }
            })
            .collect()
    }

    fn get_system_instruction(
        &self,
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
    ) -> Option<GeminiContent> {
        let system = system_prompt.map(|s| s.to_string()).or_else(|| {
            messages
                .iter()
                .find(|m| m.role == "system")
                .map(|m| m.content.clone())
        });
        system.map(|text| GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart::Text { text }],
        })
    }

    fn parse_error(&self, error_text: &str) -> String {
        #[derive(Deserialize)]
        struct GeminiErrorResponse {
            error: GeminiError,
        }

        #[derive(Deserialize)]
        struct GeminiError {
            message: String,
        }

        if let Ok(json_error) = serde_json::from_str::<GeminiErrorResponse>(error_text) {
            json_error.error.message
        } else {
            error_text.to_string()
        }
    }
}

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text { text: String },
    InlineData { inline_data: GeminiInlineData },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiInlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }
    fn default_model(&self) -> &'static str {
        "gemini-2.0-flash"
    }
    fn supports_vision(&self) -> bool {
        true
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let contents = self.convert_messages(&request.messages);
        let system_instruction =
            self.get_system_instruction(&request.messages, request.system_prompt.as_deref());

        let req = GeminiRequest {
            contents,
            system_instruction,
            generation_config: Some(GeminiGenerationConfig {
                temperature: Some(request.temperature),
                max_output_tokens: request.max_tokens,
                candidate_count: Some(1),
            }),
        };

        let url = format!(
            "{}/{}:generateContent?key={}",
            GEMINI_API_URL, request.model, self.api_key
        );
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let message = self.parse_error(&error_text);
            return Err(AppError::LlmProvider(format!(
                "Gemini API error: {}",
                message
            )));
        }

        let data: GeminiResponse = response.json().await?;
        let candidate = data
            .candidates
            .and_then(|c| c.into_iter().next())
            .ok_or_else(|| AppError::LlmProvider("No response from Gemini".into()))?;

        let content = candidate
            .content
            .map(|c| {
                c.parts
                    .into_iter()
                    .filter_map(|p| match p {
                        GeminiPart::Text { text } => Some(text),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        Ok(ChatResponse {
            id: Uuid::new_v4().to_string(),
            content,
            finish_reason: candidate.finish_reason,
            usage: data.usage_metadata.map(|u| Usage {
                prompt_tokens: u.prompt_token_count.unwrap_or(0),
                completion_tokens: u.candidates_token_count.unwrap_or(0),
                total_tokens: u.total_token_count.unwrap_or(0),
            }),
        })
    }

    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream> {
        let contents = self.convert_messages(&request.messages);
        let system_instruction =
            self.get_system_instruction(&request.messages, request.system_prompt.as_deref());

        let req = GeminiRequest {
            contents,
            system_instruction,
            generation_config: Some(GeminiGenerationConfig {
                temperature: Some(request.temperature),
                max_output_tokens: request.max_tokens,
                candidate_count: Some(1),
            }),
        };

        let url = format!(
            "{}/{}:streamGenerateContent?key={}&alt=sse",
            GEMINI_API_URL, request.model, self.api_key
        );
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let message = self.parse_error(&error_text);
            return Err(AppError::LlmProvider(format!(
                "Gemini API error: {}",
                message
            )));
        }

        let (tx, rx) = mpsc::channel(100);
        let response_id = Uuid::new_v4().to_string();

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
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
                                if let Ok(resp) = serde_json::from_str::<GeminiResponse>(data) {
                                    if let Some(candidates) = resp.candidates {
                                        for candidate in candidates {
                                            if let Some(content) = candidate.content {
                                                for part in content.parts {
                                                    if let GeminiPart::Text { text } = part {
                                                        let chunk = ChatChunk {
                                                            id: response_id.clone(),
                                                            delta: text,
                                                            finish_reason: candidate
                                                                .finish_reason
                                                                .clone(),
                                                        };
                                                        if tx.send(Ok(chunk)).await.is_err() {
                                                            return;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(AppError::LlmProvider(format!("Stream error: {}", e))))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}
