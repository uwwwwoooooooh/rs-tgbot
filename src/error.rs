use teloxide::RequestError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Telegram error: {0}")]
    Telegram(#[from] RequestError),

    #[error("User info not found")]
    UserInfoNotFound,

    #[error("User text not found")]
    UserTextNotFound,

    #[error("Config error: {0}")]
    ConfigError(#[from] config::ConfigError),

    #[error("Env variable error: {0}")]
    VarError(#[from] std::env::VarError),
    #[error("User prefs load error")]
    UserPrefsLoadError,

    #[error("User prefs save error")]
    UserPrefsSaveError,

    #[error("User history load error")]
    UserHistoryLoadError,

    #[error("User history save error")]
    UserHistorySaveError,

    #[error("LLM config error: {0}")]
    LlmConfigError(String),

    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Json error: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("System prompt load error")]
    SystemPromptLoadError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_info_not_found_display() {
        assert_eq!(
            format!("{}", AppError::UserInfoNotFound),
            "User info not found"
        );
    }

    #[test]
    fn user_text_not_found_display() {
        assert_eq!(
            format!("{}", AppError::UserTextNotFound),
            "User text not found"
        );
    }

    #[test]
    fn llm_config_error_display() {
        let e = AppError::LlmConfigError("missing url".to_string());
        assert_eq!(format!("{}", e), "LLM config error: missing url");
    }
}
