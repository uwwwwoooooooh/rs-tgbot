use crate::bot::telegram_client::TelegramClient;
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
                .send_text(
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
            let msg = format!("I'm already {} u gym bag", soul);
            self.bot.send_text(self.msg.chat.id, &msg).await?;
            return Ok(());
        }

        if !llm::is_system_prompt_exists(&soul) {
            let msg = format!("who is {}?", &soul);
            self.bot.send_text(self.msg.chat.id, &msg).await?;
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
        let msg = format!("I'm {} meow", soul);
        self.bot.send_text(self.msg.chat.id, &msg).await?;
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
            .send_text(
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
    use crate::bot::telegram_client::TelegramClient;
    use crate::db::history_json::JsonHistoryStore;
    use crate::db::user_prefs::JsonUserPrefsStore;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use mockito::Server;
    use std::sync::{Arc, Mutex};
    use teloxide::types::{
        Chat, ChatId, ChatKind, ChatPrivate, ChatPublic, LinkPreviewOptions, Me, MediaKind,
        MediaText, Message, MessageCommon, MessageId, MessageKind, PublicChatKind,
        PublicChatSupergroup, User, UserId,
    };

    struct MockTelegram {
        me: Me,
        sent: Arc<Mutex<Vec<(ChatId, String)>>>,
    }

    impl MockTelegram {
        fn new(me: Me) -> Self {
            MockTelegram {
                me,
                sent: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl TelegramClient for MockTelegram {
        async fn get_me(&self) -> Result<Me, crate::error::AppError> {
            Ok(self.me.clone())
        }

        async fn send_text(
            &self,
            chat_id: ChatId,
            text: &str,
        ) -> Result<(), crate::error::AppError> {
            self.sent.lock().unwrap().push((chat_id, text.to_string()));
            Ok(())
        }
    }

    fn test_bot_me() -> Me {
        Me {
            user: User {
                id: UserId(999),
                is_bot: true,
                first_name: "B".into(),
                last_name: None,
                username: Some("TestBot".into()),
                language_code: None,
                is_premium: false,
                added_to_attachment_menu: false,
            },
            can_join_groups: true,
            can_read_all_group_messages: false,
            supports_inline_queries: false,
            can_connect_to_business: false,
            has_main_web_app: false,
        }
    }

    fn private_chat(chat_id: i64) -> Chat {
        Chat {
            id: ChatId(chat_id),
            kind: ChatKind::Private(ChatPrivate {
                username: Some("user".into()),
                first_name: Some("U".into()),
                last_name: None,
            }),
        }
    }

    fn supergroup_chat(chat_id: i64) -> Chat {
        Chat {
            id: ChatId(chat_id),
            kind: ChatKind::Public(ChatPublic {
                title: Some("Group".into()),
                kind: PublicChatKind::Supergroup(PublicChatSupergroup {
                    username: None,
                    is_forum: false,
                }),
            }),
        }
    }

    fn text_message(chat: Chat, text: &str) -> Message {
        let date = DateTime::from_timestamp(1_569_518_829, 0).unwrap();
        Message {
            via_bot: None,
            id: MessageId(1),
            thread_id: None,
            from: Some(User {
                id: UserId(100),
                is_bot: false,
                first_name: "U".into(),
                last_name: None,
                username: Some("u1".into()),
                language_code: None,
                is_premium: false,
                added_to_attachment_menu: false,
            }),
            sender_chat: None,
            is_topic_message: false,
            sender_business_bot: None,
            date,
            chat,
            kind: MessageKind::Common(MessageCommon {
                reply_to_message: None,
                forward_origin: None,
                external_reply: None,
                quote: None,
                edit_date: None,
                media_kind: MediaKind::Text(MediaText {
                    text: text.to_string(),
                    entities: vec![],
                    link_preview_options: Some(LinkPreviewOptions {
                        is_disabled: true,
                        url: None,
                        prefer_small_media: false,
                        prefer_large_media: false,
                        show_above_text: false,
                    }),
                }),
                reply_markup: None,
                author_signature: None,
                paid_star_count: None,
                effect_id: None,
                is_automatic_forward: false,
                has_protected_content: false,
                reply_to_story: None,
                sender_boost_count: None,
                is_from_offline: false,
                business_connection_id: None,
            }),
        }
    }

    fn empty_kind_message(chat: Chat) -> Message {
        let date = Utc::now();
        Message {
            via_bot: None,
            id: MessageId(2),
            thread_id: None,
            from: Some(User {
                id: UserId(100),
                is_bot: false,
                first_name: "U".into(),
                last_name: None,
                username: None,
                language_code: None,
                is_premium: false,
                added_to_attachment_menu: false,
            }),
            sender_chat: None,
            is_topic_message: false,
            sender_business_bot: None,
            date,
            chat,
            kind: MessageKind::Empty {},
        }
    }

    fn dummy_llm_config(url: &str) -> LlmConfig {
        LlmConfig {
            api_key: "k".into(),
            url: url.into(),
            model_name: "m".into(),
            temperature: None,
            top_p: None,
            max_completion_tokens: None,
        }
    }

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
    async fn execute_reset_sends_confirmation_and_defaults_prefs() {
        let mock = MockTelegram::new(test_bot_me());
        let sent = Arc::clone(&mock.sent);
        let prefs_path = std::env::temp_dir().join(format!(
            "rs_tgbot_mock_reset_prefs_{}.json",
            std::process::id()
        ));
        let hist_dir =
            std::env::temp_dir().join(format!("rs_tgbot_mock_reset_hist_{}", std::process::id()));
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
            prefs_store: prefs_store.clone(),
            history_store,
        };

        let msg = text_message(private_chat(55), "/reset");
        let ex = MessageExecutor::new(Arc::new(mock), msg, deps)
            .await
            .unwrap();
        ex.execute().await.unwrap();

        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].1, "Reset to default soul and cleared history.");
        drop(messages);

        let prefs = prefs_store.get(55, 100).await.unwrap();
        assert_eq!(prefs.soul, "neuro");

        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);
    }

    #[tokio::test]
    async fn execute_set_wrong_arity_sends_snark() {
        let mock = MockTelegram::new(test_bot_me());
        let sent = Arc::clone(&mock.sent);
        let prefs_path = std::env::temp_dir().join(format!(
            "rs_tgbot_mock_set_prefs_{}.json",
            std::process::id()
        ));
        let hist_dir =
            std::env::temp_dir().join(format!("rs_tgbot_mock_set_hist_{}", std::process::id()));
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

        // Must match `starts_with("/set ")`; `/set` alone is treated as normal chat text.
        let msg = text_message(private_chat(1), "/set a b");
        let ex = MessageExecutor::new(Arc::new(mock), msg, deps)
            .await
            .unwrap();
        ex.execute().await.unwrap();

        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].1.contains("wanna leave me"));

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
