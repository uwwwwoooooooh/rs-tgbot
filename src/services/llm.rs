use config::Config;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Choice {
    pub message: Message,
}

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

/// Load system prompt from file in prompts/ directory
pub fn load_system_prompt(filename: &str) -> String {
    let prompt_path = PathBuf::from("prompts").join(filename);

    fs::read_to_string(&prompt_path)
        .unwrap_or_else(|err| {
            eprintln!("Warning: Could not read {}: {}. Using default system prompt.", 
                prompt_path.display(), err);
            "You are a helpless AI assistant. Please reply in English but spell by katakana. Example: goodo morningu".to_string()
        })
}

/// Load LLM configuration from config file and env variables
pub fn load_llm_config() -> Result<LlmConfig, crate::error::AppError> {
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

/// Send entire conversation history
pub async fn ask_llm(
    config: &LlmConfig,
    prompt: Vec<Message>,
) -> Result<String, crate::error::AppError> {
    let client = Client::new();

    let request_body = ChatRequest {
        model: &config.model_name,
        messages: prompt,
        temperature: config.temperature,
        top_p: config.top_p,
        max_completion_tokens: config.max_completion_tokens,
    };

    // HTTP POST request with URL and API key
    let response = client
        .post(&config.url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    let raw_text = response.text().await?;

    // parse response
    let parsed_response = serde_json::from_str::<ChatResponse>(&raw_text)?;
    // first() changed to into_iter, take the ownership to avoid deep copy
    let Some(choice) = parsed_response.choices.into_iter().next() else {
        return Ok("Error: The API replied successfully, but gave no content.".to_string());
    };

    let mut final_answer = choice.message.content;

    // Clean up <tool_call> block
    if let Some(end_index) = final_answer.find("</think>") {
        final_answer = final_answer[end_index + 8..].trim().to_string();
    }
    Ok(final_answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use std::env;

    #[tokio::test]
    async fn test_ask_llm_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"choices":[{"message":{"role":"assistant","content":"Hello, world!"}}]}"#,
            )
            .create_async()
            .await;

        let config = LlmConfig {
            api_key: "test_key".to_string(),
            url: server.url() + "/v1/chat/completions",
            model_name: "test-model".to_string(),
            temperature: Some(0.5),
            top_p: Some(0.9),
            max_completion_tokens: Some(100),
        };

        let history = vec![
            Message {
                role: "system".to_string(),
                content: "System prompt".to_string(),
            },
            Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            },
        ];

        let result = ask_llm(&config, history).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, world!");

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ask_llm_with_think_block() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"<think>Thinking...</think>Final answer"}}]}"#)
            .create_async()
            .await;

        let config = LlmConfig {
            api_key: "test_key".to_string(),
            url: server.url() + "/v1/chat/completions",
            model_name: "test-model".to_string(),
            temperature: None,
            top_p: None,
            max_completion_tokens: None,
        };

        let history = vec![Message {
            role: "user".to_string(),
            content: "Test".to_string(),
        }];

        let result = ask_llm(&config, history).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Final answer");

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ask_llm_error_response() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(400)
            .with_body("Bad Request")
            .create_async()
            .await;

        let config = LlmConfig {
            api_key: "test_key".to_string(),
            url: server.url() + "/v1/chat/completions",
            model_name: "test-model".to_string(),
            temperature: None,
            top_p: None,
            max_completion_tokens: None,
        };

        let history = vec![Message {
            role: "user".to_string(),
            content: "Test".to_string(),
        }];

        let result = ask_llm(&config, history).await;
        assert!(result.is_err());

        mock.assert_async().await;
    }

    #[test]
    fn test_load_llm_config_with_env_vars() {
        // Set API key (required for config loading)
        unsafe {
            env::set_var("LLM_API_KEY", "test_api_key");
        }

        let config = load_llm_config().unwrap();

        // Check that defaults from config/default.toml are loaded
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

        // Verify expected defaults from config/default.toml
        assert_eq!(config.url, "https://api.minimax.io/v1/chat/completions");
        assert_eq!(config.model_name, "MiniMax-M2.7");
        // Temperature and other values come from config/default.toml
        assert!(config.temperature.is_some());
        assert!(config.top_p.is_some());
        assert!(config.max_completion_tokens.is_some());
    }

    #[test]
    #[ignore] // This test would panic if LLM_API_KEY is not set.
    // Skipped in automated tests due to environment variable persistence.
    fn test_load_llm_config_missing_api_key() {
        // Note: This test would panic if LLM_API_KEY is not set.
        // Skipped in automated tests due to environment variable persistence.
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
        assert_eq!(validate_temperature(Some(-1.0)), None); // Out of range
        assert_eq!(validate_temperature(Some(3.0)), None); // Out of range
        assert_eq!(validate_temperature(None), None);
    }

    #[test]
    fn test_validate_max_tokens() {
        assert_eq!(validate_max_tokens(Some(100)), Some(100));
        assert_eq!(validate_max_tokens(Some(1)), Some(1));
        assert_eq!(validate_max_tokens(Some(0)), None); // Not positive
        assert_eq!(validate_max_tokens(None), None);
    }
}
