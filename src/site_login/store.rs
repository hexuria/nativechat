//! sqlite metadata + secret backend. Never a password column.

use std::path::Path;
use std::sync::Arc;

use crate::db::DbPool;

use super::StoreError;
use super::secrets::{MemorySecrets, SecretStore, open_secrets};
use super::{default_label, timestamp_ms};

/// Row the Settings list paints. No secret.
#[derive(Debug, Clone, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct SiteLoginRecord {
    pub id: String,
    pub origin: String,
    pub username: String,
    pub label: String,
    /// What the row holds: `password`, `code` or `passkey`.
    pub kind: String,
    /// The person's own words about this login. A line starting `Security:` files the row
    /// under the Security tile.
    pub notes: String,
    /// When the server last saw the login used, in unix milliseconds. None until it has been.
    pub last_used_at_ms: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Every column the record reads, in its order.
const SELECT: &str = "SELECT id, origin, username, label, kind, notes, last_used_at_ms, \
                      created_at, updated_at FROM site_logins";

/// An empty kind is a password: that is what every row was before kinds existed.
fn kind_or_default(kind: &str) -> &str {
    let kind = kind.trim();
    if kind.is_empty() { "password" } else { kind }
}

#[derive(Clone)]
pub struct SiteLoginVault {
    pool: DbPool,
    secrets: Arc<dyn SecretStore>,
}

impl SiteLoginVault {
    pub fn open(pool: DbPool, data_dir: &Path) -> Self {
        Self {
            pool,
            secrets: open_secrets(data_dir),
        }
    }

    pub fn memory(pool: DbPool) -> Self {
        Self {
            pool,
            secrets: Arc::new(MemorySecrets::new()),
        }
    }

    pub async fn list(&self) -> Result<Vec<SiteLoginRecord>, StoreError> {
        let rows =
            sqlx::query_as::<_, SiteLoginRecord>(&format!("{SELECT} ORDER BY origin, username"))
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    pub async fn find(
        &self,
        origin: &str,
        username: Option<&str>,
    ) -> Result<Option<SiteLoginRecord>, StoreError> {
        if let Some(username) = username.filter(|name| !name.is_empty()) {
            let row = sqlx::query_as::<_, SiteLoginRecord>(&format!(
                "{SELECT} WHERE origin = ? AND username = ?"
            ))
            .bind(origin)
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;
            return Ok(row);
        }
        let row = sqlx::query_as::<_, SiteLoginRecord>(&format!(
            "{SELECT} WHERE origin = ? ORDER BY updated_at DESC LIMIT 1"
        ))
        .bind(origin)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// The words a saved row keeps when the new ones are blank: a second save of the same
    /// login (an import, the card's offer) must not wipe a title or notes the person wrote.
    fn keep_words(
        existing: Option<&SiteLoginRecord>,
        origin: &str,
        username: &str,
        label: &str,
        notes: &str,
    ) -> (String, String) {
        let label = match (label.trim(), existing) {
            ("", Some(row)) if !row.label.trim().is_empty() => row.label.clone(),
            ("", _) => default_label(username, origin),
            (label, _) => label.to_string(),
        };
        let notes = match (notes, existing) {
            ("", Some(row)) => row.notes.clone(),
            (notes, _) => notes.to_string(),
        };
        (label, notes)
    }

    /// Write password to the secret store, metadata to sqlite. Returns the row.
    pub async fn save(
        &self,
        origin: &str,
        username: &str,
        label: &str,
        notes: &str,
        kind: &str,
        password: &str,
    ) -> Result<SiteLoginRecord, StoreError> {
        let existing = self.find(origin, Some(username)).await?;
        let id = existing
            .as_ref()
            .map(|row| row.id.clone())
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let (label, notes) = Self::keep_words(existing.as_ref(), origin, username, label, notes);
        self.secrets.set(&id, password)?;
        sqlx::query(
            "INSERT INTO site_logins (id, origin, username, label, kind, notes, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                origin = excluded.origin,
                username = excluded.username,
                label = excluded.label,
                kind = excluded.kind,
                notes = excluded.notes,
                updated_at = datetime('now')",
        )
        .bind(&id)
        .bind(origin)
        .bind(username)
        .bind(&label)
        .bind(kind_or_default(kind))
        .bind(&notes)
        .execute(&self.pool)
        .await?;
        self.find(origin, Some(username))
            .await?
            .ok_or_else(|| StoreError::Db("site login row missing after save".into()))
    }

    /// Save under an id the server chose, so the keychain copy and the server row share it.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_with_id(
        &self,
        id: &str,
        origin: &str,
        username: &str,
        label: &str,
        notes: &str,
        kind: &str,
        password: &str,
    ) -> Result<SiteLoginRecord, StoreError> {
        let now = chrono::Utc::now().to_rfc3339();
        let existing = self.find(origin, Some(username)).await?;
        let (label, notes) = Self::keep_words(existing.as_ref(), origin, username, label, notes);
        // A row already here under another id moves to this one, keychain item included,
        // so no copy is left behind under an id nothing points at.
        if let Some(existing) = existing
            && existing.id != id
        {
            self.adopt_id(&existing.id, id).await?;
        }
        self.secrets.set(id, password)?;
        sqlx::query(
            "INSERT INTO site_logins (id, origin, username, label, kind, notes, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(origin, username) DO UPDATE SET
               id = excluded.id, label = excluded.label, kind = excluded.kind,
               notes = excluded.notes, updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(origin)
        .bind(username)
        .bind(&label)
        .bind(kind_or_default(kind))
        .bind(&notes)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        self.find(origin, Some(username))
            .await?
            .ok_or_else(|| StoreError::Db("the row did not come back".to_string()))
    }

    /// A row the server has and this Mac does not: metadata only, no secret yet. The
    /// password is fetched after Touch ID the first time it is used here, then cached.
    #[allow(clippy::too_many_arguments)]
    pub async fn remember_remote(
        &self,
        id: &str,
        origin: &str,
        username: &str,
        label: &str,
        notes: &str,
        kind: &str,
        last_used_at_ms: Option<i64>,
    ) -> Result<(), StoreError> {
        let now = chrono::Utc::now().to_rfc3339();
        let label = if label.trim().is_empty() {
            default_label(username, origin)
        } else {
            label.trim().to_string()
        };
        sqlx::query(
            "INSERT INTO site_logins
               (id, origin, username, label, kind, notes, last_used_at_ms, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(origin, username) DO NOTHING",
        )
        .bind(id)
        .bind(origin)
        .bind(username)
        .bind(&label)
        .bind(kind_or_default(kind))
        .bind(notes)
        .bind(last_used_at_ms)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The server's words for a row this Mac already has, taken only when the server's copy
    /// is the newer one: notes typed on another Mac arrive, notes typed here and not yet
    /// filed there do not get wiped by an older server row. True when something was written.
    pub async fn update_from_server(
        &self,
        row: &SiteLoginRecord,
        label: &str,
        notes: &str,
        kind: &str,
        last_used_at_ms: Option<i64>,
        updated_at_ms: i64,
    ) -> Result<bool, StoreError> {
        let local_ms = timestamp_ms(&row.updated_at).unwrap_or(0);
        if updated_at_ms <= local_ms {
            return Ok(false);
        }
        let label = if label.trim().is_empty() {
            row.label.clone()
        } else {
            label.trim().to_string()
        };
        // The row's stamp becomes the server's, so the next sync sees the two level.
        let stamp = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(updated_at_ms)
            .map(|at| at.to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        sqlx::query(
            "UPDATE site_logins
             SET label = ?, notes = ?, kind = ?, last_used_at_ms = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&label)
        .bind(notes)
        .bind(kind_or_default(kind))
        .bind(last_used_at_ms)
        .bind(&stamp)
        .bind(&row.id)
        .execute(&self.pool)
        .await?;
        Ok(true)
    }

    /// The person's words on the detail pane, as typed.
    pub async fn update_notes(&self, id: &str, notes: &str) -> Result<(), StoreError> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE site_logins SET notes = ?, updated_at = ? WHERE id = ?")
            .bind(notes)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// A row saved on this Mac before the server knew it, now filed on the server under
    /// `new_id`: the row and its keychain item move to that id.
    pub async fn adopt_id(&self, old_id: &str, new_id: &str) -> Result<(), StoreError> {
        if old_id == new_id {
            return Ok(());
        }
        if let Some(secret) = self.secrets.get(old_id)? {
            self.secrets.set(new_id, &secret)?;
        }
        sqlx::query("UPDATE site_logins SET id = ? WHERE id = ?")
            .bind(new_id)
            .bind(old_id)
            .execute(&self.pool)
            .await?;
        self.secrets.delete(old_id)?;
        Ok(())
    }

    /// Keep a password fetched from the server in this Mac's keychain, so the next use
    /// needs Touch ID only.
    pub fn cache_secret(&self, id: &str, password: &str) -> Result<(), StoreError> {
        self.secrets.set(id, password)
    }

    pub async fn delete(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM site_logins WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        self.secrets.delete(id)?;
        Ok(())
    }

    pub fn secret_present(&self, id: &str) -> bool {
        self.secrets.contains(id)
    }

    /// The password itself. Call it only right after the person passed Touch ID for this
    /// row, to hand the value to the fill, or once to file a row on the person's own server
    /// vault; never to paint it.
    pub fn secret_for_fill(&self, id: &str) -> Result<Option<String>, StoreError> {
        self.secrets.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn memory_db() -> SiteLoginVault {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("dsn")
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("schema");
        SiteLoginVault::memory(pool)
    }

    #[tokio::test]
    async fn save_list_delete_keeps_password_out_of_sqlite() {
        let vault = memory_db().await;
        let saved = vault
            .save("google.com", "ada@example.com", "", "", "", "s3cret-pass")
            .await
            .expect("save");
        assert_eq!(saved.origin, "google.com");
        assert_eq!(saved.username, "ada@example.com");
        assert!(!format!("{saved:?}").contains("s3cret-pass"));

        let cols: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM pragma_table_info('site_logins')")
                .fetch_all(&vault.pool)
                .await
                .expect("pragma");
        let names: Vec<&str> = cols.iter().map(|(name,)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "id",
                "origin",
                "username",
                "label",
                "created_at",
                "updated_at",
                "kind",
                "notes",
                "last_used_at_ms",
            ]
        );
        assert!(
            !names
                .iter()
                .any(|name| name.contains("pass") || name.contains("secret"))
        );

        let blob: Vec<(String, String, String, String, String)> =
            sqlx::query_as("SELECT id, origin, username, label, notes FROM site_logins")
                .fetch_all(&vault.pool)
                .await
                .expect("rows");
        let dump = format!("{blob:?}");
        assert!(!dump.contains("s3cret-pass"));

        assert_eq!(
            vault.secret_for_fill(&saved.id).expect("get").as_deref(),
            Some("s3cret-pass")
        );
        assert!(vault.secret_present(&saved.id));

        let listed = vault.list().await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].username, "ada@example.com");

        vault.delete(&saved.id).await.expect("delete");
        assert!(vault.list().await.expect("empty").is_empty());
        assert!(!vault.secret_present(&saved.id));
        assert_eq!(vault.secret_for_fill(&saved.id).expect("gone"), None);
    }

    #[tokio::test]
    async fn save_same_origin_username_updates_secret() {
        let vault = memory_db().await;
        let first = vault
            .save("github.com", "ada", "", "", "", "one")
            .await
            .expect("first");
        let second = vault
            .save("github.com", "ada", "", "", "", "two")
            .await
            .expect("second");
        assert_eq!(first.id, second.id);
        assert_eq!(
            vault.secret_for_fill(&first.id).expect("get").as_deref(),
            Some("two")
        );
        assert_eq!(vault.list().await.expect("one row").len(), 1);
    }

    #[tokio::test]
    async fn a_server_id_is_adopted_and_a_remote_row_waits_for_its_secret() {
        let vault = memory_db().await;
        let local = vault
            .save("github.com", "ada", "", "", "", "pw-local")
            .await
            .expect("save");
        vault.adopt_id(&local.id, "sl_server").await.expect("adopt");
        let rows = vault.list().await.expect("list");
        assert_eq!(rows[0].id, "sl_server");
        assert_eq!(
            vault.secret_for_fill("sl_server").expect("get").as_deref(),
            Some("pw-local")
        );
        assert!(!vault.secret_present(&local.id), "the old item is gone");

        vault
            .remember_remote("sl_other", "gitlab.com", "ada", "", "", "", None)
            .await
            .expect("remember");
        assert!(!vault.secret_present("sl_other"));
        vault.cache_secret("sl_other", "pw-remote").expect("cache");
        assert_eq!(
            vault.secret_for_fill("sl_other").expect("get").as_deref(),
            Some("pw-remote")
        );
        // Remembering again does not clobber what is there.
        vault
            .remember_remote("sl_dup", "gitlab.com", "ada", "", "", "", None)
            .await
            .expect("remember again");
        let rows = vault.list().await.expect("list");
        assert_eq!(rows.iter().filter(|r| r.origin == "gitlab.com").count(), 1);

        let saved = vault
            .save_with_id("sl_new", "x.com", "bea", "", "", "", "pw")
            .await
            .expect("save with id");
        assert_eq!(saved.id, "sl_new");
        // The same login saved under the server's id later: one row, one keychain item.
        let local = vault
            .save("y.com", "cy", "", "", "", "pw1")
            .await
            .expect("save");
        let moved = vault
            .save_with_id("sl_y", "y.com", "cy", "", "", "", "pw2")
            .await
            .expect("save with id");
        assert_eq!(moved.id, "sl_y");
        assert!(!vault.secret_present(&local.id), "the old item moved");
        assert_eq!(
            vault.secret_for_fill("sl_y").expect("get").as_deref(),
            Some("pw2")
        );
        assert_eq!(
            vault
                .list()
                .await
                .expect("list")
                .iter()
                .filter(|r| r.origin == "y.com")
                .count(),
            1
        );
    }

    /// A row saved with nothing but the login gets the plain defaults; one saved with a
    /// title, notes and a kind keeps them; a remembered server row carries its last use.
    #[tokio::test]
    async fn the_new_columns_default_and_are_kept_when_given() {
        let vault = memory_db().await;
        let plain = vault
            .save("x.com", "ada", "", "", "", "pw")
            .await
            .expect("save");
        assert_eq!(plain.label, "ada on x.com");
        assert_eq!(plain.kind, "password");
        assert_eq!(plain.notes, "");
        assert_eq!(plain.last_used_at_ms, None);

        let full = vault
            .save_with_id(
                "sl_1",
                "y.com",
                "bea",
                "Work mail",
                "Security: 2FA on the phone",
                "passkey",
                "pw",
            )
            .await
            .expect("save with id");
        assert_eq!(full.label, "Work mail");
        assert_eq!(full.kind, "passkey");
        assert_eq!(full.notes, "Security: 2FA on the phone");

        vault
            .remember_remote(
                "sl_2",
                "z.com",
                "cy",
                "",
                "",
                "code",
                Some(1_700_000_000_000),
            )
            .await
            .expect("remember");
        let remote = vault
            .find("z.com", Some("cy"))
            .await
            .expect("find")
            .unwrap();
        assert_eq!(remote.label, "cy on z.com");
        assert_eq!(remote.kind, "code");
        assert_eq!(remote.last_used_at_ms, Some(1_700_000_000_000));
    }

    /// A second save of the same login with blank words keeps the title and notes the
    /// person wrote; new words replace them.
    #[tokio::test]
    async fn a_resave_with_blank_words_keeps_the_old_ones() {
        let vault = memory_db().await;
        vault
            .save(
                "x.com",
                "ada",
                "Bank",
                "Security: ask for the card",
                "",
                "pw1",
            )
            .await
            .expect("save");
        let again = vault
            .save("x.com", "ada", "", "", "", "pw2")
            .await
            .expect("resave");
        assert_eq!(again.label, "Bank");
        assert_eq!(again.notes, "Security: ask for the card");
        let renamed = vault
            .save_with_id("sl_1", "x.com", "ada", "Savings", "", "", "pw3")
            .await
            .expect("save with id");
        assert_eq!(renamed.label, "Savings");
        assert_eq!(renamed.notes, "Security: ask for the card");
    }

    /// Notes typed here are written with a fresh stamp; the server's words land only when
    /// its row is newer than this Mac's.
    #[tokio::test]
    async fn notes_update_and_a_newer_server_row_wins() {
        let vault = memory_db().await;
        let row = vault
            .save_with_id("sl_1", "x.com", "ada", "", "", "", "pw")
            .await
            .expect("save");
        vault
            .update_notes("sl_1", "Security: call first")
            .await
            .expect("notes");
        let after = vault
            .find("x.com", Some("ada"))
            .await
            .expect("find")
            .unwrap();
        assert_eq!(after.notes, "Security: call first");
        assert!(after.updated_at >= row.updated_at);

        let local_ms = timestamp_ms(&after.updated_at).expect("stamp");
        let stale = vault
            .update_from_server(
                &after,
                "Old",
                "old words",
                "code",
                Some(5),
                local_ms - 1_000,
            )
            .await
            .expect("stale");
        assert!(!stale);
        let kept = vault
            .find("x.com", Some("ada"))
            .await
            .expect("find")
            .unwrap();
        assert_eq!(kept.notes, "Security: call first");
        assert_eq!(kept.kind, "password");

        let newer = vault
            .update_from_server(
                &kept,
                "From the other Mac",
                "typed there",
                "passkey",
                Some(1_700_000_000_000),
                local_ms + 60_000,
            )
            .await
            .expect("newer");
        assert!(newer);
        let taken = vault
            .find("x.com", Some("ada"))
            .await
            .expect("find")
            .unwrap();
        assert_eq!(taken.label, "From the other Mac");
        assert_eq!(taken.notes, "typed there");
        assert_eq!(taken.kind, "passkey");
        assert_eq!(taken.last_used_at_ms, Some(1_700_000_000_000));
        assert_eq!(timestamp_ms(&taken.updated_at), Some(local_ms + 60_000));
        // The same server row again is not newer than what it just wrote.
        let same = vault
            .update_from_server(
                &taken,
                "",
                "typed there",
                "passkey",
                None,
                local_ms + 60_000,
            )
            .await
            .expect("same");
        assert!(!same);
    }
}
