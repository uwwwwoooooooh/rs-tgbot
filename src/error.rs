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

    #[error("User prefs not found")]
    UserPrefsNotFound,

    #[error("User prefs load error")]
    UserPrefsLoadError,

    #[error("User prefs save error")]
    UserPrefsSaveError,

    #[error("LLM config error: {0}")]
    LlmConfigError(String),
    
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Json error: {0}")]
    JsonParse(#[from] serde_json::Error),
}
