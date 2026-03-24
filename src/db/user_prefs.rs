use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
//use std::fs;  //will block other user use tokio instead
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserPrefs {
    pub soul: String,
}

impl Default for UserPrefs {
    fn default() -> Self {
        UserPrefs {
            soul: "neuro".to_string(), // default to neuro
        }
    }
}

// TODO: postgres version
#[async_trait]
pub trait UserPrefsStore: Send + Sync {
    async fn get(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<Arc<UserPrefs>, crate::error::AppError>;
    async fn set(
        &self,
        chat_id: i64,
        user_id: i64,
        prefs: UserPrefs,
    ) -> Result<(), crate::error::AppError>;
}

// simple JSON version
pub struct JsonUserPrefsStore {
    prefs: Mutex<HashMap<String, Arc<UserPrefs>>>, // user_id -> prefs
    file_path: String,
}

#[async_trait]
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
