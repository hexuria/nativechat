//! Application configuration.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub data_dir: PathBuf,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub openai_base_url: String,
    pub default_provider: String,
    pub default_model: Option<String>,
    pub opengrok_base_url: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let project_dirs = ProjectDirs::from("ai", "nativechat", "NativeChat")
            .ok_or_else(|| AppError::Config("Could not determine app directories".into()))?;

        let data_dir = std::env::var("NATIVECHAT_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| project_dirs.data_dir().to_path_buf());

        std::fs::create_dir_all(&data_dir)?;

        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            format!("sqlite://{}?mode=rwc", data_dir.join("data.db").display())
        });

        let openai_api_key = std::env::var("OPENAI_API_KEY").ok();
        let anthropic_api_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let gemini_api_key = std::env::var("GEMINI_API_KEY").ok();

        let openai_base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

        // Default to gemini if GEMINI_API_KEY is set
        let default_provider = std::env::var("DEFAULT_PROVIDER").unwrap_or_else(|_| {
            if gemini_api_key.is_some() {
                "gemini".to_string()
            } else if anthropic_api_key.is_some() {
                "anthropic".to_string()
            } else {
                "openai".to_string()
            }
        });

        let default_model = std::env::var("DEFAULT_MODEL").ok();
        let opengrok_base_url = std::env::var("OPENGROK_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:1447".to_string());

        Ok(Self {
            database_url,
            data_dir,
            openai_api_key,
            anthropic_api_key,
            gemini_api_key,
            openai_base_url,
            default_provider,
            default_model,
            opengrok_base_url,
        })
    }
}
