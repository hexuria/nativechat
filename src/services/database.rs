//! Database service for the application.

use crate::db::DbPool;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Credential stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Default)]
pub struct Credential {
    pub id: i64,
    pub name: String,
    pub provider: String,
    pub api_key: String,
    pub created_at: String,
}

/// Profile stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub text_credential_id: Option<i64>,
    pub embedding_credential_id: Option<i64>,
    pub image_credential_id: Option<i64>,
    pub text_model_id: Option<String>,
    pub embedding_model_id: Option<String>,
    pub image_model_id: Option<String>,
    pub tts_model_id: Option<String>,
    pub tts_voice: Option<String>,
    pub created_at: String,
}

/// Database service for credential and profile operations.
#[derive(Clone)]
pub struct DatabaseService {
    pool: DbPool,
}

impl DatabaseService {
    /// Creates a new DatabaseService with the given connection pool.
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Creates a new credential and returns its ID.
    pub async fn create_credential(
        &self,
        name: &str,
        provider: &str,
        api_key: &str,
    ) -> Result<i64> {
        let result = sqlx::query_scalar::<_, i64>(
            "INSERT INTO credentials (name, provider, api_key) VALUES (?, ?, ?) RETURNING id",
        )
        .bind(name)
        .bind(provider)
        .bind(api_key)
        .fetch_one(&self.pool)
        .await?;
        Ok(result)
    }

    /// Returns all credentials.
    pub async fn get_credentials(&self) -> Result<Vec<Credential>> {
        let rows = sqlx::query_as::<_, Credential>(
            "SELECT id, name, provider, api_key, created_at FROM credentials ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Deletes a credential by ID.
    pub async fn delete_credential(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM credentials WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Updates an existing credential.
    pub async fn update_credential(
        &self,
        id: i64,
        name: &str,
        provider: &str,
        api_key: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE credentials SET name = ?, provider = ?, api_key = ? WHERE id = ?")
            .bind(name)
            .bind(provider)
            .bind(api_key)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Creates a new profile.
    pub async fn create_profile(&self, name: &str) -> Result<i64> {
        let result =
            sqlx::query_scalar::<_, i64>("INSERT INTO profiles (name) VALUES (?) RETURNING id")
                .bind(name)
                .fetch_one(&self.pool)
                .await?;
        Ok(result)
    }

    /// Returns all profiles.
    pub async fn get_profiles(&self) -> Result<Vec<Profile>> {
        let rows = sqlx::query_as::<_, Profile>(
            "SELECT id, name, text_credential_id, embedding_credential_id, image_credential_id, 
             text_model_id, embedding_model_id, image_model_id, tts_model_id, tts_voice, created_at 
             FROM profiles ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Updates a profile.
    pub async fn update_profile(&self, profile: &Profile) -> Result<()> {
        sqlx::query(
            "UPDATE profiles SET name = ?, text_credential_id = ?, embedding_credential_id = ?,
             image_credential_id = ?, text_model_id = ?, embedding_model_id = ?, image_model_id = ?, tts_model_id = ?, tts_voice = ?
             WHERE id = ?",
        )
        .bind(&profile.name)
        .bind(profile.text_credential_id)
        .bind(profile.embedding_credential_id)
        .bind(profile.image_credential_id)
        .bind(&profile.text_model_id)
        .bind(&profile.embedding_model_id)
        .bind(&profile.image_model_id)
        .bind(&profile.tts_model_id)
        .bind(&profile.tts_voice)
        .bind(profile.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Deletes a profile by ID.
    pub async fn delete_profile(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM profiles WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Gets a profile by ID.
    pub async fn get_profile(&self, id: i64) -> Result<Profile> {
        let profile = sqlx::query_as::<_, Profile>(
            "SELECT id, name, text_credential_id, embedding_credential_id, image_credential_id,
             text_model_id, embedding_model_id, image_model_id, tts_model_id, tts_voice, created_at
             FROM profiles WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(profile)
    }

    /// Gets a credential by ID.
    pub async fn get_credential(&self, id: i64) -> Result<Credential> {
        let credential = sqlx::query_as::<_, Credential>(
            "SELECT id, name, provider, api_key, created_at FROM credentials WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(credential)
    }

    /// Gets a setting value by key.
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let result = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(result)
    }

    /// Sets a setting value by key (upsert).
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Deletes a setting by key.
    pub async fn delete_setting(&self, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // Chat Session Management

    /// Creates a new chat session.
    pub async fn create_session(&self, title: &str) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query("INSERT INTO chat_sessions (id, title) VALUES (?, ?)")
            .bind(&id)
            .bind(title)
            .execute(&self.pool)
            .await?;
        Ok(id)
    }

    /// Returns all chat sessions for the user (currently single user).
    pub async fn get_sessions(&self) -> Result<Vec<ChatSession>> {
        let rows = sqlx::query_as::<_, ChatSession>(
            "SELECT id, title, created_at, updated_at FROM chat_sessions ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Gets a specific chat session.
    pub async fn get_session(&self, id: &str) -> Result<ChatSession> {
        let session = sqlx::query_as::<_, ChatSession>(
            "SELECT id, title, created_at, updated_at FROM chat_sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(session)
    }

    /// Updates session title.
    pub async fn update_session_title(&self, id: &str, title: &str) -> Result<()> {
        sqlx::query(
            "UPDATE chat_sessions SET title = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(title)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Deletes a session and all its messages (via cascade).
    pub async fn delete_session(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM chat_sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // Chat Message Management

    /// Saves a message to a session.
    pub async fn save_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        model: Option<String>,
        provider: Option<String>,
    ) -> Result<String> {
        // First update the session's updated_at timestamp
        sqlx::query("UPDATE chat_sessions SET updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO chat_messages (id, session_id, role, content, model, provider) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(model)
        .bind(provider)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn delete_message(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM chat_messages WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Returns all messages for a session.
    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        let rows = sqlx::query_as::<_, ChatMessage>(
            "SELECT id, session_id, role, content, created_at, model, provider FROM chat_messages WHERE session_id = ? ORDER BY created_at ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

/// Chat Session stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Chat Message stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub model: Option<String>,
    pub provider: Option<String>,
}
