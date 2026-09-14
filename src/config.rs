//! Application configuration.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub data_dir: PathBuf,
    /// Optional Gemini key for local TTS / live voice. Chat inference goes through OpenGrok.
    pub gemini_api_key: Option<String>,
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

        let gemini_api_key = std::env::var("GEMINI_API_KEY").ok();
        let opengrok_base_url = std::env::var("OPENGROK_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:1447".to_string());

        Ok(Self {
            database_url,
            data_dir,
            gemini_api_key,
            opengrok_base_url,
        })
    }
}
