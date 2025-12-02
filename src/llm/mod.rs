//! LLM providers module.

mod gemini;
mod provider;

pub use gemini::GeminiProvider;
pub use provider::*;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::services::database::Credential;

/// Create an LLM provider from a database credential.
///
/// This function creates the appropriate provider based on the credential's provider field.
/// Supports: gemini, openai, anthropic
///
/// # Arguments
/// * `credential` - The database credential containing provider type and API key
/// * `model_id` - Optional model ID to use (provider default if None)
///
/// # Returns
/// A boxed LlmProvider configured with the credential's API key
pub fn create_provider_from_credential(
    credential: &Credential,
    _model_id: Option<&str>,
) -> Result<Box<dyn LlmProvider>> {
    match credential.provider.to_lowercase().as_str() {
        "gemini" | "google gemini" => Ok(Box::new(GeminiProvider::new(credential.api_key.clone()))),
        "openai" => {
            // TODO: Implement OpenAI provider when available
            Err(AppError::BadRequest(
                "OpenAI provider not yet implemented".into(),
            ))
        }
        "anthropic" => {
            // TODO: Implement Anthropic provider when available
            Err(AppError::BadRequest(
                "Anthropic provider not yet implemented".into(),
            ))
        }
        _ => Err(AppError::BadRequest(format!(
            "Unknown provider: {}",
            credential.provider
        ))),
    }
}

/// Create an LLM provider from application config (environment variables).
///
/// This is the fallback method when no profile/credential is selected.
pub fn create_provider(config: &Config) -> Result<Box<dyn LlmProvider>> {
    match config.default_provider.to_lowercase().as_str() {
        "gemini" => {
            let api_key = config
                .gemini_api_key
                .clone()
                .ok_or_else(|| AppError::Config("Gemini API key not configured".into()))?;
            Ok(Box::new(GeminiProvider::new(api_key)))
        }
        _ => Err(AppError::BadRequest(format!(
            "Unknown provider: {}",
            config.default_provider
        ))),
    }
}
