use crate::services::llm::Message as LlmMessage;
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Arc;

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

mod json;
mod sqlite;

#[cfg_attr(not(test), allow(unused_imports))]
// JSON backend used from unit tests / optional wiring
pub use json::JsonHistoryStore;
pub use sqlite::SqliteHistoryStore;
