use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod json;
mod sqlite;

#[cfg_attr(not(test), allow(unused_imports))]
// JSON backend used from unit tests / optional wiring
pub use json::JsonUserPrefsStore;
pub use sqlite::SqliteUserPrefsStore;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserPrefs {
    pub soul: String,
}

impl Default for UserPrefs {
    fn default() -> Self {
        UserPrefs {
            soul: "neuro".to_string(),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_prefs_default_matches_expected() {
        let p = UserPrefs::default();
        assert_eq!(p.soul, "neuro");
    }
}
