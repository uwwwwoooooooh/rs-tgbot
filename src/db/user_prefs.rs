use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
//use std::fs;  //will block other user use tokio instead
use std::path::Path;
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
    async fn get(&self, user_id: i64) -> Result<UserPrefs, crate::error::AppError>;
    async fn set(&self, user_id: i64, prefs: UserPrefs) -> Result<(), crate::error::AppError>;
}

// simple JSON version
pub struct JsonUserPrefsStore {
    prefs: Mutex<HashMap<i64, UserPrefs>>, // user_id -> prefs
    file_path: String,
}

#[async_trait]
impl UserPrefsStore for JsonUserPrefsStore {
    async fn get(&self, user_id: i64) -> Result<UserPrefs, crate::error::AppError> {
        let prefs = self.prefs.lock().await;
        prefs
            .get(&user_id)
            .cloned()
            .ok_or(crate::error::AppError::UserPrefsNotFound)
    }

    async fn set(&self, user_id: i64, prefs: UserPrefs) -> Result<(), crate::error::AppError> {
        // let mut prefs_map = self.prefs.lock().await;
        // prefs_map.insert(user_id, prefs);
        // self.save_to_file(&prefs_map)

        let data_to_wtite = {
            let mut prefs_map = self.prefs.lock().await;
            prefs_map.insert(user_id, prefs);
            serde_json::to_string_pretty(&*prefs_map).map_err(|e| {
                eprintln!("Failed to serialize user prefs: {}", e);
                crate::error::AppError::UserPrefsSaveError
            })?
        };

        self.save_to_file(&data_to_wtite).await
    }
}

impl JsonUserPrefsStore {
    pub fn new(file_path: &str) -> Result<Self, crate::error::AppError> {
        let prefs = Self::load_from_file(file_path)?;
        Ok(JsonUserPrefsStore {
            prefs: Mutex::new(prefs),
            file_path: file_path.to_string(),
        })
    }

    fn load_from_file(file_path: &str) -> Result<HashMap<i64, UserPrefs>, crate::error::AppError> {
        if Path::new(file_path).exists() {
            let data = std::fs::read_to_string(file_path).map_err(|e| {
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
