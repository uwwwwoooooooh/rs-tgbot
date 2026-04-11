mod bot;
pub mod config;
mod db;
mod error;
mod services;
mod util;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app_config = config::AppConfig::load().map_err(|e| {
        eprintln!("Failed to load config: {}", e);
        std::process::exit(1);
    })?;

    println!("Configuration loaded successfully.");

    // start server
    bot::run_bot(app_config.llm, app_config.telegram_bot_token).await?;

    Ok(())
}
