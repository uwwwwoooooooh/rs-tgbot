use crate::db::history::HistoryStore;
use crate::db::user_prefs::{UserPrefs, UserPrefsStore};
use crate::services::llm::{LlmConfig, Message as LlmMessage, ask_llm};
use std::sync::Arc;
use teloxide::prelude::*;

/// text message handler
pub async fn handle_text_message(
    bot: Bot,
    msg: Message,
    config: Arc<LlmConfig>,
    prefs_store: Arc<dyn UserPrefsStore>,
    history_store: Arc<dyn HistoryStore>,
) -> Result<(), crate::error::AppError> {
    // needs to be thread-safe. Send + Sync

    let Some(user) = msg.from.as_ref() else {
        return Err(crate::error::AppError::UserInfoNotFound);
    };

    let Some(user_text) = msg.text() else {
        return Err(crate::error::AppError::UserTextNotFound);
    };

    let user_id = user.id.0 as i64;
    let chat_id = msg.chat.id.0;
    let me = bot.get_me().await?;
    let bot_username = &format!("@{}", me.username());
    let is_mentioned = user_text.contains(bot_username);
    let is_private = msg.chat.is_private();
    let is_reply_to_bot = msg
        .reply_to_message()
        .is_some_and(|reply| reply.from.as_ref().is_some_and(|user| user.id == me.id));
    // TODO: group chat not finished. will add memory for each member

    if !is_private && !is_mentioned && !is_reply_to_bot {
        return Ok(()); // messages not relevant
    }

    let cleaned_text = user_text.replace(bot_username, "").trim().to_string();
    println!(
        "Received message from chat {}: {}",
        msg.chat.id, cleaned_text
    );

    // Handle commands
    if cleaned_text.starts_with("/set ") {
        let parts: Vec<&str> = cleaned_text.split_whitespace().collect();
        if parts.len() == 2 {
            let soul = parts[1].to_lowercase();
            let current_soul = &prefs_store.get(chat_id, user_id).await?.soul;
            if &soul == current_soul {
                bot.send_message(msg.chat.id, format!("I'm already {} u gym bag", soul))
                    .await?;
                return Ok(());
            }
            if soul == "nanami" || soul == "neuro" {
                prefs_store
                    .set(chat_id, user_id, UserPrefs { soul: soul.clone() })
                    .await?;
                history_store.clear_history(chat_id, user_id).await?;
                bot.send_message(msg.chat.id, format!("I'm {} now", soul))
                    .await?;
                return Ok(());
            } else {
                bot.send_message(msg.chat.id, "only nanami or neuro")
                    .await?;
                return Ok(());
            }
        }
    } else if cleaned_text.starts_with("/reset") {
        prefs_store
            .set(chat_id, user_id, UserPrefs::default())
            .await?;
        history_store.clear_history(chat_id, user_id).await?;
        bot.send_message(msg.chat.id, "Reset to default soul and cleared history.")
            .await?;
        return Ok(());
    }

    // Build the stateless history using LlmMessage
    let prefs = prefs_store.get(chat_id, user_id).await?;
    let system_prompt = match prefs.soul.as_str() {
        "neuro" => crate::services::llm::load_system_prompt("neuro_soul.md"),
        _ => crate::services::llm::load_system_prompt("nanami_soul.md"), // default to nanami
    };

    let mut prompt = vec![LlmMessage {
        role: Arc::from("system"),
        content: Arc::from(system_prompt),
    }];

    if let Ok(past_messages) = history_store.get_history(chat_id, user_id).await {
        prompt.extend(past_messages.iter().cloned());
    }

    // prepare history
    let current_user_msg = LlmMessage {
        role: Arc::from("user"),
        content: Arc::from(cleaned_text),
    };

    // only deep copy one message
    prompt.push(current_user_msg.clone());
    println!(" prompt {:#?}", prompt);

    history_store
        .add_message(chat_id, user_id, current_user_msg)
        .await?;

    match ask_llm(&config, prompt).await {
        Ok(reply_text) => {
            println!("Reply to chat {}: {}", msg.chat.id, reply_text);
            bot.send_message(msg.chat.id, &reply_text).await?;
            let assistant_msg = LlmMessage {
                role: Arc::from("assistant"),
                content: Arc::from(reply_text),
            };

            let _ = history_store
                .add_message(chat_id, user_id, assistant_msg)
                .await;
        }
        Err(error) => {
            eprintln!("Failed to get response from LLM: {}", error);
            bot.send_message(
                msg.chat.id,
                "Someone tell Vedal that there is a problem with my AI.",
            )
            .await?;
        }
    }

    Ok(())
}
