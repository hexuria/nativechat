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
    pub created_at: String,
}

/// Model stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModelEntity {
    pub id: String,
    pub provider: String,
    pub name: String,
    pub description: Option<String>,
    pub model_type: String,
    pub input_token_limit: Option<i64>,
    pub output_token_limit: Option<i64>,
    pub capabilities: String,
    pub is_thinking: bool,
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
             text_model_id, embedding_model_id, image_model_id, created_at 
             FROM profiles ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Saves a model to the database.
    pub async fn save_model(&self, model: &ModelEntity) -> Result<()> {
        sqlx::query(
            "INSERT INTO models (id, provider, name, description, model_type, input_token_limit, output_token_limit, capabilities, is_thinking, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                name = excluded.name,
                description = excluded.description,
                model_type = excluded.model_type,
                input_token_limit = excluded.input_token_limit,
                output_token_limit = excluded.output_token_limit,
                capabilities = excluded.capabilities,
                is_thinking = excluded.is_thinking"
        )
        .bind(&model.id)
        .bind(&model.provider)
        .bind(&model.name)
        .bind(&model.description)
        .bind(&model.model_type)
        .bind(model.input_token_limit)
        .bind(model.output_token_limit)
        .bind(&model.capabilities)
        .bind(model.is_thinking)
        .bind(&model.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Returns all models.
    pub async fn get_models_list(&self) -> Result<Vec<ModelEntity>> {
        let rows = sqlx::query_as::<_, ModelEntity>("SELECT * FROM models ORDER BY provider, name")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    /// Updates a profile.
    pub async fn update_profile(&self, profile: &Profile) -> Result<()> {
        sqlx::query(
            "UPDATE profiles SET name = ?, text_credential_id = ?, embedding_credential_id = ?,
             image_credential_id = ?, text_model_id = ?, embedding_model_id = ?, image_model_id = ?
             WHERE id = ?",
        )
        .bind(&profile.name)
        .bind(profile.text_credential_id)
        .bind(profile.embedding_credential_id)
        .bind(profile.image_credential_id)
        .bind(&profile.text_model_id)
        .bind(&profile.embedding_model_id)
        .bind(&profile.image_model_id)
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
             text_model_id, embedding_model_id, image_model_id, created_at
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
}
