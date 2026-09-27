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
    /// A statement this test builds from the schema itself: table, index and column names read
    /// back out of `sqlite_master` and `PRAGMA`, each written inside double quotes, never a
    /// value from outside. sqlx 0.9 asks for SQL that is not a literal to say so.
    fn audited(sql: String) -> sqlx::AssertSqlSafe<String> {
        sqlx::AssertSqlSafe(sql)
    }

    use super::{DbPool, create_pool, run_migrations};
    use sqlx::Row;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    const MIGRATIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

    /// Tables a migration drops on purpose, data and all, as (migration file, table). Empty: no
    /// shipped migration drops a table that held rows before it ran. A migration that means to
    /// adds its entry here, in the same change, so losing that data is a decision and not an
    /// accident the test waves through.
    const INTENDED_DROPS: &[(&str, &str)] = &[];

    fn migration_files() -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(MIGRATIONS)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
            .collect();
        files.sort();
        files
    }

    fn file_name(path: &Path) -> String {
        path.file_name().unwrap().to_string_lossy().to_string()
    }

    async fn user_tables(pool: &DbPool) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%' ORDER BY name",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    async fn column_names(pool: &DbPool, table: &str) -> Vec<String> {
        sqlx::query(audited(format!("PRAGMA table_info(\"{table}\")")))
            .fetch_all(pool)
            .await
            .unwrap()
            .iter()
            .map(|column| column.get::<String, _>("name"))
            .collect()
    }

    /// Every row of `table`, over `columns`, as SQLite writes it back as a literal (`quote`), in a
    /// stable order: the table's contents, compared value by value rather than counted.
    async fn contents(
        pool: &DbPool,
        table: &str,
        columns: &[String],
    ) -> Result<Vec<String>, String> {
        let row = columns
            .iter()
            .map(|column| format!("quote(\"{column}\")"))
            .collect::<Vec<_>>()
            .join(" || ',' || ");
        sqlx::query_scalar(audited(format!("SELECT {row} FROM \"{table}\" ORDER BY 1")))
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())
    }

    /// A literal of the column's declared type, distinct per table, column and `copy`, and never
    /// NULL or a column's default: a migration that resets a value to either is a change the
    /// comparison sees.
    fn literal(table: &str, column: &str, kind: &str, copy: u32) -> String {
        if kind.contains("INT") {
            format!("{}", 1000 + copy)
        } else if kind.contains("REAL") || kind.contains("FLOA") || kind.contains("DOUB") {
            format!("{}.5", copy)
        } else if kind.contains("BLOB") {
            format!("x'0{copy}'")
        } else {
            format!("'seed{copy}-{table}-{column}'")
        }
    }

    /// Each unique index (a key included) on `table`, as its columns.
    async fn unique_indexes(pool: &DbPool, table: &str) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        for index in sqlx::query(audited(format!("PRAGMA index_list(\"{table}\")")))
            .fetch_all(pool)
            .await
            .unwrap()
        {
            if index.get::<i64, _>("unique") != 1 {
                continue;
            }
            let name: String = index.get("name");
            let columns = sqlx::query(audited(format!("PRAGMA index_info(\"{name}\")")))
                .fetch_all(pool)
                .await
                .unwrap()
                .iter()
                .filter_map(|column| column.get::<Option<String>, _>("name"))
                .collect();
            out.push(columns);
        }
        out
    }

    /// Two rows in every table, with foreign keys on, parents first, every foreign key pointing
    /// at a real parent row. Every column holds a value (never NULL, never its default), so a
    /// migration that wipes one is seen. The two rows are the same except where a key or unique
    /// index makes them differ, so a later unique index over existing duplicates meets them.
    async fn seed(pool: &DbPool) {
        let tables = user_tables(pool).await;
        let mut parents: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
        for table in &tables {
            let links = sqlx::query(audited(format!("PRAGMA foreign_key_list(\"{table}\")")))
                .fetch_all(pool)
                .await
                .unwrap()
                .iter()
                .map(|link| {
                    (
                        link.get::<String, _>("from"),
                        link.get::<String, _>("table"),
                        link.get::<Option<String>, _>("to").unwrap_or_default(),
                    )
                })
                .collect();
            parents.insert(table.clone(), links);
        }
        // Parents before children; a table whose parents are all seeded goes next.
        let mut order: Vec<String> = Vec::new();
        while order.len() < tables.len() {
            let before = order.len();
            for table in &tables {
                if order.contains(table) {
                    continue;
                }
                let ready = parents[table]
                    .iter()
                    .all(|(_, parent, _)| parent == table || order.contains(parent));
                if ready {
                    order.push(table.clone());
                }
            }
            assert!(
                order.len() > before,
                "foreign keys form a cycle: {tables:?}"
            );
        }
        // (table, column, copy) -> the literal that seeded row holds there.
        let mut seeded: BTreeMap<(String, String, u32), String> = BTreeMap::new();
        for table in &order {
            let columns = sqlx::query(audited(format!("PRAGMA table_info(\"{table}\")")))
                .fetch_all(pool)
                .await
                .unwrap();
            let key_columns = columns
                .iter()
                .filter(|column| column.get::<i64, _>("pk") > 0)
                .count();
            let rowid_key = |column: &sqlx::sqlite::SqliteRow| {
                column.get::<i64, _>("pk") > 0
                    && key_columns == 1
                    && column
                        .get::<String, _>("type")
                        .eq_ignore_ascii_case("INTEGER")
            };
            let uniques = unique_indexes(pool, table).await;
            let foreign: Vec<&str> = parents[table]
                .iter()
                .map(|(from, ..)| from.as_str())
                .collect();
            // A column the second row varies: in some unique index, and either not a foreign key
            // or in an index made only of foreign keys (which then point at the parents' second
            // rows instead).
            let varies = |name: &str| {
                uniques.iter().any(|index| {
                    index.iter().any(|c| c == name)
                        && (!foreign.contains(&name)
                            || index.iter().all(|c| foreign.contains(&c.as_str())))
                })
            };
            let count_sql = format!("SELECT COUNT(*) FROM \"{table}\"");
            let before: i64 = sqlx::query_scalar(audited(count_sql.clone()))
                .fetch_one(pool)
                .await
                .unwrap();
            for copy in 1..=2u32 {
                let mut names = Vec::new();
                let mut values = Vec::new();
                for column in &columns {
                    let name: String = column.get("name");
                    if rowid_key(column) {
                        continue; // SQLite fills the rowid in
                    }
                    let kind = column.get::<String, _>("type").to_ascii_uppercase();
                    let use_copy = if varies(&name) { copy } else { 1 };
                    let link = parents[table].iter().find(|(from, ..)| *from == name);
                    let value = match link {
                        Some((_, parent, to)) => {
                            let to = if to.is_empty() { "rowid" } else { to.as_str() };
                            seeded
                                .get(&(parent.clone(), to.to_string(), use_copy))
                                .unwrap_or_else(|| {
                                    panic!("{table}.{name}: no seeded {parent}.{to}")
                                })
                                .clone()
                        }
                        None => literal(table, &name, &kind, use_copy),
                    };
                    names.push(format!("\"{name}\""));
                    values.push(value);
                }
                let sql = format!(
                    "INSERT INTO \"{table}\" ({}) VALUES ({})",
                    names.join(", "),
                    values.join(", ")
                );
                let done = sqlx::query(audited(sql.clone()))
                    .execute(pool)
                    .await
                    .unwrap_or_else(|e| panic!("seeding {table} (row {copy}): {e}\n{sql}"));
                for (name, value) in names.iter().zip(&values) {
                    seeded.insert(
                        (table.clone(), name.trim_matches('"').into(), copy),
                        value.clone(),
                    );
                }
                let rowid = done.last_insert_rowid().to_string();
                seeded.insert((table.clone(), "rowid".into(), copy), rowid.clone());
                for column in &columns {
                    if rowid_key(column) {
                        seeded.insert((table.clone(), column.get("name"), copy), rowid.clone());
                    }
                }
            }
            let after: i64 = sqlx::query_scalar(audited(count_sql.clone()))
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(after - before, 2, "{table}: both seeded rows are there");
        }
    }

    /// The schema as SQLite stores it: every table, index and trigger with its SQL.
    async fn schema(pool: &DbPool) -> Vec<(String, String, Option<String>)> {
        sqlx::query_as(
            "SELECT type, name, sql FROM sqlite_master \
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    /// For every point in `files`' history: build the schema that far, seed it, finish the
    /// upgrade with `finish`, and check that it succeeded, that foreign keys were on for it, and
    /// that every table that existed keeps every column and every row, value for value.
    async fn upgrade_from_every_point<F, Fut>(
        files: &[PathBuf],
        fresh_schema: Option<&Vec<(String, String, Option<String>)>>,
        finish: F,
    ) -> Result<(), String>
    where
        F: Fn(DbPool) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        for applied in 1..files.len() {
            let last = file_name(&files[applied - 1]);
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
            seed(&pool).await;
            let mut before = BTreeMap::new();
            for table in user_tables(&pool).await {
                let columns = column_names(&pool, &table).await;
                let rows = contents(&pool, &table, &columns).await?;
                before.insert(table, (columns, rows));
            }

            let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
                .fetch_one(&pool)
                .await
                .unwrap();
            if foreign_keys != 1 {
                return Err(format!("upgrading from {last}: foreign keys were off"));
            }
            finish(pool.clone())
                .await
                .map_err(|e| format!("upgrading from {last} failed: {e}"))?;

            let applied_all: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            if applied_all != files.len() as i64 {
                return Err(format!(
                    "upgrading from {last}: {applied_all} of {} migrations applied",
                    files.len()
                ));
            }
            let broken: Vec<String> =
                sqlx::query_scalar("SELECT \"table\" FROM pragma_foreign_key_check")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            if !broken.is_empty() {
                return Err(format!(
                    "upgrading from {last}: rows point at missing parents in {broken:?}"
                ));
            }
            if let Some(fresh) = fresh_schema
                && &schema(&pool).await != fresh
            {
                return Err(format!(
                    "upgrading from {last}: the schema differs from a fresh install's"
                ));
            }
            let after_tables = user_tables(&pool).await;
            for (table, (columns, rows)) in &before {
                let dropped = INTENDED_DROPS.iter().any(|(file, dropped)| {
                    dropped == table && files[applied..].iter().any(|f| file_name(f) == *file)
                });
                if dropped {
                    continue;
                }
                if !after_tables.contains(table) {
                    return Err(format!("upgrading from {last}: table {table} is gone"));
                }
                let now = column_names(&pool, table).await;
                if let Some(missing) = columns.iter().find(|column| !now.contains(column)) {
                    return Err(format!(
                        "upgrading from {last}: column {table}.{missing} is gone"
                    ));
                }
                let kept = contents(&pool, table, columns).await?;
                if &kept != rows {
                    return Err(format!(
                        "upgrading from {last}: rows in {table} did not survive\n before: {rows:?}\n after:  {kept:?}"
                    ));
                }
            }
            pool.close().await;
        }
        Ok(())
    }

    /// An install upgrades from whatever schema it last ran, with its data in it, and the app
    /// panics at launch if a migration fails. From every point in the history, the app's own
    /// migrations must bring a populated database to the current schema and keep every row.
    #[tokio::test]
    async fn every_past_schema_with_data_in_it_upgrades_to_the_current_one() {
        let files = migration_files();
        assert!(files.len() > 1, "no migrations found in {MIGRATIONS}");
        // What a new install ends up with, to compare every upgrade against: an upgrade that
        // skips a step (`CREATE TABLE IF NOT EXISTS` over an older table) differs from it.
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join("fresh.db").display());
        let fresh_pool = create_pool(&url).await.unwrap();
        run_migrations(&fresh_pool).await.unwrap();
        let fresh = schema(&fresh_pool).await;
        fresh_pool.close().await;
        upgrade_from_every_point(&files, Some(&fresh), |pool| async move {
            run_migrations(&pool).await.map_err(|e| e.to_string())
        })
        .await
        .unwrap();
    }

    /// The shipped history plus one more migration, finished by a migrator over all of them.
    async fn upgrade_with_one_more(sql: &str) -> Result<(), String> {
        let extra = tempfile::tempdir().unwrap();
        let next = extra.path().join("29990101000000_one_more.sql");
        std::fs::write(&next, sql).unwrap();
        let mut files = migration_files();
        files.push(next);
        let all = extra.path().join("all");
        std::fs::create_dir(&all).unwrap();
        for file in &files {
            std::fs::copy(file, all.join(file.file_name().unwrap())).unwrap();
        }
        upgrade_from_every_point(&files, None, |pool| {
            let all = all.clone();
            async move {
                sqlx::migrate::Migrator::new(all.as_path())
                    .await
                    .map_err(|e| e.to_string())?
                    .run(&pool)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await
    }

    /// The check itself is checked: each of these migrations is wrong only once there is data,
    /// and each is caught.
    #[tokio::test]
    async fn a_migration_that_breaks_a_populated_database_is_caught() {
        let failed = upgrade_with_one_more("UPDATE credentials SET name = NULL;")
            .await
            .unwrap_err();
        assert!(
            failed.contains("NOT NULL constraint failed: credentials.name"),
            "{failed}"
        );

        let lost = upgrade_with_one_more("DELETE FROM site_logins;")
            .await
            .unwrap_err();
        assert!(
            lost.contains("rows in site_logins did not survive"),
            "{lost}"
        );

        let wiped = upgrade_with_one_more("UPDATE site_logins SET label = 'x';")
            .await
            .unwrap_err();
        assert!(
            wiped.contains("rows in site_logins did not survive"),
            "{wiped}"
        );

        let unique = upgrade_with_one_more("CREATE UNIQUE INDEX one_name ON credentials(name);")
            .await
            .unwrap_err();
        assert!(unique.contains("UNIQUE constraint failed"), "{unique}");

        let dropped_column = upgrade_with_one_more("ALTER TABLE site_logins DROP COLUMN notes;")
            .await
            .unwrap_err();
        assert!(
            dropped_column.contains("column site_logins.notes is gone"),
            "{dropped_column}"
        );

        let dropped_table = upgrade_with_one_more("DROP TABLE site_logins;")
            .await
            .unwrap_err();
        assert!(
            dropped_table.contains("table site_logins is gone"),
            "{dropped_table}"
        );

        // Nullable and defaulted columns hold real values too, so a reset to NULL or to the
        // default is a change, and a unique index over them meets the seeded duplicates.
        for (sql, expect) in [
            (
                "UPDATE site_logins SET notes = '';",
                "rows in site_logins did not survive",
            ),
            (
                "UPDATE chat_messages SET run_id = NULL;",
                "rows in chat_messages did not survive",
            ),
            (
                "CREATE UNIQUE INDEX one_label ON site_logins(label);",
                "UNIQUE constraint failed",
            ),
            (
                "CREATE UNIQUE INDEX one_run ON chat_messages(run_id);",
                "UNIQUE constraint failed",
            ),
        ] {
            let caught = upgrade_with_one_more(sql).await.unwrap_err();
            assert!(caught.contains(expect), "{sql}: {caught}");
        }
    }
}
