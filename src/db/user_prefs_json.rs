use super::user_prefs::{UserPrefs, UserPrefsStore};
use async_trait::async_trait;
use std::collections::HashMap;
//use std::fs;  //will block other user use tokio instead
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;

// simple JSON version
#[allow(dead_code)]
pub struct JsonUserPrefsStore {
    prefs: Mutex<HashMap<String, Arc<UserPrefs>>>,
    file_path: String,
}

#[async_trait]
#[allow(dead_code)]
impl UserPrefsStore for JsonUserPrefsStore {
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
        // let mut prefs_map = self.prefs.lock().await;
        // prefs_map.insert(user_id, prefs);
        // self.save_to_file(&prefs_map)

        let prefs_map = {
            let mut prefs_map = self.prefs.lock().await;
            let user_prefs_arc = prefs_map
                .entry(key)
                .or_insert_with(|| Arc::new(UserPrefs::default()));
            let user_prefs = Arc::make_mut(user_prefs_arc);
            *user_prefs = prefs;
            prefs_map.clone()
        };
        let data_to_write = serde_json::to_string_pretty(&prefs_map).map_err(|e| {
            eprintln!("Failed to serialize user prefs: {}", e);
            crate::error::AppError::UserPrefsSaveError
        })?;

        self.save_to_file(&data_to_write).await
    }
}

#[allow(dead_code)]
impl JsonUserPrefsStore {
    pub async fn new(file_path: &str) -> Result<Self, crate::error::AppError> {
        let prefs = Self::load_from_file(file_path).await?;
        Ok(JsonUserPrefsStore {
            prefs: Mutex::new(prefs),
            file_path: file_path.to_string(),
        })
    }

    async fn load_from_file(
        file_path: &str,
    ) -> Result<HashMap<String, Arc<UserPrefs>>, crate::error::AppError> {
        if Path::new(file_path).exists() {
            let data = fs::read_to_string(file_path).await.map_err(|e| {
                eprintln!("Failed to read user prefs file: {}", e);
                crate::error::AppError::UserPrefsLoadError
            })?;
            serde_json::from_str(&data).map_err(|_| crate::error::AppError::UserPrefsLoadError)
        } else {
            Ok(HashMap::new())
        }
    }

    async fn save_to_file(&self, data: &str) -> Result<(), crate::error::AppError> {
        fs::write(&self.file_path, data).await.map_err(|e| {
            eprintln!("Failed to save user prefs: {}", e);
            crate::error::AppError::UserPrefsSaveError
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testutil;
    use std::path::PathBuf;

    fn temp_prefs_file() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rs_tgbot_json_prefs_{}.json",
            testutil::temp_path_suffix()
        ))
    }

    #[test]
    fn user_prefs_default_matches_expected() {
        let p = UserPrefs::default();
        assert_eq!(p.soul, "neuro");
    }

    #[tokio::test]
    async fn json_user_prefs_set_and_get() {
        let path = temp_prefs_file();
        let store = JsonUserPrefsStore::new(path.to_str().unwrap())
            .await
            .unwrap();

        store
            .set(
                100,
                200,
                UserPrefs {
                    soul: "alpha".to_string(),
                },
            )
            .await
            .unwrap();

        let got = store.get(100, 200).await.unwrap();
        assert_eq!(got.soul, "alpha");

        let _ = std::fs::remove_file(&path);
    }
}
