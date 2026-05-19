use log::{info, error};
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";

/// Prefix of OAuth tokens issued by `claude setup-token` (Claude Code's
/// subscription auth flow). Anthropic API keys start with `sk-ant-api03-`,
/// so the two are unambiguous by prefix and we route auth accordingly:
///   * `sk-ant-oat01-...`  -> `Authorization: Bearer ...` + `anthropic-beta`
///   * anything else       -> `x-api-key: ...`
const OAUTH_TOKEN_PREFIX: &str = "sk-ant-oat01-";

/// Beta header required by Anthropic when authenticating with a Claude
/// Pro/Max OAuth token instead of an API key. Matches what the Claude Agent
/// SDK sets on subscription-billed `/v1/messages` calls.
const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";

/// Identity passphrase Anthropic requires as the **first** `system` block when
/// authenticating with a subscription OAuth token (`sk-ant-oat01-*`). Since
/// roughly March 2026 the server-side check rejects Sonnet/Opus OAuth calls
/// that don't lead with this exact string (any other wording -> 400/401). The
/// model still follows the user's real system prompt because subsequent
/// blocks override identity, but the first block must be byte-for-byte equal
/// to this. API-key callers must NOT send this prefix - they get billed for
/// the extra tokens and it's not needed.
const OAUTH_SYSTEM_IDENTITY: &str =
    "You are Claude Code, Anthropic's official CLI for Claude.";

fn is_oauth_token(token: &str) -> bool {
    sanitize_credential(token).starts_with(OAUTH_TOKEN_PREFIX)
}

/// Strip ALL whitespace from a credential before sending it to Anthropic.
///
/// The `claude setup-token` CLI prints OAuth tokens that wrap across the
/// terminal's column width, so copy-paste flows routinely deliver a token
/// with embedded newlines or surrounding spaces. Anthropic then 401s with
/// "Invalid bearer token" because the literal `\n` (or the truncated half)
/// isn't a valid token. OAuth tokens and `sk-ant-api03-*` API keys are both
/// guaranteed to be whitespace-free, so unconditional whitespace stripping
/// is safe defense-in-depth — much friendlier than asking users to paste
/// twice.
fn sanitize_credential(credential: &str) -> String {
    credential.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Attach the right auth + beta headers for either API-key or subscription
/// (OAuth) credentials. All other headers (`Content-Type`,
/// `anthropic-version`) stay identical.
fn apply_auth_headers(builder: RequestBuilder, credential: &str) -> RequestBuilder {
    let clean = sanitize_credential(credential);
    if is_oauth_token(&clean) {
        info!(
            "Claude auth: Bearer (subscription OAuth), credential length={}",
            clean.len()
        );
        builder
            .header("Authorization", format!("Bearer {}", clean))
            .header("anthropic-beta", OAUTH_BETA_HEADER)
    } else {
        info!(
            "Claude auth: x-api-key (API credits), credential length={}",
            clean.len()
        );
        builder.header("x-api-key", clean)
    }
}

#[derive(Serialize)]
struct LLMRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Vec<SystemBlock>>,
    stream: bool,
}

#[derive(Serialize)]
struct SystemBlock {
    #[serde(rename = "type")]
    block_type: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    cache_type: &'static str,
}

/// Build the `system` array for `/v1/messages`. On the OAuth subscription
/// path we MUST lead with the Claude Code identity block (see
/// `OAUTH_SYSTEM_IDENTITY`); the user's real system prompt follows as a
/// second cached block and supplies the actual instructions. API-key callers
/// just send the user's prompt with cache_control as before.
fn cached_system(text: String, oauth: bool) -> Option<Vec<SystemBlock>> {
    let identity = || SystemBlock {
        block_type: "text",
        text: OAUTH_SYSTEM_IDENTITY.to_string(),
        cache_control: None,
    };
    let user_block = |text: String| SystemBlock {
        block_type: "text",
        text,
        cache_control: Some(CacheControl { cache_type: "ephemeral" }),
    };

    match (text.is_empty(), oauth) {
        // API key + no system prompt -> omit the field entirely.
        (true, false) => None,
        // OAuth + no system prompt -> still required to lead with identity.
        (true, true) => Some(vec![identity()]),
        // API key + system prompt -> single cached user block.
        (false, false) => Some(vec![user_block(text)]),
        // OAuth + system prompt -> identity first, then user prompt cached.
        (false, true) => Some(vec![identity(), user_block(text)]),
    }
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
    #[serde(default)]
    cache_creation_input_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
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
    
    // Create request with full conversation history.
    // System prompt wrapped with cache_control: ephemeral so Anthropic
    // serves the reused prefix from cache across automation turns.
    let request_body = LLMRequest {
        model: crate::engine::provider_config::get_model("claude"),
        max_tokens,
        messages: session.get_messages_for_api().into_iter().map(|msg| Message {
            role: msg.role,
            content: msg.content,
        }).collect(),
        system: cached_system(session.get_system_prompt(), is_oauth_token(api_key)),
        stream: false,
    };
    
    // Configure client
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    // Send request
    let builder = client
        .post(ANTHROPIC_URL)
        .header("Content-Type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .json(&request_body);
    let response = apply_auth_headers(builder, api_key)
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
        session.cache_read_tokens += response_body.usage.cache_read_input_tokens;
        session.cache_creation_tokens += response_body.usage.cache_creation_input_tokens;
        session.api_calls += 1;
        
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

        if response_body.usage.cache_read_input_tokens > 0
            || response_body.usage.cache_creation_input_tokens > 0
        {
            info!(
                "Claude cache: {} read, {} write, {} total input",
                response_body.usage.cache_read_input_tokens,
                response_body.usage.cache_creation_input_tokens,
                response_body.usage.input_tokens
            );
        }

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

    // Create request. Same ephemeral-cache treatment of the system prompt
    // as the session path — if the caller reuses a long system prompt
    // (e.g. completion/summary passes), repeat calls hit the prefix cache.
    let request_body = LLMRequest {
        model: crate::engine::provider_config::get_model("claude"),
        max_tokens,
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt,
        }],
        system: cached_system(system_prompt.to_string(), is_oauth_token(api_key)),
        stream: false,
    };
    
    // Send request
    let builder = client
        .post(ANTHROPIC_URL)
        .header("Content-Type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .json(&request_body);
    let response = apply_auth_headers(builder, api_key)
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

        if response_body.usage.cache_read_input_tokens > 0
            || response_body.usage.cache_creation_input_tokens > 0
        {
            info!(
                "Claude cache: {} read, {} write, {} total input",
                response_body.usage.cache_read_input_tokens,
                response_body.usage.cache_creation_input_tokens,
                response_body.usage.input_tokens
            );
        }

        // Fold into active LLM_SESSION (or standalone tally if none).
        crate::engine::usage_tracker::record_stateless_call(
            "claude",
            response_body.usage.input_tokens,
            response_body.usage.output_tokens,
            response_body.usage.cache_read_input_tokens,
            response_body.usage.cache_creation_input_tokens,
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
