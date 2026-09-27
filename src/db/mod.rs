//! Database module for SQLite with sqlx.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::str::FromStr;

use crate::error::Result;

pub type DbPool = Pool<Sqlite>;

pub async fn create_pool(database_url: &str) -> Result<DbPool> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    Ok(pool)
}

pub async fn run_migrations(pool: &DbPool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    eprintln!("Database ready");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DbPool, create_pool, run_migrations};
    use sqlx::Row;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    const MIGRATIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

    fn migration_files() -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(MIGRATIONS)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
            .collect();
        files.sort();
        files
    }

    /// Tables a migration drops on purpose: `20251203000000_create_chat_sessions.sql` resets the
    /// chat tables. Rows seeded before such a migration are expected to go with it.
    fn dropped_by(files: &[PathBuf]) -> BTreeSet<String> {
        let mut dropped = BTreeSet::new();
        for file in files {
            let sql = std::fs::read_to_string(file).unwrap().to_ascii_lowercase();
            for line in sql.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("drop table") {
                    let rest = rest.trim().trim_start_matches("if exists").trim();
                    dropped.insert(rest.trim_end_matches(';').trim().to_string());
                }
            }
        }
        dropped
    }

    /// Every user table and its row count.
    async fn row_counts(pool: &DbPool) -> BTreeMap<String, i64> {
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%'",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        let mut counts = BTreeMap::new();
        for table in tables {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{table}\""))
                .fetch_one(pool)
                .await
                .unwrap();
            counts.insert(table, count);
        }
        counts
    }

    /// One row in every table, whatever the schema at this point says it needs: a value of the
    /// declared type for each NOT NULL column with no default, and for a non-integer primary key.
    async fn seed_every_table(pool: &DbPool) {
        let mut conn = pool.acquire().await.unwrap();
        // A seeded row points at nothing; foreign keys are the schema's business, not this row's.
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *conn)
            .await
            .unwrap();
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%'",
        )
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        for table in tables {
            let columns = sqlx::query(&format!("PRAGMA table_info(\"{table}\")"))
                .fetch_all(&mut *conn)
                .await
                .unwrap();
            // Only a table's one `INTEGER PRIMARY KEY` is the rowid, which SQLite fills in; a
            // column in a key of several is an ordinary column that needs a value.
            let key_columns = columns
                .iter()
                .filter(|column| column.get::<i64, _>("pk") > 0)
                .count();
            let mut names = Vec::new();
            let mut values = Vec::new();
            for column in columns {
                let name: String = column.get("name");
                let kind: String = column.get::<String, _>("type").to_ascii_uppercase();
                let not_null: i64 = column.get("notnull");
                let default: Option<String> = column.get("dflt_value");
                let pk: i64 = column.get("pk");
                let integer_key = pk > 0 && key_columns == 1 && kind == "INTEGER";
                if integer_key || !(pk > 0 || (not_null == 1 && default.is_none())) {
                    continue;
                }
                let value = if kind.contains("INT") {
                    "1".to_string()
                } else if kind.contains("REAL") || kind.contains("FLOA") || kind.contains("DOUB") {
                    "1.5".to_string()
                } else if kind.contains("BLOB") {
                    "x'00'".to_string()
                } else {
                    format!("'seed-{table}-{name}'")
                };
                names.push(format!("\"{name}\""));
                values.push(value);
            }
            let sql = if names.is_empty() {
                format!("INSERT INTO \"{table}\" DEFAULT VALUES")
            } else {
                format!(
                    "INSERT INTO \"{table}\" ({}) VALUES ({})",
                    names.join(", "),
                    values.join(", ")
                )
            };
            sqlx::query(&sql)
                .execute(&mut *conn)
                .await
                .unwrap_or_else(|e| panic!("seeding {table}: {e}\n{sql}"));
        }
    }

    /// An install upgrades from whatever schema it was last run with, with its data in it, and
    /// the app panics at launch if a migration fails. For every point in the history: build the
    /// schema up to there, put a row in every table, then run the app's own migrations to the
    /// end. Every migration must succeed on a populated database and every row must survive,
    /// except where a later migration drops its table on purpose.
    #[tokio::test]
    async fn every_past_schema_with_data_in_it_upgrades_to_the_current_one() {
        let files = migration_files();
        assert!(files.len() > 1, "no migrations found in {MIGRATIONS}");
        for applied in 1..files.len() {
            let dir = tempfile::tempdir().unwrap();
            let past = dir.path().join("migrations");
            std::fs::create_dir(&past).unwrap();
            for file in &files[..applied] {
                std::fs::copy(file, past.join(file.file_name().unwrap())).unwrap();
            }
            let url = format!("sqlite://{}", dir.path().join("app.db").display());
            let pool = create_pool(&url).await.unwrap();
            sqlx::migrate::Migrator::new(past.as_path())
                .await
                .unwrap()
                .run(&pool)
                .await
                .unwrap();
            seed_every_table(&pool).await;
            let before = row_counts(&pool).await;

            let last = files[applied - 1]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            run_migrations(&pool)
                .await
                .unwrap_or_else(|e| panic!("upgrading from {last} failed: {e}"));

            let after = row_counts(&pool).await;
            let dropped = dropped_by(&files[applied..]);
            for (table, count) in &before {
                if dropped.contains(table) {
                    continue;
                }
                assert_eq!(
                    after.get(table),
                    Some(count),
                    "upgrading from {last}: rows in {table} did not survive"
                );
            }
            pool.close().await;
        }
    }
}
