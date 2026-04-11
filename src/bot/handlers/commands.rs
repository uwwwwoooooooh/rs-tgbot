use crate::bot::telegram_client::TelegramClient;
use crate::db::history::HistoryStore;
use crate::db::user_prefs::{UserPrefs, UserPrefsStore};
use crate::services::llm;
use std::sync::Arc;
use teloxide::macros::BotCommands;
use teloxide::prelude::*;

#[derive(BotCommands, Clone, Debug)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "Set bot soul")]
    Set(String),
    #[command(description = "Reset history and preferences")]
    Reset,
}

pub async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    prefs_store: Arc<dyn UserPrefsStore>,
    history_store: Arc<dyn HistoryStore>,
) -> Result<(), crate::error::AppError> {
    use crate::bot::telegram_client::TeloxideAdapter;
    execute_command(Arc::new(TeloxideAdapter(bot)), msg, cmd, prefs_store, history_store).await
}

pub async fn execute_command(
    bot: Arc<dyn TelegramClient>,
    msg: Message,
    cmd: Command,
    prefs_store: Arc<dyn UserPrefsStore>,
    history_store: Arc<dyn HistoryStore>,
) -> Result<(), crate::error::AppError> {
    let user = msg
        .from
        .as_ref()
        .ok_or(crate::error::AppError::UserInfoNotFound)?;
    let user_id = user.id.0 as i64;
    let chat_id = msg.chat.id.0;

    match cmd {
        Command::Set(soul) => {
            let soul = soul.trim().to_string();
            // Original code expected exactly 2 parts: `[ "/set", "soul" ]`.
            // So if soul has spaces, it means `/set a b` which is "wrong arity".
            if soul.is_empty() || soul.contains(' ') {
                bot.send_text(
                    msg.chat.id,
                    "wanna leave me but don't know how to? i won't let u go pog",
                )
                .await?;
                return Ok(());
            }

            let soul = soul.to_lowercase();
            let current_soul = &prefs_store.get(chat_id, user_id).await?.soul;

            if &soul == current_soul {
                let msg_text = format!("I'm already {} u gym bag", soul);
                bot.send_text(msg.chat.id, &msg_text).await?;
                return Ok(());
            }

            if !llm::is_system_prompt_exists(&soul) {
                let msg_text = format!("who is {}?", &soul);
                bot.send_text(msg.chat.id, &msg_text).await?;
                return Ok(());
            }

            prefs_store
                .set(chat_id, user_id, UserPrefs { soul: soul.clone() })
                .await?;
            history_store.clear_history(chat_id, user_id).await?;
            let msg_text = format!("I'm {} meow", soul);
            bot.send_text(msg.chat.id, &msg_text).await?;
        }
        Command::Reset => {
            prefs_store
                .set(chat_id, user_id, UserPrefs::default())
                .await?;
            history_store.clear_history(chat_id, user_id).await?;
            bot.send_text(msg.chat.id, "Reset to default soul and cleared history.")
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::testutil::*;
    use crate::db::history::{HistoryStore, JsonHistoryStore};
    use crate::db::user_prefs::{JsonUserPrefsStore, UserPrefsStore};

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

        let msg = text_message(private_chat(55), "/reset");
        execute_command(Arc::new(mock), msg, Command::Reset, prefs_store.clone(), history_store)
            .await
            .unwrap();

        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].1, "Reset to default soul and cleared history.");
        drop(messages);

        let prefs = prefs_store.get(55, 100).await.unwrap();
        assert_eq!(prefs.soul, "neuro"); // default is neuro

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

        // Command::Set with space simulates `/set a b`
        let msg = text_message(private_chat(1), "/set a b");
        execute_command(Arc::new(mock), msg, Command::Set("a b".to_string()), prefs_store, history_store)
            .await
            .unwrap();

        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].1.contains("wanna leave me"));

        let _ = std::fs::remove_file(&prefs_path);
        let _ = std::fs::remove_dir_all(&hist_dir);
    }
}
