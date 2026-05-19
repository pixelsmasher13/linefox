use log::{info, error};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const GROK_URL: &str = "https://api.x.ai/v1/chat/completions";

#[derive(Serialize)]
struct GrokRequest {
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
struct GrokResponse {
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

/// Call Grok API with session context
pub async fn call_llm_api_with_session(
    api_key: &str,
    session: &mut crate::engine::types::LLMSession,
    incremental_update: String,
    max_tokens: usize
) -> Result<(String, u32, u32), String> {
    // Add the incremental update as a new user message
    session.add_user_message(incremental_update);
    
    // Convert session messages to Grok format
    let mut messages: Vec<Message> = Vec::new();
    
    // Add system message first
    messages.push(Message {
        role: "system".to_string(),
        content: session.get_system_prompt(),
    });
    
    // Use get_messages_for_api() to apply sliding window and prevent token explosion
    for msg in session.get_messages_for_api() {
        messages.push(Message {
            role: msg.role,
            content: msg.content,
        });
    }
    
    let request_body = GrokRequest {
        model: crate::engine::provider_config::get_model("grok"),
        max_tokens,
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
        .post(GROK_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Grok API: {}", e))?;
    
    if response.status().is_success() {
        let response_body: GrokResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Grok API response: {}", e))?;
        
        // Extract the text response
        let assistant_response = response_body.choices.first()
            .ok_or_else(|| "Empty response from Grok API".to_string())?
            .message.content.trim().to_string();
        
        // Update session token counts
        session.total_input_tokens += response_body.usage.prompt_tokens;
        session.total_output_tokens += response_body.usage.completion_tokens;
        session.api_calls += 1;
        
        // Add assistant response to session history
        session.messages.push(crate::engine::types::Message {
            role: "assistant".to_string(),
            content: assistant_response.clone(),
        });
        
        // Log token usage with session context
        info!(
            "Grok API token usage - Input: {} (session total: {}), Output: {} (session total: {})",
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
        
        error!("Error from Grok API: {}", error_message);
        Err(format!("Error from Grok API: {}", error_message))
    }
}

/// Call Grok API (stateless version)
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
    let request_body = GrokRequest {
        model: crate::engine::provider_config::get_model("grok"),
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
    
    // Send request
    let response = client
        .post(GROK_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Grok API: {}", e))?;
    
    if response.status().is_success() {
        let response_body: GrokResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Grok API response: {}", e))?;
        
        // Log token usage
        info!(
            "Grok API token usage - Input: {}, Output: {}",
            response_body.usage.prompt_tokens, response_body.usage.completion_tokens
        );

        // Fold into active LLM_SESSION (or standalone tally if none).
        crate::engine::usage_tracker::record_stateless_call(
            "grok",
            response_body.usage.prompt_tokens,
            response_body.usage.completion_tokens,
            0,
            0,
        );

        // Extract the text
        let response_text = response_body.choices.first()
            .ok_or_else(|| "Empty response from Grok API".to_string())
            .map(|choice| choice.message.content.trim().to_string())?;

        Ok((response_text, response_body.usage.prompt_tokens, response_body.usage.completion_tokens))
    } else {
        let error_message = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error message: {}", e))?;
        
        error!("Error from Grok API: {}", error_message);
        Err(format!("Error from Grok API: {}", error_message))
    }
} 