//! sqlite metadata + secret backend. Never a password column.

use std::path::Path;
use std::sync::Arc;

use crate::db::DbPool;

use super::StoreError;
use super::secrets::{MemorySecrets, SecretStore, open_secrets};

/// Row the Settings list paints. No secret.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SiteLoginRecord {
    pub id: String,
    pub origin: String,
    pub username: String,
    pub label: String,
    pub created_at: String,
    pub updated_at: String,
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
        let rows = sqlx::query_as::<_, SiteLoginRecord>(
            "SELECT id, origin, username, label, created_at, updated_at
             FROM site_logins
             ORDER BY origin, username",
        )
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
            let row = sqlx::query_as::<_, SiteLoginRecord>(
                "SELECT id, origin, username, label, created_at, updated_at
                 FROM site_logins WHERE origin = ? AND username = ?",
            )
            .bind(origin)
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;
            return Ok(row);
        }
        let row = sqlx::query_as::<_, SiteLoginRecord>(
            "SELECT id, origin, username, label, created_at, updated_at
             FROM site_logins WHERE origin = ? ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(origin)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Write password to the secret store, metadata to sqlite. Returns the id.
    pub async fn save(
        &self,
        origin: &str,
        username: &str,
        password: &str,
    ) -> Result<SiteLoginRecord, StoreError> {
        let existing = self.find(origin, Some(username)).await?;
        let id = existing
            .as_ref()
            .map(|row| row.id.clone())
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let label = format!("{username} on {origin}");
        self.secrets.set(&id, password)?;
        sqlx::query(
            "INSERT INTO site_logins (id, origin, username, label, created_at, updated_at)
             VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                origin = excluded.origin,
                username = excluded.username,
                label = excluded.label,
                updated_at = datetime('now')",
        )
        .bind(&id)
        .bind(origin)
        .bind(username)
        .bind(&label)
        .execute(&self.pool)
        .await?;
        self.find(origin, Some(username))
            .await?
            .ok_or_else(|| StoreError::Db("site login row missing after save".into()))
    }

    /// Save under an id the server chose, so the keychain copy and the server row share it.
    pub async fn save_with_id(
        &self,
        id: &str,
        origin: &str,
        username: &str,
        password: &str,
    ) -> Result<SiteLoginRecord, StoreError> {
        let now = chrono::Utc::now().to_rfc3339();
        let label = format!("{username} on {origin}");
        // A row already here under another id moves to this one, keychain item included,
        // so no copy is left behind under an id nothing points at.
        if let Some(existing) = self.find(origin, Some(username)).await?
            && existing.id != id
        {
            self.adopt_id(&existing.id, id).await?;
        }
        self.secrets.set(id, password)?;
        sqlx::query(
            "INSERT INTO site_logins (id, origin, username, label, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(origin, username) DO UPDATE SET
               id = excluded.id, label = excluded.label, updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(origin)
        .bind(username)
        .bind(&label)
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
    pub async fn remember_remote(
        &self,
        id: &str,
        origin: &str,
        username: &str,
    ) -> Result<(), StoreError> {
        let now = chrono::Utc::now().to_rfc3339();
        let label = format!("{username} on {origin}");
        sqlx::query(
            "INSERT INTO site_logins (id, origin, username, label, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(origin, username) DO NOTHING",
        )
        .bind(id)
        .bind(origin)
        .bind(username)
        .bind(&label)
        .bind(&now)
        .bind(&now)
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
            .save("google.com", "ada@example.com", "s3cret-pass")
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
                "updated_at"
            ]
        );
        assert!(
            !names
                .iter()
                .any(|name| name.contains("pass") || name.contains("secret"))
        );

        let blob: Vec<(String, String, String, String)> =
            sqlx::query_as("SELECT id, origin, username, label FROM site_logins")
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
        let first = vault.save("github.com", "ada", "one").await.expect("first");
        let second = vault
            .save("github.com", "ada", "two")
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
            .save("github.com", "ada", "pw-local")
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
            .remember_remote("sl_other", "gitlab.com", "ada")
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
            .remember_remote("sl_dup", "gitlab.com", "ada")
            .await
            .expect("remember again");
        let rows = vault.list().await.expect("list");
        assert_eq!(rows.iter().filter(|r| r.origin == "gitlab.com").count(), 1);

        let saved = vault
            .save_with_id("sl_new", "x.com", "bea", "pw")
            .await
            .expect("save with id");
        assert_eq!(saved.id, "sl_new");
        // The same login saved under the server's id later: one row, one keychain item.
        let local = vault.save("y.com", "cy", "pw1").await.expect("save");
        let moved = vault
            .save_with_id("sl_y", "y.com", "cy", "pw2")
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
}
