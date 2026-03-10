use log::{info, error};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";

#[derive(Serialize)]
struct LLMRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<Message>,
    system: String,
    stream: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct LLMResponse {
    content: Vec<Content>,
    usage: Usage,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize)]
struct Content {
    text: String,
}

/// Call Claude API with session context
pub async fn call_llm_api_with_session(
    api_key: &str,
    session: &mut crate::engine::types::LLMSession,
    incremental_update: String,
    max_tokens: usize
) -> Result<(String, u32, u32), String> {
    // Add the incremental update as a new user message
    session.add_user_message(incremental_update);
    
    // Create request with full conversation history
    let request_body = LLMRequest {
        model: crate::engine::provider_config::get_model("claude"),
        max_tokens,
        messages: session.get_messages_for_api().into_iter().map(|msg| Message {
            role: msg.role,
            content: msg.content,
        }).collect(),
        system: session.get_system_prompt(),
        stream: false,
    };
    
    // Configure client
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    // Send request
    let response = client
        .post(ANTHROPIC_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;
    
    if response.status().is_success() {
        let response_body: LLMResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Claude API response: {}", e))?;
        
        // Extract the text response
        let assistant_response = response_body.content.first()
            .ok_or_else(|| "Empty response from Claude API".to_string())?
            .text.trim().to_string();
        
        // Update session token counts
        session.total_input_tokens += response_body.usage.input_tokens;
        session.total_output_tokens += response_body.usage.output_tokens;
        
        // Add assistant response to session history
        session.add_assistant_response(assistant_response.clone());
        
        // Log token usage with session context
        info!(
            "Claude API token usage - Input: {} (session total: {}), Output: {} (session total: {})",
            response_body.usage.input_tokens, 
            session.total_input_tokens,
            response_body.usage.output_tokens,
            session.total_output_tokens
        );
        
        Ok((assistant_response, response_body.usage.input_tokens, response_body.usage.output_tokens))
    } else {
        let error_message = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error message: {}", e))?;
        
        error!("Error from Claude API: {}", error_message);
        Err(format!("Error from Claude API: {}", error_message))
    }
}

/// Call Claude API (stateless version)
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

    // Create request
    let request_body = LLMRequest {
        model: crate::engine::provider_config::get_model("claude"),
        max_tokens,
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt,
        }],
        system: system_prompt.to_string(),
        stream: false,
    };
    
    // Send request
    let response = client
        .post(ANTHROPIC_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;
    
    if response.status().is_success() {
        let response_body: LLMResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Claude API response: {}", e))?;
        
        // Log token usage
        info!(
            "Claude API token usage - Input: {}, Output: {}",
            response_body.usage.input_tokens, response_body.usage.output_tokens
        );
        
        // Extract the text
        let response_text = response_body.content.first()
            .ok_or_else(|| "Empty response from Claude API".to_string())
            .map(|content| content.text.trim().to_string())?;
        
        Ok((response_text, response_body.usage.input_tokens, response_body.usage.output_tokens))
    } else {
        let error_message = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error message: {}", e))?;
        
        error!("Error from Claude API: {}", error_message);
        Err(format!("Error from Claude API: {}", error_message))
    }
} 