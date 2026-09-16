//! Database service for the application.

use crate::db::DbPool;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Database service for local session/message cache.
#[derive(Clone)]
pub struct DatabaseService {
    pool: DbPool,
}

impl DatabaseService {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create_session(&self, title: &str) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query("INSERT INTO chat_sessions (id, title) VALUES (?, ?)")
            .bind(&id)
            .bind(title)
            .execute(&self.pool)
            .await?;
        Ok(id)
    }

    /// A session row under a caller-chosen id. Coworker threads use the
    /// coworker id, so their messages persist under the id the conversation
    /// already carries. A row that exists is left alone.
    pub async fn ensure_session(&self, id: &str, title: &str) -> Result<()> {
        sqlx::query("INSERT OR IGNORE INTO chat_sessions (id, title) VALUES (?, ?)")
            .bind(id)
            .bind(title)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_sessions(&self) -> Result<Vec<ChatSession>> {
        let rows = sqlx::query_as::<_, ChatSession>(
            "SELECT id, title, created_at, updated_at FROM chat_sessions ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

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

    pub async fn delete_session(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM chat_sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn save_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        model: Option<String>,
        provider: Option<String>,
        reply: Option<ReplyRef>,
    ) -> Result<String> {
        sqlx::query("UPDATE chat_sessions SET updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO chat_messages (id, session_id, role, content, model, provider, reply_to_id, reply_preview, reply_is_me) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(model)
        .bind(provider)
        .bind(reply.as_ref().map(|r| r.message_id.clone()))
        .bind(reply.as_ref().map(|r| r.preview.clone()))
        .bind(reply.as_ref().map(|r| i64::from(r.is_me)))
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

    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        let rows = sqlx::query_as::<_, ChatMessage>(
            "SELECT id, session_id, role, content, created_at, model, provider, reply_to_id, reply_preview, reply_is_me FROM chat_messages WHERE session_id = ? ORDER BY created_at ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub reply_to_id: Option<String>,
    pub reply_preview: Option<String>,
    pub reply_is_me: Option<i64>,
}

/// The message a saved message answers. Rows written before replies were kept have none of it,
/// which is why every column is nullable and this is an `Option` at the call.
#[derive(Debug, Clone)]
pub struct ReplyRef {
    pub message_id: String,
    pub preview: String,
    pub is_me: bool,
}
