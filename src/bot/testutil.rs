#![cfg(test)]

use crate::bot::telegram_client::TelegramClient;
use crate::config::LlmConfig;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};
use teloxide::types::{
    Chat, ChatId, ChatKind, ChatPrivate, ChatPublic, LinkPreviewOptions, Me, MediaKind, MediaText,
    Message, MessageCommon, MessageId, MessageKind, PublicChatKind, PublicChatSupergroup, User,
    UserId,
};

pub struct MockTelegram {
    pub me: Me,
    pub sent: Arc<Mutex<Vec<(ChatId, String)>>>,
}

impl MockTelegram {
    pub fn new(me: Me) -> Self {
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

    async fn send_text(&self, chat_id: ChatId, text: &str) -> Result<(), crate::error::AppError> {
        self.sent.lock().unwrap().push((chat_id, text.to_string()));
        Ok(())
    }
}

pub fn test_bot_me() -> Me {
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
            // removed missing fields from other code
        },
        can_join_groups: true,
        can_read_all_group_messages: false,
        supports_inline_queries: false,
        can_connect_to_business: false,
        has_main_web_app: false,
    }
}

pub fn private_chat(chat_id: i64) -> Chat {
    Chat {
        id: ChatId(chat_id),
        kind: ChatKind::Private(ChatPrivate {
            username: Some("user".into()),
            first_name: Some("U".into()),
            last_name: None,
        }),
    }
}

pub fn supergroup_chat(chat_id: i64) -> Chat {
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

pub fn text_message(chat: Chat, text: &str) -> Message {
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

pub fn empty_kind_message(chat: Chat) -> Message {
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

pub fn dummy_llm_config(url: &str) -> LlmConfig {
    LlmConfig {
        api_key: "k".into(),
        url: url.into(),
        model_name: "m".into(),
        temperature: None,
        top_p: None,
        max_completion_tokens: None,
    }
}
