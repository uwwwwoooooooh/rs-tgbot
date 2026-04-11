use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{fs, path::PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: Arc<str>,
    pub content: Arc<str>,
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

pub fn is_system_prompt_exists(filename: &str) -> bool {
    let prompt_path = PathBuf::from("prompts/soul")
        .join(filename)
        .with_extension("md");
    prompt_path.exists()
}

pub fn load_system_prompt(filename: &str) -> Result<String, crate::error::AppError> {
    let prompt_path = PathBuf::from("prompts/soul")
        .join(filename)
        .with_extension("md");

    fs::read_to_string(&prompt_path).map_err(|err| {
        eprintln!(
            "Warning: Could not read {}: {}. Using default system prompt.",
            prompt_path.display(),
            err
        );
        crate::error::AppError::SystemPromptLoadError
    })
}

pub async fn ask_llm(
    config: &crate::config::LlmConfig,
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

    let response = client
        .post(&config.url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    let raw_text = response.text().await?;

    let parsed_response = serde_json::from_str::<ChatResponse>(&raw_text)?;
    let Some(choice) = parsed_response.choices.into_iter().next() else {
        return Ok("Error: The API replied successfully, but gave no content.".to_string());
    };

    let content_str = &choice.message.content;

    // Clean up <think> block
    let final_answer = if let Some(end_index) = content_str.find("</think>") {
        content_str[end_index + 8..].trim().to_string()
    } else {
        content_str.trim().to_string()
    };
    Ok(final_answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

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

        let config = crate::config::LlmConfig {
            api_key: "test_key".to_string(),
            url: server.url() + "/v1/chat/completions",
            model_name: "test-model".to_string(),
            temperature: Some(0.5),
            top_p: Some(0.9),
            max_completion_tokens: Some(100),
        };

        let history = vec![
            Message {
                role: Arc::from("system"),
                content: Arc::from("System prompt"),
            },
            Message {
                role: Arc::from("user"),
                content: Arc::from("Hello"),
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

        let config = crate::config::LlmConfig {
            api_key: "test_key".to_string(),
            url: server.url() + "/v1/chat/completions",
            model_name: "test-model".to_string(),
            temperature: None,
            top_p: None,
            max_completion_tokens: None,
        };

        let history = vec![Message {
            role: Arc::from("user"),
            content: Arc::from("Test"),
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

        let config = crate::config::LlmConfig {
            api_key: "test_key".to_string(),
            url: server.url() + "/v1/chat/completions",
            model_name: "test-model".to_string(),
            temperature: None,
            top_p: None,
            max_completion_tokens: None,
        };

        let history = vec![Message {
            role: Arc::from("user"),
            content: Arc::from("Test"),
        }];

        let result = ask_llm(&config, history).await;
        assert!(result.is_err());

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_ask_llm_empty_choices() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices":[]}"#)
            .create_async()
            .await;

        let config = crate::config::LlmConfig {
            api_key: "test_key".to_string(),
            url: server.url() + "/v1/chat/completions",
            model_name: "test-model".to_string(),
            temperature: None,
            top_p: None,
            max_completion_tokens: None,
        };

        let history = vec![Message {
            role: Arc::from("user"),
            content: Arc::from("Test"),
        }];

        let result = ask_llm(&config, history).await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "Error: The API replied successfully, but gave no content."
        );

        mock.assert_async().await;
    }

    #[test]
    fn message_serde_roundtrip() {
        let m = Message {
            role: Arc::from("user"),
            content: Arc::from("hi"),
        };
        let j = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&j).unwrap();
        assert_eq!(&*back.role, "user");
        assert_eq!(&*back.content, "hi");
    }

    #[test]
    fn chat_request_skips_none_optional_fields() {
        let req = ChatRequest {
            model: "m",
            messages: vec![],
            temperature: None,
            top_p: None,
            max_completion_tokens: None,
        };
        let j = serde_json::to_string(&req).unwrap();
        assert!(!j.contains("temperature"));
    }
}
