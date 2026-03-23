pub mod handlers;

use crate::bot::handlers::chat::handle_text_message;
use crate::services::llm::LlmConfig;
use crate::services::user_prefs::UserPrefsStore;
use std::sync::Arc;
use teloxide::prelude::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BotRunningError {
    #[error("Telegram bot error: {0}")]
    Telegram(#[from] teloxide::RequestError),
}

/// tg bot dispatcher
pub async fn run_bot(config: LlmConfig, bot_token: String) -> Result<(), BotRunningError> {
    // init bot with given token
    let bot = Bot::new(bot_token);

    let me = bot.get_me().await?;
    println!("Bot name: @{}", me.username());

    // arc for shared ownership of the config across async handlers
    let shared_config = Arc::new(config);

    // user prefs store
    let prefs_store = Arc::new(UserPrefsStore::new("user_prefs.json"));

    println!("============================");
    println!("Telegram Bot is now online");
    println!("============================");

    // only handle text messages
    // TODO: pictures, files, etc.
    let handler = Update::filter_message().endpoint(handle_text_message);

    // build and start
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![shared_config, prefs_store])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
