pub mod handlers;
pub mod telegram_client;

#[cfg(test)]
pub mod testutil;

use crate::bot::handlers::chat::ChatHandler;
use crate::bot::handlers::chat::handle_text_message;
use crate::db::history::{HistoryStore, SqliteHistoryStore};
use crate::db::sqlite::open_sqlite_pool;
use crate::db::user_prefs::{SqliteUserPrefsStore, UserPrefsStore};
use crate::services::llm::LlmConfig;
use std::sync::Arc;
use teloxide::prelude::*;

/// tg bot dispatcher
pub async fn run_bot(config: LlmConfig, bot_token: String) -> Result<(), crate::error::AppError> {
    // init bot with given token
    let bot = Bot::new(bot_token);

    let me = bot.get_me().await?;
    println!("Bot name: @{}", me.username());

    // arc for shared ownership of the config across async handlers
    let shared_config = Arc::new(config);

    let sqlite_pool = open_sqlite_pool(std::path::Path::new("data/sqlite/bot.db")).await?;

    let prefs_store: Arc<dyn UserPrefsStore> =
        Arc::new(SqliteUserPrefsStore::new(sqlite_pool.clone()).await?);

    let history_store: Arc<dyn HistoryStore> =
        Arc::new(SqliteHistoryStore::new(sqlite_pool, 10).await?);
    println!("============================");
    println!("Telegram Bot is now online");
    println!("============================");

    use crate::bot::handlers::commands::{Command, handle_command};

    // only handle text messages
    // TODO: pictures, files, etc.
    let handler = Update::filter_message()
        .branch(
            dptree::entry()
                .filter_command::<Command>()
                .endpoint(handle_command),
        )
        .branch(dptree::endpoint(handle_text_message));

    // build and start
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![ChatHandler {
            config: shared_config,
            prefs_store,
            history_store
        }])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
