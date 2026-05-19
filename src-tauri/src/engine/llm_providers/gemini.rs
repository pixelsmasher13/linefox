use log::{info, warn, error};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json;
use std::time::Duration;

fn gemini_url() -> String {
    format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        crate::engine::provider_config::get_model("gemini")
    )
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig,
    #[serde(rename = "systemInstruction")]
    system_instruction: Option<SystemInstruction>,
}

#[derive(Serialize)]
struct SystemInstruction {
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct Part {
    text: String,
}

#[derive(Serialize)]
struct GenerationConfig {
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: usize,
    temperature: f32,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<UsageMetadata>,
    #[serde(skip)]
    error: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Candidate {
    content: CandidateContent,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
    #[serde(skip)]
    index: Option<u32>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct CandidateContent {
    #[serde(default)]
    parts: Vec<ResponsePart>,
    #[serde(skip)]
    role: Option<String>,
}

#[derive(Deserialize)]
struct ResponsePart {
    text: String,
}

#[derive(Deserialize)]
struct UsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: u32,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>, // Optional because it might be missing
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>, // Gemini sometimes returns this instead
}

/// Call Gemini API with session context
pub async fn call_llm_api_with_session(
    api_key: &str,
    session: &mut crate::engine::types::LLMSession,
    incremental_update: String,
    max_tokens: usize
) -> Result<(String, u32, u32), String> {
    // Add the incremental update as a new user message
    session.add_user_message(incremental_update);
    
    // Build contents array for Gemini
    let mut contents: Vec<GeminiContent> = Vec::new();
    
    // Add conversation history (user and model messages)
    for msg in session.get_messages_for_api() {
        // Gemini uses "user" and "model" as roles
        let role = if msg.role == "assistant" { "model" } else { "user" };
        contents.push(GeminiContent {
            role: role.to_string(),
            parts: vec![Part { text: msg.content }],
        });
    }
    
    // Create system instruction
    let system_instruction = Some(SystemInstruction {
        parts: vec![Part { text: session.get_system_prompt() }],
    });
    
    let request_body = GeminiRequest {
        contents,
        generation_config: GenerationConfig {
            max_output_tokens: max_tokens,
            temperature: 0.7,
        },
        system_instruction,
    };
    
    // Configure client
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    // Build URL with API key
    let url = format!("{}?key={}", gemini_url(), api_key);
    
    // Send request
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Gemini API: {}", e))?;
    
    if response.status().is_success() {
        // First get the response as text for debugging
        let response_text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read Gemini API response: {}", e))?;
        
        // Try to parse the response
        let response_body: GeminiResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!("Failed to parse Gemini response as JSON: {}", e);
                error!("Response was: {}", if response_text.len() > 500 { 
                    format!("{}...", crate::engine::types::safe_truncate(&response_text, 500)) 
                } else { 
                    response_text.clone() 
                });
                format!("Failed to parse Gemini API response: {}", e)
            })?;
        
        // Check if we have candidates
        if response_body.candidates.is_empty() {
            error!("Gemini API returned no candidates");
            return Err("Gemini API returned empty response - may have hit safety filters or rate limits".to_string());
        }
        
        // Check if response was truncated due to token limit
        let first_candidate = &response_body.candidates[0];
        if let Some(ref finish_reason) = first_candidate.finish_reason {
            if finish_reason == "MAX_TOKENS" {
                error!("Gemini API response was truncated due to token limit");
                return Err("Gemini response was cut off - hit maximum token limit. Try with a shorter prompt or increase max_tokens.".to_string());
            }
        }
        
        // Extract the text response
        if first_candidate.content.parts.is_empty() {
            error!("Gemini API candidate has no content parts");
            return Err("Gemini API returned response with no content".to_string());
        }
        
        let assistant_response = first_candidate.content.parts[0].text.trim().to_string();
        
        if assistant_response.is_empty() {
            warn!("Gemini API returned empty text response");
            return Err("Gemini API returned empty text - may need to retry".to_string());
        }
        
        // Extract token counts (with defaults if not provided)
        let (input_tokens, output_tokens) = if let Some(usage) = response_body.usage_metadata {
            let output = usage.candidates_token_count.unwrap_or_else(|| {
                // If candidates_token_count is missing, try to calculate from total
                usage.total_token_count.map(|total| {
                    total.saturating_sub(usage.prompt_token_count)
                }).unwrap_or(0)
            });
            (usage.prompt_token_count, output)
        } else {
            (0, 0)
        };
        
        // Update session token counts
        session.total_input_tokens += input_tokens;
        session.total_output_tokens += output_tokens;
        session.api_calls += 1;
        
        // Add assistant response to session history
        session.add_assistant_response(assistant_response.clone());
        
        // Log token usage with session context
        info!(
            "Gemini API token usage - Input: {} (session total: {}), Output: {} (session total: {})",
            input_tokens, 
            session.total_input_tokens,
            output_tokens,
            session.total_output_tokens
        );
        
        Ok((assistant_response, input_tokens, output_tokens))
    } else {
        let error_message = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error message: {}", e))?;
        
        error!("Error from Gemini API: {}", error_message);
        Err(format!("Error from Gemini API: {}", error_message))
    }
}

/// Call Gemini API (stateless version)
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

    // Create request with system instruction
    let request_body = GeminiRequest {
        contents: vec![
            GeminiContent {
                role: "user".to_string(),
                parts: vec![Part { text: prompt }],
            }
        ],
        generation_config: GenerationConfig {
            max_output_tokens: max_tokens,
            temperature: 0.7,
        },
        system_instruction: Some(SystemInstruction {
            parts: vec![Part { text: system_prompt.to_string() }],
        }),
    };
    
    // Build URL with API key
    let url = format!("{}?key={}", gemini_url(), api_key);
    
    // Send request
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Gemini API: {}", e))?;
    
    if response.status().is_success() {
        // First get the response as text for debugging
        let response_text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read Gemini API response: {}", e))?;
        
        // Try to parse the response
        let response_body: GeminiResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!("Failed to parse Gemini response as JSON: {}", e);
                error!("Response was: {}", if response_text.len() > 500 { 
                    format!("{}...", crate::engine::types::safe_truncate(&response_text, 500)) 
                } else { 
                    response_text.clone() 
                });
                format!("Failed to parse Gemini API response: {}", e)
            })?;
        
        // Extract token counts (with defaults if not provided)
        let (input_tokens, output_tokens) = if let Some(usage) = response_body.usage_metadata {
            let output = usage.candidates_token_count.unwrap_or_else(|| {
                // If candidates_token_count is missing, try to calculate from total
                usage.total_token_count.map(|total| {
                    total.saturating_sub(usage.prompt_token_count)
                }).unwrap_or(0)
            });
            (usage.prompt_token_count, output)
        } else {
            (0, 0)
        };
        
        // Log token usage
        info!(
            "Gemini API token usage - Input: {}, Output: {}",
            input_tokens, output_tokens
        );
        
        // Check if we have candidates
        if response_body.candidates.is_empty() {
            error!("Gemini API returned no candidates");
            return Err("Gemini API returned empty response - may have hit safety filters or rate limits".to_string());
        }
        
        // Extract the text
        let first_candidate = &response_body.candidates[0];
        if first_candidate.content.parts.is_empty() {
            error!("Gemini API candidate has no content parts");
            return Err("Gemini API returned response with no content".to_string());
        }
        
        let response_text = first_candidate.content.parts[0].text.trim().to_string();
        
        if response_text.is_empty() {
            warn!("Gemini API returned empty text response");
            return Err("Gemini API returned empty text - may need to retry".to_string());
        }

        // Fold into active LLM_SESSION (or standalone tally if none).
        crate::engine::usage_tracker::record_stateless_call(
            "gemini",
            input_tokens,
            output_tokens,
            0,
            0,
        );

        Ok((response_text, input_tokens, output_tokens))
    } else {
        let error_message = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error message: {}", e))?;
        
        error!("Error from Gemini API: {}", error_message);
        Err(format!("Error from Gemini API: {}", error_message))
    }
}
