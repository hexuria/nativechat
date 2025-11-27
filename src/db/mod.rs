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

pub async fn run_migrations(_pool: &DbPool) -> Result<()> {
    // Migrations will be handled separately for now
    // sqlx::migrate!("./migrations").run(pool).await?;
    println!("Database ready");
    Ok(())
}
