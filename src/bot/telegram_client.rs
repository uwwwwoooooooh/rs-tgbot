use async_trait::async_trait;
use teloxide::prelude::{Bot, Requester};
use teloxide::types::{ChatId, Me};

/// Abstract Telegram client for testing and mocking
#[async_trait]
pub trait TelegramClient: Send + Sync {
    async fn get_me(&self) -> Result<Me, crate::error::AppError>;
    async fn send_text(&self, chat_id: ChatId, text: &str) -> Result<(), crate::error::AppError>;
}

/// Production implementation of TelegramClient
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
