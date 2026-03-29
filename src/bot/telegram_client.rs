use async_trait::async_trait;
use teloxide::prelude::{Bot, Requester};
use teloxide::types::{ChatId, Me};

/// Telegram Bot API surface used by [`super::handlers::chat::MessageExecutor`].
/// Production uses [`TeloxideAdapter`]; tests use a mock implementation.
#[async_trait]
pub trait TelegramClient: Send + Sync {
    async fn get_me(&self) -> Result<Me, crate::error::AppError>;
    async fn send_text(&self, chat_id: ChatId, text: &str) -> Result<(), crate::error::AppError>;
}

/// Wrapper for production [`Bot`] API calls used by message handlers.
pub struct TeloxideAdapter(pub Bot);

#[async_trait]
impl TelegramClient for TeloxideAdapter {
    async fn get_me(&self) -> Result<Me, crate::error::AppError> {
        self.0.get_me().await.map_err(Into::into)
    }

    async fn send_text(&self, chat_id: ChatId, text: &str) -> Result<(), crate::error::AppError> {
        self.0
            .send_message(chat_id, text)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}
