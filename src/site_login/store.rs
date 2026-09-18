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

    /// Tests / A.1 only. Never call from a renderer.
    pub fn secret_for_test(&self, id: &str) -> Result<Option<String>, StoreError> {
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
            vault.secret_for_test(&saved.id).expect("get").as_deref(),
            Some("s3cret-pass")
        );
        assert!(vault.secret_present(&saved.id));

        let listed = vault.list().await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].username, "ada@example.com");

        vault.delete(&saved.id).await.expect("delete");
        assert!(vault.list().await.expect("empty").is_empty());
        assert!(!vault.secret_present(&saved.id));
        assert_eq!(vault.secret_for_test(&saved.id).expect("gone"), None);
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
            vault.secret_for_test(&first.id).expect("get").as_deref(),
            Some("two")
        );
        assert_eq!(vault.list().await.expect("one row").len(), 1);
    }
}
