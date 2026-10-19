use crate::config::Config;
use anyhow::{Context, Result, ensure};
use sqlx::{
    Connection, SqliteConnection, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{path::Path, time::Duration};

pub async fn connect(config: &Config) -> Result<SqlitePool> {
    check_existing_database(&config.database_path).await?;
    if let Some(parent) = config
        .database_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    let options = SqliteConnectOptions::new()
        .filename(&config.database_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(config.database_connections)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await?;
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("Database schema upgrade failed")?;
    Ok(pool)
}

async fn check_existing_database(database: &Path) -> Result<()> {
    if !database.try_exists()? || database.metadata()?.len() == 0 {
        return Ok(());
    }
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(database)
            .read_only(true),
    )
    .await?;
    let result = async {
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_all(&mut connection)
        .await?;
        if tables.iter().any(|table| table != "_sqlx_migrations") {
            ensure!(
                tables.iter().any(|table| table == "_sqlx_migrations"),
                "Existing database has no Rust schema history; legacy database migration is no longer supported"
            );
            let initialized: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE success=1)",
            )
            .fetch_one(&mut connection)
            .await?;
            ensure!(
                initialized,
                "Existing database has no completed Rust schema history; refusing to modify it"
            );
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    connection.close().await?;
    result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unsupported_existing_databases_are_rejected_without_upgrading_or_backing_up() {
        for empty_history in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut config = Config::from_env().unwrap();
            config.database_path = directory.path().join("database.db");
            let mut source = SqliteConnection::connect_with(
                &SqliteConnectOptions::new()
                    .filename(&config.database_path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
            sqlx::raw_sql("CREATE TABLE user(id INTEGER PRIMARY KEY, name TEXT); INSERT INTO user VALUES(7,'Keep original');")
                .execute(&mut source).await.unwrap();
            if empty_history {
                sqlx::query(
                    "CREATE TABLE _sqlx_migrations(version BIGINT PRIMARY KEY, success BOOLEAN)",
                )
                .execute(&mut source)
                .await
                .unwrap();
            }
            source.close().await.unwrap();
            let before = std::fs::read(&config.database_path).unwrap();
            let error = connect(&config).await.unwrap_err().to_string();
            assert!(error.contains("schema history"), "{error}");
            assert_eq!(std::fs::read(&config.database_path).unwrap(), before);
            assert!(!directory.path().join("backups").exists());
        }
    }

    #[tokio::test]
    async fn empty_and_current_databases_initialize_and_reopen_without_legacy_conversion() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::from_env().unwrap();
        config.database_path = directory.path().join("database.db");
        let pool = connect(&config).await.unwrap();
        sqlx::query("INSERT INTO system_setting(config_name,config_value) VALUES('test-value','Keep current')")
            .execute(&pool).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        let pool = connect(&config).await.unwrap();
        let value: String = sqlx::query_scalar(
            "SELECT config_value FROM system_setting WHERE config_name='test-value'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(value, "Keep current");
        let reopened_count: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, reopened_count);
        assert!(!directory.path().join("backups").exists());
        pool.close().await;
    }
}
