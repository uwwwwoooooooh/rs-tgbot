pub mod handlers;

use teloxide::prelude::*;
use std::sync::Arc;
use crate::services::llm::LlmConfig;
use crate::bot::handlers::chat::handle_text_message;

/// tg bot dispatcher
pub async fn run_bot(config: LlmConfig, bot_token: String) {
    // init bot with given token
    let bot = Bot::new(bot_token);
    
    let me = bot.get_me().await.expect("Failed to get bot info");
    println!("Bot name: @{}", me.username());

    // arc for shared ownership of the config across async handlers
    let shared_config = Arc::new(config);

    println!("============================");
    println!("Telegram Bot is now online");
    println!("============================");

    // only handle text messages
    // TODO: pictures, files, etc.
    let handler = Update::filter_message()
        .endpoint(handle_text_message);
    
    // build and start
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![shared_config, me]) 
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

}