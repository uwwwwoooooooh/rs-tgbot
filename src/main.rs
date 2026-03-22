mod bot;
mod services;

use dotenvy::dotenv;
use std::env;
use crate::services::llm::{load_llm_config};

#[tokio::main]
async fn main() {
    // load .env
    dotenv().ok();

    let llm_config = load_llm_config();

    // 1. telegram token
    let tg_token = env::var("TELEGRAM_BOT_TOKEN")
        .expect("TELEGRAM_BOT_TOKEN must be set in the .env file!");

    println!("Configuration loaded successfully.");

    // 4. start server
    bot::run_bot(llm_config, tg_token).await;
}

#[cfg(test)]
mod tests {
    
}