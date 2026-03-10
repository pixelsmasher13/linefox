use log::{info, error};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEEPSEEK_URL: &str = "https://api.deepseek.com/v1/chat/completions";

#[derive(Serialize)]
struct DeepSeekRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct DeepSeekResponse {
    choices: Vec<Choice>,
    usage: Usage,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageResponse,
}

#[derive(Deserialize)]
struct MessageResponse {
    content: String,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

/// Call DeepSeek API with session context
pub async fn call_llm_api_with_session(
    api_key: &str,
    session: &mut crate::engine::types::LLMSession,
    incremental_update: String,
    max_tokens: usize
) -> Result<(String, u32, u32), String> {
    session.add_user_message(incremental_update);

    let mut messages: Vec<Message> = Vec::new();
    messages.push(Message {
        role: "system".to_string(),
        content: session.get_system_prompt(),
    });
    for msg in session.get_messages_for_api() {
        messages.push(Message {
            role: msg.role,
            content: msg.content,
        });
    }

    let request_body = DeepSeekRequest {
        model: crate::engine::provider_config::get_model("deepseek"),
        max_tokens,
        messages,
        stream: false,
    };

    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .post(DEEPSEEK_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to DeepSeek API: {}", e))?;

    if response.status().is_success() {
        let response_body: DeepSeekResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse DeepSeek API response: {}", e))?;

        let assistant_response = response_body.choices.first()
            .ok_or_else(|| "Empty response from DeepSeek API".to_string())?
            .message.content.trim().to_string();

        session.total_input_tokens += response_body.usage.prompt_tokens;
        session.total_output_tokens += response_body.usage.completion_tokens;

        session.messages.push(crate::engine::types::Message {
            role: "assistant".to_string(),
            content: assistant_response.clone(),
        });

        info!(
            "DeepSeek API token usage - Input: {} (session total: {}), Output: {} (session total: {})",
            response_body.usage.prompt_tokens,
            session.total_input_tokens,
            response_body.usage.completion_tokens,
            session.total_output_tokens
        );

        Ok((assistant_response, response_body.usage.prompt_tokens, response_body.usage.completion_tokens))
    } else {
        let error_message = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error message: {}", e))?;
        error!("Error from DeepSeek API: {}", error_message);
        Err(format!("Error from DeepSeek API: {}", error_message))
    }
}

/// Call DeepSeek API (stateless version)
pub async fn call_llm_api(
    api_key: &str,
    prompt: String,
    system_prompt: &str,
    max_tokens: usize
) -> Result<(String, u32, u32), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let request_body = DeepSeekRequest {
        model: crate::engine::provider_config::get_model("deepseek"),
        max_tokens,
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: prompt,
            }
        ],
        stream: false,
    };

    let response = client
        .post(DEEPSEEK_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to DeepSeek API: {}", e))?;

    if response.status().is_success() {
        let response_body: DeepSeekResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse DeepSeek API response: {}", e))?;

        info!(
            "DeepSeek API token usage - Input: {}, Output: {}",
            response_body.usage.prompt_tokens, response_body.usage.completion_tokens
        );

        let response_text = response_body.choices.first()
            .ok_or_else(|| "Empty response from DeepSeek API".to_string())
            .map(|choice| choice.message.content.trim().to_string())?;

        Ok((response_text, response_body.usage.prompt_tokens, response_body.usage.completion_tokens))
    } else {
        let error_message = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error message: {}", e))?;
        error!("Error from DeepSeek API: {}", error_message);
        Err(format!("Error from DeepSeek API: {}", error_message))
    }
}
