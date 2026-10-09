//! What went wrong, per Bot: the entries behind the bell in the title bar, the toast that says one
//! just happened, and the Copy that hands one over whole (8 Oct 2026).
//!
//! An entry says where in the app it was caught (`place`, in the person's words: "Turn", "Plugins
//! window") and where in the source (`code`, the file and line `#[track_caller]` gives the place
//! that recorded it), so a failure a toast showed for eight seconds can still be traced. Kept on
//! this Mac in sqlite until the person clears it; nothing secret goes in.

use crate::db::DbPool;
use sqlx::Row;

/// One failure, as the bell's list and the toast show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub id: String,
    pub at_ms: i64,
    /// The Bot it belongs to; `None` for one that belongs to none (signing in, the server).
    pub bot: Option<String>,
    /// Where in the app, in the person's words: "Turn", "Plugins window", "Model picker".
    pub place: String,
    /// The source file and line it was caught at, `src/state.rs:17335`.
    pub code: String,
    /// The sentence shown.
    pub said: String,
    /// The server's own words, when it sent more than the sentence.
    pub raw: Option<String>,
    pub run_id: Option<String>,
    /// Seen in the list: the bell counts the rest.
    pub read: bool,
    /// Set when this is a fault: a read that failed at one place, whose badge it drives
    /// (`crate::faults`). `None` for a notice that is not one.
    pub fault: Option<FaultFacts>,
}

/// What a fault keeps beside the notice: where it shows, the request it answered, and how often
/// it has happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaultFacts {
    pub place: crate::faults::Place,
    /// `GET /coworkers/cw_1/usage?window=month`, where known.
    pub endpoint: Option<String>,
    pub status: Option<u16>,
    /// How many times the same failure has come back since it was first seen (`at_ms`).
    pub count: u32,
    /// When it last came back.
    pub last_ms: i64,
    /// A read at the place worked after it: it went away on its own.
    pub resolved: bool,
}

impl Notice {
    /// A new entry for now, caught at `code`.
    pub fn new(bot: Option<String>, place: &str, said: &str, code: &std::panic::Location) -> Self {
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default();
        Self {
            id: format!("ntf_{}", uuid::Uuid::now_v7().simple()),
            at_ms,
            bot,
            place: place.to_string(),
            code: format!("{}:{}", code.file(), code.line()),
            said: said.to_string(),
            raw: None,
            run_id: None,
            read: false,
            fault: None,
        }
    }

    /// When it was last seen: a fault's latest return, else when it was kept.
    pub fn last_ms(&self) -> i64 {
        self.fault
            .as_ref()
            .map_or(self.at_ms, |fault| fault.last_ms)
    }

    /// The block Copy puts on the clipboard: everything there is to trace it by, as plain text.
    pub fn copy_text(&self, bot_name: Option<&str>) -> String {
        let when = chrono::DateTime::from_timestamp_millis(self.at_ms)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        let mut out = format!("When:   {when}\n");
        match (&self.bot, bot_name) {
            (Some(id), Some(name)) => out.push_str(&format!("Bot:    {name} ({id})\n")),
            (Some(id), None) => out.push_str(&format!("Bot:    {id}\n")),
            (None, _) => out.push_str("Bot:    none\n"),
        }
        out.push_str(&format!("Where:  {}\n", self.place));
        out.push_str(&format!("Code:   {}\n", self.code));
        out.push_str(&format!("Said:   {}\n", self.said));
        // A one-line text stays on its label's line; a longer one (a cause chain) follows it
        // as written, so its lines and indents survive the paste.
        match &self.raw {
            Some(raw) if raw.contains('\n') => out.push_str(&format!("Raw:\n{raw}\n")),
            Some(raw) => out.push_str(&format!("Raw:    {raw}\n")),
            None => {}
        }
        if let Some(run) = &self.run_id {
            out.push_str(&format!("Run:    {run}\n"));
        }
        if let Some(fault) = &self.fault {
            if let Some(endpoint) = &fault.endpoint {
                out.push_str(&format!("Request: {endpoint}\n"));
            }
            if let Some(status) = fault.status {
                out.push_str(&format!("Status: {status}\n"));
            }
            out.push_str(&format!("Times:  {}\n", fault.count));
        }
        out
    }

    /// The JSON a driver reads (`notices.list`, `notices.get`).
    pub fn to_json(&self, bot_name: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "atMs": self.at_ms,
            "bot": self.bot,
            "botName": bot_name,
            "place": self.place,
            "code": self.code,
            "said": self.said,
            "raw": self.raw,
            "runId": self.run_id,
            "read": self.read,
            "fault": self.fault.as_ref().map(|fault| serde_json::json!({
                "place": fault.place.word(),
                "endpoint": fault.endpoint,
                "status": fault.status,
                "count": fault.count,
                "lastMs": fault.last_ms,
                "resolved": fault.resolved,
            })),
        })
    }
}

/// The most entries kept: `load` reads the same cap, so an older one is gone rather than
/// never shown.
const KEPT_LIMIT: usize = 2000;

/// Keep one entry.
pub async fn save(pool: &DbPool, notice: &Notice) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO notifications (id, at_ms, bot_id, place, code, said, raw, run_id, \
         read, fault_place, endpoint, status, count, last_ms, resolved) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&notice.id)
    .bind(notice.at_ms)
    .bind(&notice.bot)
    .bind(&notice.place)
    .bind(&notice.code)
    .bind(&notice.said)
    .bind(&notice.raw)
    .bind(&notice.run_id)
    .bind(notice.read)
    .bind(notice.fault.as_ref().map(|fault| fault.place.word()))
    .bind(
        notice
            .fault
            .as_ref()
            .and_then(|fault| fault.endpoint.clone()),
    )
    .bind(
        notice
            .fault
            .as_ref()
            .and_then(|fault| fault.status.map(i64::from)),
    )
    .bind(
        notice
            .fault
            .as_ref()
            .map_or(1, |fault| i64::from(fault.count)),
    )
    .bind(notice.fault.as_ref().map(|fault| fault.last_ms))
    .bind(notice.fault.as_ref().is_some_and(|fault| fault.resolved))
    .execute(pool)
    .await?;
    // A Mac that never clears must not grow the table forever: only the newest are kept,
    // under the same cap `load` reads.
    sqlx::query(
        "DELETE FROM notifications WHERE id NOT IN \
         (SELECT id FROM notifications ORDER BY at_ms DESC LIMIT ?)",
    )
    .bind(KEPT_LIMIT as i64)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every entry kept, newest first.
pub async fn load(pool: &DbPool) -> Result<Vec<Notice>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, at_ms, bot_id, place, code, said, raw, run_id, read, fault_place, endpoint, \
         status, count, last_ms, resolved FROM notifications \
         ORDER BY at_ms DESC LIMIT ?",
    )
    .bind(KEPT_LIMIT as i64)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(Notice {
                id: row.try_get("id")?,
                at_ms: row.try_get("at_ms")?,
                bot: row.try_get("bot_id")?,
                place: row.try_get("place")?,
                code: row.try_get("code")?,
                said: row.try_get("said")?,
                raw: row.try_get("raw")?,
                run_id: row.try_get("run_id")?,
                read: row.try_get("read")?,
                fault: fault_of(row)?,
            })
        })
        .collect()
}

/// A row's fault, when its place is one this app knows. A place word from a newer app reads as
/// no fault rather than failing the whole list.
fn fault_of(row: &sqlx::sqlite::SqliteRow) -> Result<Option<FaultFacts>, sqlx::Error> {
    let place: Option<String> = row.try_get("fault_place")?;
    let Some(place) = place.as_deref().and_then(crate::faults::Place::from_word) else {
        return Ok(None);
    };
    let status: Option<i64> = row.try_get("status")?;
    let count: i64 = row.try_get("count")?;
    let last_ms: Option<i64> = row.try_get("last_ms")?;
    let at_ms: i64 = row.try_get("at_ms")?;
    Ok(Some(FaultFacts {
        place,
        endpoint: row.try_get("endpoint")?,
        status: status.and_then(|status| u16::try_from(status).ok()),
        count: u32::try_from(count).unwrap_or(1),
        last_ms: last_ms.unwrap_or(at_ms),
        resolved: row.try_get("resolved")?,
    }))
}

/// Mark every entry of one Bot (or every entry, with `None`) as seen.
pub async fn mark_read(pool: &DbPool, bot: Option<&str>) -> Result<(), sqlx::Error> {
    match bot {
        Some(bot) => {
            sqlx::query("UPDATE notifications SET read = 1 WHERE bot_id = ?")
                .bind(bot)
                .execute(pool)
                .await?
        }
        None => {
            sqlx::query("UPDATE notifications SET read = 1")
                .execute(pool)
                .await?
        }
    };
    Ok(())
}

/// Mark these entries read (or unread, with `false`).
pub async fn set_read(pool: &DbPool, ids: &[String], read: bool) -> Result<(), sqlx::Error> {
    for id in ids {
        sqlx::query("UPDATE notifications SET read = ? WHERE id = ?")
            .bind(read)
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Forget these entries.
pub async fn delete(pool: &DbPool, ids: &[String]) -> Result<(), sqlx::Error> {
    for id in ids {
        sqlx::query("DELETE FROM notifications WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Forget one Bot's entries, or every entry with `None`.
pub async fn clear(pool: &DbPool, bot: Option<&str>) -> Result<(), sqlx::Error> {
    match bot {
        Some(bot) => {
            sqlx::query("DELETE FROM notifications WHERE bot_id = ?")
                .bind(bot)
                .execute(pool)
                .await?
        }
        None => {
            sqlx::query("DELETE FROM notifications")
                .execute(pool)
                .await?
        }
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    async fn pool() -> DbPool {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("an in-memory database")
            .create_if_missing(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("a pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("the schema");
        pool
    }

    /// A notice keeps where it was caught, comes back after a restart newest first, is seen per
    /// Bot, and goes with Clear, one Bot's or all; Copy's block carries everything to trace it by.
    #[tokio::test]
    async fn a_notice_is_kept_read_and_cleared_per_bot_and_copies_whole() {
        let pool = pool().await;
        let here = std::panic::Location::caller();
        let mut first = Notice::new(
            Some("cw_1".into()),
            "Turn",
            "upstream anthropic returned 400",
            here,
        );
        first.at_ms = 1;
        first.raw = Some("code plan_unavailable".into());
        first.run_id = Some("run_1".into());
        let mut second = Notice::new(
            Some("cw_2".into()),
            "Plugins window",
            "could not share",
            here,
        );
        second.at_ms = 2;
        save(&pool, &first).await.unwrap();
        save(&pool, &second).await.unwrap();

        let kept = load(&pool).await.unwrap();
        assert_eq!(
            kept.iter().map(|n| n.at_ms).collect::<Vec<_>>(),
            [2, 1],
            "newest first"
        );
        assert_eq!(kept[1], first, "every field comes back");

        mark_read(&pool, Some("cw_1")).await.unwrap();
        let kept = load(&pool).await.unwrap();
        assert!(
            kept.iter()
                .find(|n| n.bot.as_deref() == Some("cw_1"))
                .unwrap()
                .read
        );
        assert!(
            !kept
                .iter()
                .find(|n| n.bot.as_deref() == Some("cw_2"))
                .unwrap()
                .read
        );

        let text = first.copy_text(Some("Genie"));
        for line in [
            "Bot:    Genie (cw_1)",
            "Where:  Turn",
            "Said:   upstream anthropic returned 400",
            "Raw:    code plan_unavailable",
            "Run:    run_1",
        ] {
            assert!(text.contains(line), "{line} in {text}");
        }
        assert!(text.contains("Code:   src/notifications.rs:"), "{text}");

        clear(&pool, Some("cw_1")).await.unwrap();
        assert_eq!(load(&pool).await.unwrap().len(), 1, "only cw_1's went");
        clear(&pool, None).await.unwrap();
        assert!(load(&pool).await.unwrap().is_empty());
    }
}
