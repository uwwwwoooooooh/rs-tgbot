use super::history::HistoryStore;
use crate::services::llm::Message as LlmMessage;
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;
// simple json version
// TODO: 1. separate files for each user
// 2. postgres version(long term plan)
#[allow(dead_code)]
pub struct JsonHistoryStore {
    history: Mutex<HashMap<String, Arc<VecDeque<LlmMessage>>>>,
    base_dir: PathBuf,
    max_history: usize,
}

#[allow(dead_code)]
impl JsonHistoryStore {
    pub async fn new(
        base_dir: impl AsRef<Path>,
        max_history: usize,
    ) -> Result<Self, crate::error::AppError> {
        let base_dir = base_dir.as_ref().to_path_buf();
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir).await.map_err(|e| {
                eprintln!("Failed to create history dir: {}", e);
                crate::error::AppError::UserHistorySaveError
            })?;
        }
        let history = Self::load_from_dir(&base_dir).await?;
        Ok(JsonHistoryStore {
            history: Mutex::new(history),
            base_dir,
            max_history,
        })
    }

    async fn load_from_dir(
        base_dir: &PathBuf,
    ) -> Result<HashMap<String, Arc<VecDeque<LlmMessage>>>, crate::error::AppError> {
        let mut history_map = HashMap::new();

        let mut entries = fs::read_dir(base_dir).await.map_err(|e| {
            eprintln!("Failed to read history dir: {}", e);
            crate::error::AppError::UserHistoryLoadError
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            eprintln!("Failed to read history dir entry: {}", e);
            crate::error::AppError::UserHistoryLoadError
        })? {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue; // skip non-json files
            }

            let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue; // skip files without valid name
            };

            let data = fs::read_to_string(&path).await.map_err(|e| {
                eprintln!("Failed to read history file {}: {}", path.display(), e);
                crate::error::AppError::UserHistoryLoadError
            })?;

            let Ok(user_history) = serde_json::from_str::<VecDeque<LlmMessage>>(&data) else {
                eprintln!(
                    "Failed to parse history file {}: Invalid format",
                    path.display()
                );
                continue; // skip invalid files
            };

            history_map.insert(file_stem.to_string(), Arc::new(user_history));
        }
        Ok(history_map)
    }

    fn get_user_file_path(&self, key: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", key))
    }

    async fn save_user_history(&self, key: &str, data: &str) -> Result<(), crate::error::AppError> {
        let file_path = self.get_user_file_path(key);
        fs::write(&file_path, data).await.map_err(|e| {
            eprintln!(
                "Failed to write history file {}: {}",
                file_path.display(),
                e
            );
            crate::error::AppError::UserHistorySaveError
        })
    }
}

#[async_trait]
#[allow(dead_code)]
impl HistoryStore for JsonHistoryStore {
    async fn add_message(
        &self,
        chat_id: i64,
        user_id: i64,
        message: LlmMessage,
    ) -> Result<(), crate::error::AppError> {
        let key = format!("{}_{}", chat_id, user_id);
        let user_history = {
            let mut history_map = self.history.lock().await;
            let user_history_arc = history_map
                .entry(key.clone())
                .or_insert_with(|| Arc::new(VecDeque::new()));

            let user_history = Arc::make_mut(user_history_arc);
            user_history.push_back(message);
            if user_history.len() > self.max_history {
                user_history.pop_front(); // remove oldest
            }
            // TODO: heavy operation, need to optimize.
            user_history.clone()
        };

        let data_to_write = serde_json::to_string_pretty(&user_history).map_err(|e| {
            eprintln!("Failed to serialize history: {}", e);
            crate::error::AppError::UserHistorySaveError
        })?;
        self.save_user_history(&key, &data_to_write).await
    }

    async fn get_history(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<Arc<VecDeque<LlmMessage>>, crate::error::AppError> {
        let key = format!("{}_{}", chat_id, user_id);
        let history = self.history.lock().await;
        Ok(history.get(&key).cloned().unwrap_or_default())
    }

    async fn clear_history(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), crate::error::AppError> {
        let key = format!("{}_{}", chat_id, user_id);
        {
            let mut history_map = self.history.lock().await;
            history_map.remove(&key);
        }

        let file_path = self.get_user_file_path(&key);
        if file_path.exists() {
            fs::remove_file(&file_path).await.map_err(|e| {
                eprintln!(
                    "Failed to delete history file {}: {}",
                    file_path.display(),
                    e
                );
                crate::error::AppError::UserHistorySaveError
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::llm::Message as LlmMessage;
    use crate::util::testutil;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn temp_history_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rs_tgbot_json_hist_{}",
            testutil::temp_path_suffix()
        ))
    }

    fn user_msg(content: &str) -> LlmMessage {
        LlmMessage {
            role: Arc::from("user"),
            content: Arc::from(content),
        }
    }

    #[tokio::test]
    async fn json_history_add_get_clear() {
        let dir = temp_history_dir();
        let store = JsonHistoryStore::new(&dir, 10).await.unwrap();

        store.add_message(5, 6, user_msg("ping")).await.unwrap();
        let hist = store.get_history(5, 6).await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(&*hist[0].content, "ping");

        store.clear_history(5, 6).await.unwrap();
        let hist = store.get_history(5, 6).await.unwrap();
        assert!(hist.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn json_history_truncates_to_max() {
        let dir = temp_history_dir();
        let store = JsonHistoryStore::new(&dir, 2).await.unwrap();

        store.add_message(1, 1, user_msg("x")).await.unwrap();
        store.add_message(1, 1, user_msg("y")).await.unwrap();
        store.add_message(1, 1, user_msg("z")).await.unwrap();

        let hist = store.get_history(1, 1).await.unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(&*hist[0].content, "y");
        assert_eq!(&*hist[1].content, "z");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
