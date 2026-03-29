use super::user_prefs::{UserPrefs, UserPrefsStore};
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SqliteUserPrefsStore {
    pool: SqlitePool,
    prefs: Mutex<HashMap<String, Arc<UserPrefs>>>,
}

#[async_trait]
impl UserPrefsStore for SqliteUserPrefsStore {
    async fn get(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<Arc<UserPrefs>, crate::error::AppError> {
        let key = format!("{}_{}", chat_id, user_id);
        let prefs = self.prefs.lock().await;
        Ok(prefs.get(&key).cloned().unwrap_or_default())
    }

    async fn set(
        &self,
        chat_id: i64,
        user_id: i64,
        prefs: UserPrefs,
    ) -> Result<(), crate::error::AppError> {
        let key = format!("{}_{}", chat_id, user_id);
        let soul = prefs.soul.clone();
        {
            let mut prefs_map = self.prefs.lock().await;
            let user_prefs_arc = prefs_map
                .entry(key.clone())
                .or_insert_with(|| Arc::new(UserPrefs::default()));
            let user_prefs = Arc::make_mut(user_prefs_arc);
            *user_prefs = prefs;
        }

        sqlx::query(
            "INSERT INTO user_prefs (user_key, soul) VALUES (?, ?)
             ON CONFLICT(user_key) DO UPDATE SET soul = excluded.soul",
        )
        .bind(&key)
        .bind(&soul)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            eprintln!("Failed to persist user prefs: {}", e);
            crate::error::AppError::UserPrefsSaveError
        })?;

        Ok(())
    }
}

impl SqliteUserPrefsStore {
    pub async fn new(pool: SqlitePool) -> Result<Self, crate::error::AppError> {
        let mut prefs_map = HashMap::new();

        let rows = sqlx::query("SELECT user_key, soul FROM user_prefs")
            .fetch_all(&pool)
            .await
            .map_err(|e| {
                eprintln!("Failed to load user prefs: {}", e);
                crate::error::AppError::UserPrefsLoadError
            })?;

        for row in rows {
            let key: String = row.try_get("user_key").map_err(|e| {
                eprintln!("Failed to read user_key: {}", e);
                crate::error::AppError::UserPrefsLoadError
            })?;
            let soul: String = row.try_get("soul").map_err(|e| {
                eprintln!("Failed to read soul: {}", e);
                crate::error::AppError::UserPrefsLoadError
            })?;
            prefs_map.insert(key, Arc::new(UserPrefs { soul }));
        }

        Ok(SqliteUserPrefsStore {
            pool,
            prefs: Mutex::new(prefs_map),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sqlite_pool::open_sqlite_pool;
    use crate::db::user_prefs::UserPrefs;
    use crate::util::testutil;
    use std::path::PathBuf;

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rs_tgbot_user_prefs_{}.db",
            testutil::temp_path_suffix()
        ))
    }

    #[tokio::test]
    async fn sqlite_user_prefs_get_default() {
        let path = temp_db_path();
        let pool = open_sqlite_pool(&path).await.unwrap();
        let store = SqliteUserPrefsStore::new(pool).await.unwrap();

        let prefs = store.get(10, 20).await.unwrap();
        assert_eq!(prefs.soul, "neuro");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn sqlite_user_prefs_set_and_persist() {
        let path = temp_db_path();
        {
            let pool = open_sqlite_pool(&path).await.unwrap();
            let store = SqliteUserPrefsStore::new(pool).await.unwrap();
            store
                .set(
                    1,
                    2,
                    UserPrefs {
                        soul: "custom".to_string(),
                    },
                )
                .await
                .unwrap();
        }

        let pool = open_sqlite_pool(&path).await.unwrap();
        let store = SqliteUserPrefsStore::new(pool).await.unwrap();
        let prefs = store.get(1, 2).await.unwrap();
        assert_eq!(prefs.soul, "custom");

        let _ = std::fs::remove_file(&path);
    }
}
