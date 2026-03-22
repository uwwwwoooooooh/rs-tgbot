mod bot;
mod services;

use crate::services::llm::load_llm_config;
use dotenvy::dotenv;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // load .env
    dotenv().ok();

    let llm_config = load_llm_config().map_err(|e| {
        eprintln!("Failed to load LLM config: {}", e);
        std::process::exit(1);
    })?;

    // 1. telegram token
    let tg_token = env::var("TELEGRAM_BOT_TOKEN").map_err(|_| {
        eprintln!("TELEGRAM_BOT_TOKEN must be set in the .env file!");
        std::process::exit(1);
    })?;

    println!("Configuration loaded successfully.");

    // 4. start server
    bot::run_bot(llm_config, tg_token).await?;

    Ok(())
}

#[cfg(test)]
mod tests {}
