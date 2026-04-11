use crate::bot::telegram_client::TelegramClient;
use crate::db::history::HistoryStore;
use crate::db::user_prefs::UserPrefsStore;
use crate::services::llm::{LlmConfig, Message as LlmMessage, ask_llm};
use std::sync::Arc;
use teloxide::prelude::*;

#[derive(Clone)]
pub struct ChatHandler {
    pub config: Arc<LlmConfig>,
    pub prefs_store: Arc<dyn UserPrefsStore>,
    pub history_store: Arc<dyn HistoryStore>,
}
pub struct MessageExecutor {
    bot: Arc<dyn TelegramClient>,
    msg: Message,
    deps: ChatHandler,
    user_id: i64,
    chat_id: i64,
}

impl MessageExecutor {
    pub async fn new(
        bot: Arc<dyn TelegramClient>,
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
        self.handle_chat(cleaned_text).await
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
                self.bot.send_text(self.msg.chat.id, &reply_text).await?;
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
                    .send_text(
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
    use crate::bot::telegram_client::TeloxideAdapter;

    MessageExecutor::new(Arc::new(TeloxideAdapter(bot)), msg, deps)
        .await?
        .execute()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::testutil::*;
    use crate::db::history::JsonHistoryStore;
    use crate::db::user_prefs::JsonUserPrefsStore;
    use mockito::Server;
    use std::sync::Arc;

    #[tokio::test]
    async fn execute_supergroup_without_mention_sends_nothing() {
        let mock = MockTelegram::new(test_bot_me());
        let sent = Arc::clone(&mock.sent);
        let prefs_path =
            std::env::temp_dir().join(format!("rs_tgbot_mock_prefs_{}.json", std::process::id()));
        let hist_dir =
            std::env::temp_dir().join(format!("rs_tgbot_mock_hist_{}", std::process::id()));
        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);

        let prefs_store: Arc<dyn UserPrefsStore> = Arc::new(
            JsonUserPrefsStore::new(prefs_path.to_str().unwrap())
                .await
                .unwrap(),
        );
        let history_store: Arc<dyn HistoryStore> =
            Arc::new(JsonHistoryStore::new(&hist_dir, 10).await.unwrap());
        let deps = ChatHandler {
            config: Arc::new(dummy_llm_config("http://unused.invalid")),
            prefs_store,
            history_store,
        };

        let msg = text_message(supergroup_chat(-100123), "hello");
        let ex = MessageExecutor::new(Arc::new(mock), msg, deps)
            .await
            .unwrap();
        ex.execute().await.unwrap();

        assert!(sent.lock().unwrap().is_empty());

        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);
    }

    #[tokio::test]
    async fn new_errors_when_from_missing() {
        let prefs_path = std::env::temp_dir().join(format!(
            "rs_tgbot_mock_from_prefs_{}.json",
            std::process::id()
        ));
        let hist_dir =
            std::env::temp_dir().join(format!("rs_tgbot_mock_from_hist_{}", std::process::id()));
        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);

        let prefs_store: Arc<dyn UserPrefsStore> = Arc::new(
            JsonUserPrefsStore::new(prefs_path.to_str().unwrap())
                .await
                .unwrap(),
        );
        let history_store: Arc<dyn HistoryStore> =
            Arc::new(JsonHistoryStore::new(&hist_dir, 10).await.unwrap());
        let deps = ChatHandler {
            config: Arc::new(dummy_llm_config("http://unused.invalid")),
            prefs_store,
            history_store,
        };

        let mut m = text_message(private_chat(1), "hi");
        m.from = None;
        let mock = MockTelegram::new(test_bot_me());
        let r = MessageExecutor::new(Arc::new(mock), m, deps).await;
        assert!(matches!(r, Err(crate::error::AppError::UserInfoNotFound)));

        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);
    }

    #[tokio::test]
    async fn execute_errors_when_message_has_no_text() {
        let prefs_path = std::env::temp_dir().join(format!(
            "rs_tgbot_mock_text_prefs_{}.json",
            std::process::id()
        ));
        let hist_dir =
            std::env::temp_dir().join(format!("rs_tgbot_mock_text_hist_{}", std::process::id()));
        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);

        let prefs_store: Arc<dyn UserPrefsStore> = Arc::new(
            JsonUserPrefsStore::new(prefs_path.to_str().unwrap())
                .await
                .unwrap(),
        );
        let history_store: Arc<dyn HistoryStore> =
            Arc::new(JsonHistoryStore::new(&hist_dir, 10).await.unwrap());
        let deps = ChatHandler {
            config: Arc::new(dummy_llm_config("http://unused.invalid")),
            prefs_store,
            history_store,
        };

        let mock = MockTelegram::new(test_bot_me());
        let msg = empty_kind_message(private_chat(1));
        let ex = MessageExecutor::new(Arc::new(mock), msg, deps)
            .await
            .unwrap();
        let r = ex.execute().await;
        assert!(matches!(r, Err(crate::error::AppError::UserTextNotFound)));

        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);
    }

    #[tokio::test]
    async fn execute_private_chat_uses_mock_llm_via_mockito() {
        let mut server = Server::new_async().await;
        let mock_http = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"mock reply"}}]}"#)
            .create_async()
            .await;

        let prefs_path = std::env::temp_dir().join(format!(
            "rs_tgbot_mock_llm_prefs_{}.json",
            std::process::id()
        ));
        let hist_dir =
            std::env::temp_dir().join(format!("rs_tgbot_mock_llm_hist_{}", std::process::id()));
        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);

        let prefs_store: Arc<dyn UserPrefsStore> = Arc::new(
            JsonUserPrefsStore::new(prefs_path.to_str().unwrap())
                .await
                .unwrap(),
        );
        let history_store: Arc<dyn HistoryStore> =
            Arc::new(JsonHistoryStore::new(&hist_dir, 10).await.unwrap());
        let deps = ChatHandler {
            config: Arc::new(LlmConfig {
                api_key: "k".into(),
                url: server.url() + "/v1/chat/completions",
                model_name: "m".into(),
                temperature: None,
                top_p: None,
                max_completion_tokens: None,
            }),
            prefs_store: prefs_store.clone(),
            history_store: history_store.clone(),
        };

        let mock = MockTelegram::new(test_bot_me());
        let sent = Arc::clone(&mock.sent);
        let msg = text_message(private_chat(77), "hello");
        let ex = MessageExecutor::new(Arc::new(mock), msg, deps)
            .await
            .unwrap();
        ex.execute().await.unwrap();

        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].1, "mock reply");

        let hist = history_store.get_history(77, 100).await.unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(&*hist[0].content, "hello");
        assert_eq!(&*hist[1].content, "mock reply");

        mock_http.assert_async().await;

        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);
    }
}
