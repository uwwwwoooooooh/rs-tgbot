use crate::services::llm::Message as LlmMessage;
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;
#[async_trait]
pub trait HistoryStore: Send + Sync {
    async fn add_message(
        &self,
        chat_id: i64,
        user_id: i64,
        message: LlmMessage,
    ) -> Result<(), crate::error::AppError>;
    async fn get_history(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<Arc<VecDeque<LlmMessage>>, crate::error::AppError>;
    async fn clear_history(&self, chat_id: i64, user_id: i64)
    -> Result<(), crate::error::AppError>;
}

// simple json version
pub struct JsonHistoryStore {
    history: Mutex<HashMap<String, Arc<VecDeque<LlmMessage>>>>, // user_id -> message history
    file_path: String,
    max_history: usize,
}

impl JsonHistoryStore {
    pub async fn new(file_path: &str, max_history: usize) -> Result<Self, crate::error::AppError> {
        let history = Self::load_from_file(file_path).await?;
        Ok(JsonHistoryStore {
            history: Mutex::new(history),
            file_path: file_path.to_string(),
            max_history,
        })
    }

    async fn load_from_file(
        file_path: &str,
    ) -> Result<HashMap<String, Arc<VecDeque<LlmMessage>>>, crate::error::AppError> {
        if std::path::Path::new(file_path).exists() {
            let data = fs::read_to_string(file_path).await.map_err(|e| {
                eprintln!("Failed to read history file: {}", e);
                crate::error::AppError::UserHistoryLoadError
            })?;
            serde_json::from_str(&data).map_err(|_| crate::error::AppError::UserHistoryLoadError)
        } else {
            Ok(HashMap::new())
        }
    }

    async fn save_to_file(&self, data: &str) -> Result<(), crate::error::AppError> {
        fs::write(&self.file_path, data).await.map_err(|e| {
            eprintln!("Failed to write history file: {}", e);
            crate::error::AppError::UserHistorySaveError
        })
    }
}

#[async_trait]
impl HistoryStore for JsonHistoryStore {
    async fn add_message(
        &self,
        chat_id: i64,
        user_id: i64,
        message: LlmMessage,
    ) -> Result<(), crate::error::AppError> {
        let history_map = {
            let key = format!("{}_{}", chat_id, user_id);
            let mut history_map = self.history.lock().await;
            let user_history_arc = history_map
                .entry(key)
                .or_insert_with(|| Arc::new(VecDeque::new()));

            let user_history = Arc::make_mut(user_history_arc);
            user_history.push_back(message);
            if user_history.len() > self.max_history {
                user_history.pop_front(); // remove oldest
            }
            history_map.clone()
        };

        let data_to_write = serde_json::to_string_pretty(&history_map).map_err(|e| {
            eprintln!("Failed to serialize history: {}", e);
            crate::error::AppError::UserHistorySaveError
        })?;
        self.save_to_file(&data_to_write).await
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
        let history_map = {
            let key = format!("{}_{}", chat_id, user_id);
            let mut history_map = self.history.lock().await;
            history_map.remove(&key);
            history_map.clone()
        };
        let data_to_write = serde_json::to_string_pretty(&history_map).map_err(|e| {
            eprintln!("Failed to serialize history: {}", e);
            crate::error::AppError::UserHistorySaveError
        })?;
        self.save_to_file(&data_to_write).await
    }
}
