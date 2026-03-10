use log::{info, error};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    max_completion_tokens: usize,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
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
    #[serde(default)]
    completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Deserialize)]
struct CompletionTokensDetails {
    #[serde(default)]
    reasoning_tokens: u32,
}

/// Call OpenAI API with session context
pub async fn call_llm_api_with_session(
    api_key: &str,
    session: &mut crate::engine::types::LLMSession,
    incremental_update: String,
    max_tokens: usize
) -> Result<(String, u32, u32), String> {
    // Add the incremental update as a new user message
    session.add_user_message(incremental_update);
    
    // For OpenAI, we include the system message in the messages array
    let mut messages: Vec<Message> = vec![
        Message {
            role: "system".to_string(),
            content: session.get_system_prompt(),
        }
    ];
    
    // Add all user/assistant messages from session
    for msg in session.get_messages_for_api() {
        messages.push(Message {
            role: msg.role,
            content: msg.content,
        });
    }
    
    let request_body = OpenAIRequest {
        model: crate::engine::provider_config::get_model("openai"),
        max_completion_tokens: max_tokens,
        messages,
        stream: false,
    };
    
    // Configure client
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    // Send request
    let response = client
        .post(OPENAI_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to OpenAI API: {}", e))?;
    
    if response.status().is_success() {
        let response_body: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenAI API response: {}", e))?;
        
        // Extract the text response
        let assistant_response = response_body.choices.first()
            .ok_or_else(|| "Empty response from OpenAI API".to_string())?
            .message.content.trim().to_string();
        
        // Update session token counts
        session.total_input_tokens += response_body.usage.prompt_tokens;
        session.total_output_tokens += response_body.usage.completion_tokens;

        // Add assistant response to session history
        session.add_assistant_response(assistant_response.clone());

        // Log token usage with session context (including thinking tokens if present)
        let reasoning_tokens = response_body.usage.completion_tokens_details
            .as_ref()
            .map(|d| d.reasoning_tokens)
            .unwrap_or(0);

        if reasoning_tokens > 0 {
            info!(
                "OpenAI API token usage - Input: {} (session total: {}), Output: {} (session total: {}), Reasoning: {}",
                response_body.usage.prompt_tokens,
                session.total_input_tokens,
                response_body.usage.completion_tokens,
                session.total_output_tokens,
                reasoning_tokens
            );
        } else {
            info!(
                "OpenAI API token usage - Input: {} (session total: {}), Output: {} (session total: {})",
                response_body.usage.prompt_tokens,
                session.total_input_tokens,
                response_body.usage.completion_tokens,
                session.total_output_tokens
            );
        }
        
        Ok((assistant_response, response_body.usage.prompt_tokens, response_body.usage.completion_tokens))
    } else {
        let error_message = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error message: {}", e))?;
        
        error!("Error from OpenAI API: {}", error_message);
        Err(format!("Error from OpenAI API: {}", error_message))
    }
}

/// Call OpenAI API (stateless version)
pub async fn call_llm_api(
    api_key: &str,
    prompt: String,
    system_prompt: &str,
    max_tokens: usize
) -> Result<(String, u32, u32), String> {
    // Configure client
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Create request with system message included in messages array
    let request_body = OpenAIRequest {
        model: crate::engine::provider_config::get_model("openai"),
        max_completion_tokens: max_tokens,
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
    
    // Send request
    let response = client
        .post(OPENAI_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to OpenAI API: {}", e))?;
    
    if response.status().is_success() {
        let response_body: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenAI API response: {}", e))?;
        
        // Log token usage (including thinking tokens if present)
        let reasoning_tokens = response_body.usage.completion_tokens_details
            .as_ref()
            .map(|d| d.reasoning_tokens)
            .unwrap_or(0);

        if reasoning_tokens > 0 {
            info!(
                "OpenAI API token usage - Input: {}, Output: {}, Reasoning: {}",
                response_body.usage.prompt_tokens, response_body.usage.completion_tokens, reasoning_tokens
            );
        } else {
            info!(
                "OpenAI API token usage - Input: {}, Output: {}",
                response_body.usage.prompt_tokens, response_body.usage.completion_tokens
            );
        }
        
        // Extract the text
        let response_text = response_body.choices.first()
            .ok_or_else(|| "Empty response from OpenAI API".to_string())
            .map(|choice| choice.message.content.trim().to_string())?;
        
        Ok((response_text, response_body.usage.prompt_tokens, response_body.usage.completion_tokens))
    } else {
        let error_message = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error message: {}", e))?;
        
        error!("Error from OpenAI API: {}", error_message);
        Err(format!("Error from OpenAI API: {}", error_message))
    }
} 