use crate::bot::telegram_client::TelegramClient;
use crate::config::LlmConfig;
use crate::domain::history::HistoryStore;
use crate::domain::user::UserPrefsStore;
use crate::services::llm::{Message as LlmMessage, ask_llm};
use std::sync::Arc;
use teloxide::prelude::*;

pub async fn handle_text_message_inner(
    bot: Arc<dyn TelegramClient>,
    msg: Message,
    config: Arc<LlmConfig>,
    prefs_store: Arc<dyn UserPrefsStore>,
    history_store: Arc<dyn HistoryStore>,
) -> Result<(), crate::error::AppError> {
    let user = msg
        .from
        .as_ref()
        .ok_or(crate::error::AppError::UserInfoNotFound)?;
    let user_id = user.id.0 as i64;
    let chat_id = msg.chat.id.0;

    let user_text = msg.text().ok_or(crate::error::AppError::UserTextNotFound)?;

    let me = bot.get_me().await?;
    let bot_username = format!("@{}", me.username());
    let cleaned_text = user_text.replace(&bot_username, "").trim().to_string();
    let is_mentioned = user_text.contains(&bot_username);
    let is_private = msg.chat.is_private();
    let is_reply_to_bot = msg
        .reply_to_message()
        .is_some_and(|reply| reply.from.as_ref().is_some_and(|user| user.id == me.id));

    // TODO: group chat not finished. will add memory for each member
    if !is_private && !is_mentioned && !is_reply_to_bot {
        return Ok(()); // messages not relevant
    }

    println!("Received message from chat {}: {}", chat_id, cleaned_text);

    // Build the stateless history using LlmMessage
    let prefs = prefs_store.get(chat_id, user_id).await?;
    let system_prompt = crate::services::llm::load_system_prompt(prefs.soul.as_str())
        .unwrap_or_else(|err| {
            eprintln!("Error loading system prompt: {}. Using default prompt.", err);
            "You are a helpless AI assistant. Please reply in English but spell by katakana. Example: goodo morningu".to_string()
        });

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
            println!("Reply to chat {}: {}", chat_id, reply_text);
            bot.send_text(msg.chat.id, &reply_text).await?;
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
            bot.send_text(
                msg.chat.id,
                "Someone tell Vedal that there is a problem with my AI.",
            )
            .await?;
        }
    }

    Ok(())
}

/// text message handler
pub async fn handle_text_message(
    bot: Bot,
    msg: Message,
    config: Arc<LlmConfig>,
    prefs_store: Arc<dyn UserPrefsStore>,
    history_store: Arc<dyn HistoryStore>,
) -> Result<(), crate::error::AppError> {
    use crate::bot::telegram_client::TeloxideAdapter;

    handle_text_message_inner(
        Arc::new(TeloxideAdapter(bot)),
        msg,
        config,
        prefs_store,
        history_store,
    )
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
        let config = Arc::new(dummy_llm_config("http://unused.invalid"));

        let msg = text_message(supergroup_chat(-100123), "hello");
        super::handle_text_message_inner(Arc::new(mock), msg, config, prefs_store, history_store)
            .await
            .unwrap();

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
        let config = Arc::new(dummy_llm_config("http://unused.invalid"));

        let mut m = text_message(private_chat(1), "hi");
        m.from = None;
        let mock = MockTelegram::new(test_bot_me());
        let r =
            super::handle_text_message_inner(Arc::new(mock), m, config, prefs_store, history_store)
                .await;
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
        let config = Arc::new(dummy_llm_config("http://unused.invalid"));

        let mock = MockTelegram::new(test_bot_me());
        let msg = empty_kind_message(private_chat(1));
        let r = super::handle_text_message_inner(
            Arc::new(mock),
            msg,
            config,
            prefs_store,
            history_store,
        )
        .await;
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
        let config = Arc::new(LlmConfig {
            api_key: "k".into(),
            url: server.url() + "/v1/chat/completions",
            model_name: "m".into(),
            temperature: None,
            top_p: None,
            max_completion_tokens: None,
        });

        let mock = MockTelegram::new(test_bot_me());
        let sent = Arc::clone(&mock.sent);
        let msg = text_message(private_chat(77), "hello");
        super::handle_text_message_inner(
            Arc::new(mock),
            msg,
            config,
            prefs_store.clone(),
            history_store.clone(),
        )
        .await
        .unwrap();

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
