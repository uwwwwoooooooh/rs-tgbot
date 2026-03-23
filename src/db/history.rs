
pub trait HistoryStore: Send + Sync {
    async fn add_message(&self, user_id: i64, message: String);
    async fn get_history(&self, user_id: i64) -> Vec<String>;
    async fn clear_history(&self, user_id: i64);
}