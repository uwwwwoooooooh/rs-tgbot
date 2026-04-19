use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;

pub async fn open_sqlite_pool(path: &Path) -> Result<SqlitePool, crate::error::AppError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            eprintln!("Failed to create sqlite dir: {}", e);
            crate::error::AppError::UserHistorySaveError
        })?;
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await
        .map_err(|e| {
            eprintln!("Failed to open sqlite pool: {}", e);
            crate::error::AppError::UserHistoryLoadError
        })?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chat_history (
            user_key TEXT PRIMARY KEY NOT NULL,
            messages_json TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("Failed to init chat_history table: {}", e);
        crate::error::AppError::UserHistorySaveError
    })?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS user_prefs (
            user_key TEXT PRIMARY KEY NOT NULL,
            soul TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("Failed to init user_prefs table: {}", e);
        crate::error::AppError::UserPrefsSaveError
    })?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testutil;
    use std::path::PathBuf;

    fn unique_db_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rs_tgbot_sqlite_pool_{}.db",
            testutil::temp_path_suffix()
        ))
    }

    #[tokio::test]
    async fn open_sqlite_pool_creates_tables() {
        let path = unique_db_path();
        let pool = open_sqlite_pool(&path).await.unwrap();

        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('chat_history', 'user_prefs')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 2);

        let _ = std::fs::remove_file(&path);
    }
}
