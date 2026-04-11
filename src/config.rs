use config::Config;
use serde::Deserialize;
use std::env;

/// all config needed to communicate with the LLM provider.
#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    pub api_key: String,
    pub url: String,
    #[serde(rename = "model")]
    pub model_name: String,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_completion_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub telegram_bot_token: String,
    pub llm: LlmConfig,
}

impl AppConfig {
    pub fn load() -> Result<Self, crate::error::AppError> {
        // load .env
        dotenvy::dotenv().ok();

        // 1. telegram token
        let telegram_bot_token = env::var("TELEGRAM_BOT_TOKEN").map_err(|_| {
            crate::error::AppError::ConfigError(config::ConfigError::Message(
                "TELEGRAM_BOT_TOKEN must be set in the .env file!".into(),
            ))
        })?;

        let llm = load_llm_config()?;

        Ok(Self {
            telegram_bot_token,
            llm,
        })
    }
}

/// temperature within (0.0 - 2.0)
fn validate_temperature(temp: Option<f32>) -> Option<f32> {
    temp.and_then(|t| {
        if (0.0..=2.0).contains(&t) {
            Some(t)
        } else {
            None
        }
    })
}

/// max_completion_tokens must be positive
fn validate_max_tokens(tokens: Option<u32>) -> Option<u32> {
    tokens.and_then(|t| if t > 0 { Some(t) } else { None })
}

/// Load LLM configuration from config file and env variables
fn load_llm_config() -> Result<LlmConfig, crate::error::AppError> {
    // Define config structure
    #[derive(Deserialize)]
    struct LlmConfigFile {
        url: Option<String>,
        model: Option<String>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_completion_tokens: Option<u32>,
    }

    // Load from default config file (config/default.toml)
    let config = Config::builder()
        .add_source(config::File::with_name("config/default.toml"))
        .add_source(config::Environment::with_prefix("LLM"))
        .build()?;

    // Extract llm section and convert to struct
    let llm_file: LlmConfigFile = config.get::<LlmConfigFile>("llm")?;

    let url = llm_file.url.ok_or(crate::error::AppError::LlmConfigError(
        "LLM URL is missing in config".to_string(),
    ))?;

    let model_name = llm_file
        .model
        .ok_or(crate::error::AppError::LlmConfigError(
            "LLM model name is missing in config".to_string(),
        ))?;

    // API key must be set
    let api_key = env::var("LLM_API_KEY")?;

    let temperature = validate_temperature(llm_file.temperature);
    let top_p = validate_temperature(llm_file.top_p);
    let max_completion_tokens = validate_max_tokens(llm_file.max_completion_tokens);

    Ok(LlmConfig {
        api_key,
        url,
        model_name,
        temperature,
        top_p,
        max_completion_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_load_llm_config_with_env_vars() {
        unsafe {
            env::set_var("LLM_API_KEY", "test_api_key");
        }

        let config = load_llm_config().unwrap();

        assert_eq!(config.api_key, "test_api_key");
        assert!(!config.url.is_empty());
        assert!(!config.model_name.is_empty());
    }

    #[test]
    fn test_load_llm_config_defaults() {
        unsafe {
            env::set_var("LLM_API_KEY", "test_api_key");
        }

        let config = load_llm_config().unwrap();

        assert_eq!(config.url, "https://api.minimax.io/v1/chat/completions");
        assert_eq!(config.model_name, "MiniMax-M2.7");
        assert!(config.temperature.is_some());
        assert!(config.top_p.is_some());
        assert!(config.max_completion_tokens.is_some());
    }

    #[test]
    #[ignore]
    fn test_load_llm_config_missing_api_key() {
        unsafe {
            env::remove_var("LLM_API_KEY");
        }
        let result = load_llm_config();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_temperature() {
        assert_eq!(validate_temperature(Some(0.5)), Some(0.5));
        assert_eq!(validate_temperature(Some(0.0)), Some(0.0));
        assert_eq!(validate_temperature(Some(2.0)), Some(2.0));
        assert_eq!(validate_temperature(Some(-1.0)), None);
        assert_eq!(validate_temperature(Some(3.0)), None);
        assert_eq!(validate_temperature(None), None);
    }

    #[test]
    fn test_validate_max_tokens() {
        assert_eq!(validate_max_tokens(Some(100)), Some(100));
        assert_eq!(validate_max_tokens(Some(1)), Some(1));
        assert_eq!(validate_max_tokens(Some(0)), None);
        assert_eq!(validate_max_tokens(None), None);
    }
}
