use teloxide::prelude::*;
use teloxide::types::Me;
use std::sync::Arc;
// colliding with teloxide::prelude::Message
// LLM Message => LlmMessage
use crate::services::llm::{ask_llm, Message as LlmMessage, LlmConfig};

/// text message handler
pub async fn handle_text_message(
    bot: Bot,
    msg: Message,
    config: Arc<LlmConfig>,
    me: Me,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> { // needs to be thread-safe. Send + Sync
    
    if let Some(user_text) = msg.text() {
        let bot_username = &format!("@{}", me.username());
        let is_mentioned = user_text.contains(bot_username);
        let is_private = msg.chat.is_private();
        let is_reply_to_bot = msg.reply_to_message().map_or(false, |reply| {
            reply.from.as_ref().map_or(false, |user| user.id == me.id)
        });
        // TODO: group chat not finished. will add memory for each member

        if is_private || is_mentioned || is_reply_to_bot {
            let cleaned_text = user_text.replace(bot_username, "").trim().to_string();
            println!("Received message from chat {}: {}", msg.chat.id, cleaned_text);
        } else {
            return Ok(());
        }

        // Build the stateless history using LlmMessage
        let history = vec![
            LlmMessage {
                role: "system".to_string(),
                content: config.system_prompt.clone(),
            },
            LlmMessage {
                role: "user".to_string(),
                content: user_text.to_string(),
            }
        ];

        match ask_llm(&config, &history).await {
            Ok(reply_text) => {
                bot.send_message(msg.chat.id, reply_text).await?;
            }
            Err(error) => {
                eprintln!("Failed to get response from LLM: {}", error);
                bot.send_message(msg.chat.id, "Someone tell Vedal that there is a problem with my AI.").await?;
            }
        }
    }

    Ok(())
}