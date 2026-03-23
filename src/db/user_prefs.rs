use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserPrefs {
    pub soul: String,
}

impl Default for UserPrefs {
    fn default() -> Self {
        UserPrefs {
            soul: "neuro".to_string(), // default to nanami
        }
    }
}

// TODO: postgres version
#[async_trait]
pub trait UserPrefsStore: Send + Sync {
    async fn get(&self, user_id: i64) -> UserPrefs;
    async fn set(&self, user_id: i64, prefs: UserPrefs);
}

// simple JSON version
pub struct JsonUserPrefsStore {
    prefs: Mutex<HashMap<i64, UserPrefs>>, // user_id -> prefs
    file_path: String,
}

#[async_trait]
impl UserPrefsStore for JsonUserPrefsStore {
    async fn get(&self, user_id: i64) -> UserPrefs {
        let prefs = self.prefs.lock().await;
        prefs.get(&user_id).cloned().unwrap_or_default()
    }

    async fn set(&self, user_id: i64, prefs: UserPrefs) {
        let mut prefs_map = self.prefs.lock().await;
        prefs_map.insert(user_id, prefs);
        self.save_to_file(&prefs_map);
    }
}

impl JsonUserPrefsStore {
    pub fn new(file_path: &str) -> Self {
        let prefs = Self::load_from_file(file_path);
        JsonUserPrefsStore {
            prefs: Mutex::new(prefs),
            file_path: file_path.to_string(),
        }
    }

    fn load_from_file(file_path: &str) -> HashMap<i64, UserPrefs> {
        if Path::new(file_path).exists() {
            let data = fs::read_to_string(file_path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    fn save_to_file(&self, prefs: &HashMap<i64, UserPrefs>) {
        let data = serde_json::to_string_pretty(prefs).unwrap_or_default();
        fs::write(&self.file_path, data).unwrap_or_else(|e| {
            eprintln!("Failed to save user prefs: {}", e);
        });
    }
}
