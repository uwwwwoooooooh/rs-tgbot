use crate::db::history::HistoryStore;
use crate::db::user_prefs::{UserPrefs, UserPrefsStore};
use crate::services::llm::{self, LlmConfig, Message as LlmMessage, ask_llm};
use std::sync::Arc;
use teloxide::prelude::*;

#[derive(Clone)]
pub struct ChatHandler {
    pub config: Arc<LlmConfig>,
    pub prefs_store: Arc<dyn UserPrefsStore>,
    pub history_store: Arc<dyn HistoryStore>,
}
pub struct MessageExecutor {
    bot: Bot,
    msg: Message,
    deps: ChatHandler,
    user_id: i64,
    chat_id: i64,
}

impl MessageExecutor {
    pub async fn new(
        bot: Bot,
        msg: Message,
        deps: ChatHandler,
    ) -> Result<Self, crate::error::AppError> {
        let user = msg
            .from
            .as_ref()
            .ok_or(crate::error::AppError::UserInfoNotFound)?;
        let user_id = user.id.0 as i64;
        let chat_id = msg.chat.id.0;

        Ok(MessageExecutor {
            bot,
            msg,
            deps,
            user_id,
            chat_id,
        })
    }

    pub async fn execute(&self) -> Result<(), crate::error::AppError> {
        let user_text = self
            .msg
            .text()
            .ok_or(crate::error::AppError::UserTextNotFound)?;

        let me = self.bot.get_me().await?;
        let bot_username = &format!("@{}", me.username());
        let cleaned_text = user_text.replace(bot_username, "").trim().to_string();
        let is_mentioned = user_text.contains(bot_username);
        let is_private = self.msg.chat.is_private();
        let is_reply_to_bot = self
            .msg
            .reply_to_message()
            .is_some_and(|reply| reply.from.as_ref().is_some_and(|user| user.id == me.id));
        // TODO: group chat not finished. will add memory for each member

        if !is_private && !is_mentioned && !is_reply_to_bot {
            return Ok(()); // messages not relevant
        }
        println!(
            "Received message from chat {}: {}",
            self.msg.chat.id, cleaned_text
        );
        if cleaned_text.starts_with("/set ") {
            return self.handle_set(cleaned_text).await;
        } else if cleaned_text.starts_with("/reset") {
            return self.handle_reset().await;
        }

        self.handle_chat(cleaned_text).await
    }

    async fn handle_set(&self, cleaned_text: String) -> Result<(), crate::error::AppError> {
        let parts: Vec<&str> = cleaned_text.split_whitespace().collect();
        if parts.len() != 2 {
            self.bot
                .send_message(
                    self.msg.chat.id,
                    "wanna leave me but don't know how to? i won't let u go pog",
                )
                .await?;
            return Ok(());
        }
        let soul = parts[1].to_lowercase();
        let current_soul = &self
            .deps
            .prefs_store
            .get(self.chat_id, self.user_id)
            .await?
            .soul;
        if &soul == current_soul {
            self.bot
                .send_message(self.msg.chat.id, format!("I'm already {} u gym bag", soul))
                .await?;
            return Ok(());
        }

        if !llm::is_system_prompt_exists(&soul) {
            self.bot
                .send_message(self.msg.chat.id, format!("who is {}?", &soul))
                .await?;
            return Ok(());
        }

        self.deps
            .prefs_store
            .set(self.chat_id, self.user_id, UserPrefs { soul: soul.clone() })
            .await?;
        self.deps
            .history_store
            .clear_history(self.chat_id, self.user_id)
            .await?;
        self.bot
            .send_message(self.msg.chat.id, format!("I'm {} meow", soul))
            .await?;
        Ok(())
    }

    async fn handle_reset(&self) -> Result<(), crate::error::AppError> {
        self.deps
            .prefs_store
            .set(self.chat_id, self.user_id, UserPrefs::default())
            .await?;
        self.deps
            .history_store
            .clear_history(self.chat_id, self.user_id)
            .await?;
        self.bot
            .send_message(
                self.msg.chat.id,
                "Reset to default soul and cleared history.",
            )
            .await?;
        Ok(())
    }

    async fn handle_chat(&self, cleaned_text: String) -> Result<(), crate::error::AppError> {
        // Build the stateless history using LlmMessage
        let prefs = self
            .deps
            .prefs_store
            .get(self.chat_id, self.user_id)
            .await?;
        let system_prompt = crate::services::llm::load_system_prompt(prefs.soul.as_str())
        .unwrap_or_else(|err| {
            eprintln!("Error loading system prompt: {}. Using default prompt.", err);
            "You are a helpless AI assistant. Please reply in English but spell by katakana. Example: goodo morningu".to_string()
        });

        let mut prompt = vec![LlmMessage {
            role: Arc::from("system"),
            content: Arc::from(system_prompt),
        }];

        if let Ok(past_messages) = self
            .deps
            .history_store
            .get_history(self.chat_id, self.user_id)
            .await
        {
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

        self.deps
            .history_store
            .add_message(self.chat_id, self.user_id, current_user_msg)
            .await?;

        match ask_llm(&self.deps.config, prompt).await {
            Ok(reply_text) => {
                println!("Reply to chat {}: {}", self.msg.chat.id, reply_text);
                self.bot.send_message(self.msg.chat.id, &reply_text).await?;
                let assistant_msg = LlmMessage {
                    role: Arc::from("assistant"),
                    content: Arc::from(reply_text),
                };

                let _ = self
                    .deps
                    .history_store
                    .add_message(self.chat_id, self.user_id, assistant_msg)
                    .await;
            }
            Err(error) => {
                eprintln!("Failed to get response from LLM: {}", error);
                self.bot
                    .send_message(
                        self.msg.chat.id,
                        "Someone tell Vedal that there is a problem with my AI.",
                    )
                    .await?;
            }
        }

        Ok(())
    }
}

/// text message handler
pub async fn handle_text_message(
    bot: Bot,
    msg: Message,
    deps: ChatHandler,
) -> Result<(), crate::error::AppError> {
    // Handle commands
    MessageExecutor::new(bot, msg, deps).await?.execute().await
}
