//! Database service for the application.

use crate::db::DbPool;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::time::SystemTime;

/// When a message was said, and — for a coworker's reply — when its run ended.
///
/// `sent_at` is the thread's order: a run is placed by when it began, so a
/// turn recovered after a restart still sits after the message it answers.
/// `finished_at` is the other end of the wait, for the peek stamp. Timing
/// JSON is the harness CUSTOM payload, when one arrived.
#[derive(Debug, Clone)]
pub struct SaveStamp {
    pub sent_at: SystemTime,
    pub finished_at: Option<SystemTime>,
    pub timing_json: Option<String>,
}

impl SaveStamp {
    pub fn at(sent_at: SystemTime) -> Self {
        Self {
            sent_at,
            finished_at: None,
            timing_json: None,
        }
    }
}

/// Database service for local session/message cache.
#[derive(Clone)]
pub struct DatabaseService {
    pool: DbPool,
}

impl DatabaseService {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> DbPool {
        self.pool.clone()
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

    /// A session row for a thread the server listed and this Mac has never had.
    ///
    /// It is written with the thread's latest activity as the server gave it, not the moment it
    /// was written, so it sorts and reads as that on the next launch. When the thread began is
    /// not known here, and `created_at` is left empty to say so. A row that exists is left
    /// alone.
    pub async fn keep_listed_session(&self, id: &str, title: &str, updated_at: &str) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO chat_sessions (id, title, created_at, updated_at) VALUES (?, ?, '', ?)",
        )
        .bind(id)
        .bind(title)
        .bind(updated_at)
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

    /// A message and, when it was more than words, the pieces it was made of.
    ///
    /// The two go in together: a half-written turn would come back as a bubble whose picture
    /// never arrived, which is the very thing keeping the pieces is here to stop.
    ///
    /// `run_id` is the run a coworker's reply came out of, and is what lets a thread be
    /// reconciled against the server without saying everything twice. The person's own messages
    /// came out of no run and carry none.
    /// Write a message down under the name its bubble already has.
    ///
    /// The id comes from the caller, and that is the whole point: a bubble whose row is filed
    /// under some other name cannot be deleted, edited or found again — the statement runs, no
    /// row matches, and the next reload paints the message straight back. The caller has an id
    /// from the moment the bubble exists, so there is nothing to mint here and nothing to hand
    /// back. Writing the same message twice is the same row twice, not two rows: a run that is
    /// settled by two paths at once (the live stream and the watcher) must not say everything
    /// twice.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_message(
        &self,
        id: &str,
        session_id: &str,
        role: &str,
        content: &str,
        model: Option<String>,
        provider: Option<String>,
        reply: Option<ReplyRef>,
        parts: &[MessagePart],
        run_id: Option<&str>,
        hidden: bool,
        // When the message was said, not when it reached the database: a reply recovered from
        // the server happened before the turns either side of it, and the moment it was
        // recovered would put it at the bottom of the thread on the next load. Finished-at
        // and the harness timing JSON ride along; they never move `sent_at`.
        stamp: SaveStamp,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE chat_sessions SET updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            // A message hidden before it was ever written down is written down hidden, and a
            // write that comes later never clears a mark that is already there. The person
            // can hide a reply while it is still being typed out, and the row for it does not
            // exist until the turn settles.
            "INSERT INTO chat_messages (id, session_id, role, content, model, provider, reply_to_id, reply_preview, reply_is_me, run_id, deleted_at, created_at, finished_at, run_timing)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               content = excluded.content,
               model = excluded.model,
               provider = excluded.provider,
               run_id = excluded.run_id,
               deleted_at = coalesce(chat_messages.deleted_at, excluded.deleted_at),
               finished_at = coalesce(excluded.finished_at, chat_messages.finished_at),
               run_timing = coalesce(excluded.run_timing, chat_messages.run_timing)",
        )
        .bind(id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(model)
        .bind(provider)
        .bind(reply.as_ref().map(|r| r.message_id.clone()))
        .bind(reply.as_ref().map(|r| r.preview.clone()))
        .bind(reply.as_ref().map(|r| i64::from(r.is_me)))
        .bind(run_id)
        .bind(hidden.then(Self::hidden_now))
        // Only on the way in: a second write of the same message is the same message, said
        // when it was said.
        .bind(Self::stamp(stamp.sent_at).unwrap_or_else(Self::hidden_now))
        .bind(stamp.finished_at.and_then(Self::stamp))
        .bind(stamp.timing_json.as_deref())
        .execute(&mut *tx)
        .await?;

        // A second write of the same message brings the whole of its pieces, so the first
        // write's are cleared rather than left to pile up beside them.
        sqlx::query("DELETE FROM chat_message_parts WHERE message_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        for (ord, part) in parts.iter().enumerate() {
            sqlx::query(
                "INSERT INTO chat_message_parts (message_id, ord, kind, text, call_id, image, width, height) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(ord as i64)
            .bind(part.kind())
            .bind(part.text())
            .bind(part.call_id())
            .bind(part.image())
            .bind(part.width())
            .bind(part.height())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The stamp a hidden row carries, in the same shape as `created_at`.
    fn hidden_now() -> String {
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }

    /// A time as a row carries it: to the millisecond, so a turn read back off the server and
    /// the message it answered do not land in the same second with nothing to order them by.
    /// It reads the same as the old whole-second stamps and sorts beside them.
    ///
    /// `None` for a time chrono cannot hold: both clocks can come from the server, and chrono's
    /// own `From<SystemTime>` panics on them.
    fn stamp(at: SystemTime) -> Option<String> {
        let since = at.duration_since(SystemTime::UNIX_EPOCH).ok()?;
        let ms = i64::try_from(since.as_millis()).ok()?;
        let at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)?;
        Some(at.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
    }

    /// Rewrite a message's words. A queued send that is edited before it drains has to
    /// change the row as well as the bubble, or a reload paints the old sentence.
    pub async fn update_message_content(&self, id: &str, content: &str) -> Result<()> {
        sqlx::query("UPDATE chat_messages SET content = ? WHERE id = ?")
            .bind(content)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Hide a message rather than take it away.
    ///
    /// The row is what names the run it came out of, and a thread that can name a run does not
    /// fetch it back from the server. Take the row away and the thread forgets, and what the
    /// person deleted returns on their next visit. So the row stays, stamped, and nothing
    /// paints it.
    pub async fn hide_message(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE chat_messages SET deleted_at = strftime('%Y-%m-%d %H:%M:%S', 'now') WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A thread's messages, each carrying the pieces saved under it. A message written before
    /// pieces were kept has none, and reads back as the words in `content`.
    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        let mut rows = sqlx::query_as::<_, ChatMessage>(
            "SELECT id, session_id, role, content, created_at, model, provider, reply_to_id, reply_preview, reply_is_me, run_id, deleted_at, finished_at, run_timing FROM chat_messages WHERE session_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        let part_rows = sqlx::query_as::<_, PartRow>(
            "SELECT p.message_id, p.kind, p.text, p.call_id, p.image, p.width, p.height FROM chat_message_parts p JOIN chat_messages m ON m.id = p.message_id WHERE m.session_id = ? ORDER BY p.message_id, p.ord",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        let mut by_message: HashMap<String, Vec<MessagePart>> = HashMap::new();
        for row in part_rows {
            by_message
                .entry(row.message_id.clone())
                .or_default()
                .push(row.into_part());
        }
        for row in &mut rows {
            row.parts = by_message.remove(&row.id).unwrap_or_default();
        }
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
    /// The run this reply came out of, when it came out of one. It is how a thread being
    /// reconciled against the server tells a run it has already written down from one it has
    /// only just heard about.
    pub run_id: Option<String>,
    /// When the person hid this message, for a message they hid. The row stays so the thread
    /// can still name its run; nothing paints it.
    pub deleted_at: Option<String>,
    /// When the run ended, for a coworker's reply. Null on the person's own
    /// messages, on rows written before this column, and on a bubble still
    /// being filled in.
    pub finished_at: Option<String>,
    /// Harness CUSTOM `run-timing` JSON (`v:1`), when the turn sent one.
    pub run_timing: Option<String>,
    /// The pieces of the message, in the order they were seen. They live in a table of their own
    /// so the picture bytes stay off this row; `get_messages` is what fills this in.
    #[sqlx(skip)]
    pub parts: Vec<MessagePart>,
}

/// One piece of a saved message: the words of a bubble, or a picture of the box's screen with
/// the caption the tool wrote under it.
///
/// A permission card is deliberately not one of these. It belongs to a run that is long over by
/// the time the thread is opened again, and reviving it would ask the person to allow something
/// that has already happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessagePart {
    Text(String),
    Screenshot {
        call_id: String,
        caption: String,
        /// The PNG as it arrived, not base64: a third smaller, and out of the text column.
        image: Vec<u8>,
        width: u32,
        height: u32,
    },
    /// A generative widget the coworker answered with — a form of choice chips, a bar chart —
    /// kept as the JSON it was parsed from, so the thread mounts the same widget when it is read
    /// back. Without it a turn that said nothing but the form came back as its stand-in line
    /// alone, "[used form]", on the very next visit.
    Ui {
        /// `{"component": "form" | "bar-chart", ...}` as `UiSpec::to_value` writes it.
        spec: String,
    },
}

impl MessagePart {
    fn kind(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Screenshot { .. } => "screenshot",
            Self::Ui { .. } => "ui",
        }
    }

    /// One text column serves all three: a text part's words, a screenshot's caption, a
    /// widget's JSON.
    fn text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Screenshot { caption, .. } => caption.clone(),
            Self::Ui { spec } => spec.clone(),
        }
    }

    fn call_id(&self) -> Option<String> {
        match self {
            Self::Text(_) | Self::Ui { .. } => None,
            Self::Screenshot { call_id, .. } => Some(call_id.clone()),
        }
    }

    fn image(&self) -> Option<Vec<u8>> {
        match self {
            Self::Text(_) | Self::Ui { .. } => None,
            Self::Screenshot { image, .. } => Some(image.clone()),
        }
    }

    fn width(&self) -> Option<i64> {
        match self {
            Self::Text(_) | Self::Ui { .. } => None,
            Self::Screenshot { width, .. } => Some(i64::from(*width)),
        }
    }

    fn height(&self) -> Option<i64> {
        match self {
            Self::Text(_) | Self::Ui { .. } => None,
            Self::Screenshot { height, .. } => Some(i64::from(*height)),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct PartRow {
    message_id: String,
    kind: String,
    text: Option<String>,
    call_id: Option<String>,
    image: Option<Vec<u8>>,
    width: Option<i64>,
    height: Option<i64>,
}

impl PartRow {
    /// A row this build cannot paint as a picture — an unknown kind, or a screenshot whose bytes
    /// are gone — still has words on it, so it comes back as text rather than as nothing.
    fn into_part(self) -> MessagePart {
        let text = self.text.unwrap_or_default();
        if self.kind == "screenshot"
            && let Some(image) = self.image
            && let (Some(width), Some(height)) = (self.width, self.height)
        {
            return MessagePart::Screenshot {
                call_id: self.call_id.unwrap_or_default(),
                caption: text,
                image,
                width: width.max(0) as u32,
                height: height.max(0) as u32,
            };
        }
        if self.kind == "ui" {
            return MessagePart::Ui { spec: text };
        }
        MessagePart::Text(text)
    }
}

/// The message a saved message answers. Rows written before replies were kept have none of it,
/// which is why every column is nullable and this is an `Option` at the call.
#[derive(Debug, Clone)]
pub struct ReplyRef {
    pub message_id: String,
    pub preview: String,
    pub is_me: bool,
}
