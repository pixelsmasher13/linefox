// OpenAI Codex Responses client (ChatGPT Plus/Pro subscription path).
//
// Hits `https://chatgpt.com/backend-api/codex/responses` using SSE, aggregates streamed
// `response.output_text.delta` events into a final string, and returns the same
// `(text, input_tokens, output_tokens)` shape as `openai::call_llm_api*` so callers
// can swap providers with a single match arm.
//
// Auth: caller passes the access token (`api_key`). The `chatgpt-account-id` header is
// derived on the fly from that token's JWT payload, so callers don't have to thread a
// second value through every dispatch site.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";

#[derive(Deserialize)]
struct JwtAuthClaim {
    chatgpt_account_id: Option<String>,
}

/// Pull `chatgpt_account_id` out of a Codex OAuth access token JWT.
fn account_id_from_token(access_token: &str) -> Result<String, String> {
    let mut parts = access_token.split('.');
    let _header = parts.next();
    let payload = parts
        .next()
        .ok_or_else(|| "ChatGPT access token is not a valid JWT".to_string())?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|e| format!("Could not decode ChatGPT access token payload: {}", e))?;
    let value: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|e| format!("ChatGPT access token payload was not JSON: {}", e))?;
    let claim = value.get(JWT_CLAIM_PATH).cloned().ok_or_else(|| {
        format!(
            "ChatGPT access token missing `{}` claim (re-sign in)",
            JWT_CLAIM_PATH
        )
    })?;
    let parsed: JwtAuthClaim = serde_json::from_value(claim).map_err(|e| {
        format!(
            "ChatGPT access token had unexpected `{}` shape: {}",
            JWT_CLAIM_PATH, e
        )
    })?;
    parsed
        .chatgpt_account_id
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "ChatGPT access token missing chatgpt_account_id; please sign in again".into())
}

const CODEX_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const ORIGINATOR: &str = "linefox";

#[derive(Serialize)]
struct CodexRequest<'a> {
    model: String,
    instructions: &'a str,
    input: Vec<InputItem>,
    stream: bool,
    store: bool,
    /// Optional reasoning controls. When omitted, the Codex backend picks its own
    /// default (currently `medium` for gpt-5.5). When present, `effort` is one of
    /// `minimal | low | medium | high | xhigh` (model-dependent; `gpt-5.5` clamps
    /// `minimal` -> `low`). `summary: "auto"` lets the model decide how much
    /// reasoning summary to emit on the stream.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningParam>,
}

#[derive(Serialize)]
struct ReasoningParam {
    effort: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<&'static str>,
}

#[derive(Serialize)]
struct InputItem {
    role: String,
    content: Vec<InputContent>,
}

#[derive(Serialize)]
struct InputContent {
    #[serde(rename = "type")]
    content_type: &'static str,
    text: String,
}

#[derive(Deserialize, Debug)]
struct ResponseUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    #[serde(default)]
    input_tokens_details: Option<InputTokensDetails>,
    #[serde(default)]
    output_tokens_details: Option<OutputTokensDetails>,
}

#[derive(Deserialize, Debug)]
struct InputTokensDetails {
    #[serde(default)]
    cached_tokens: u32,
}

#[derive(Deserialize, Debug)]
struct OutputTokensDetails {
    #[serde(default)]
    reasoning_tokens: u32,
}

fn user_agent() -> String {
    format!(
        "linefox/{} ({}; {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn build_text_item(role: &str, text: String) -> InputItem {
    // The Responses API uses different content-type tags depending on whether the
    // message represents *input to* the model or a prior *output from* the model:
    //   * user / system / developer / tool messages  ->  "input_text"
    //   * assistant messages echoed back as history  ->  "output_text"
    // Mixing them up yields HTTP 400: "Invalid value: 'input_text'. Supported
    // values are: 'output_text' and 'refusal'." on multi-turn calls.
    let content_type = if role == "assistant" {
        "output_text"
    } else {
        "input_text"
    };
    InputItem {
        role: role.to_string(),
        content: vec![InputContent {
            content_type,
            text,
        }],
    }
}

// Streaming time budgets.
//
// `gpt-5.5` and other reasoning models can spend many minutes on hard prompts
// before any output bytes arrive, so a single global wall-clock timeout (which
// reqwest counts against the entire request including body read) is the wrong
// shape — it manifests as a confusing "Transport error: error decoding response
// body" the moment the deadline hits mid-stream.
//
// Instead we use:
//   * `connect_timeout`  – cap the initial TCP/TLS handshake.
//   * `read_timeout`     – cap each individual read on the body. As long as the
//                          server keeps trickling SSE keepalives or events, the
//                          stream stays alive.
//   * `STREAM_WALL_CAP`  – an outer wall-clock cap enforced inside the SSE loop
//                          so we still bail on truly stuck connections without
//                          hanging forever.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const READ_TIMEOUT: Duration = Duration::from_secs(180);
const STREAM_WALL_CAP: Duration = Duration::from_secs(900); // 15 minutes — generous for deep reasoning

/// POST to the Codex Responses endpoint and aggregate SSE deltas into a single string.
async fn run_codex(
    api_key: &str,
    model: String,
    instructions: &str,
    input: Vec<InputItem>,
    max_tokens: usize,
) -> Result<(String, u32, u32, u32, u32), String> {
    let account_id = account_id_from_token(api_key)?;
    let reasoning = {
        let effort = crate::engine::provider_config::get_openai_codex_reasoning_effort();
        let trimmed = effort.trim().to_lowercase();
        // `none` / empty -> don't send the field at all (let backend default).
        // Anything else gets passed through; the backend rejects unknown values
        // with a 400, which is louder than silently ignoring them.
        if trimmed.is_empty() || trimmed == "none" || trimmed == "default" {
            None
        } else {
            Some(ReasoningParam {
                effort: trimmed,
                summary: Some("auto"),
            })
        }
    };
    let body = CodexRequest {
        model: model.clone(),
        instructions,
        input,
        stream: true,
        store: false,
        reasoning,
    };
    // The Responses API exposes `max_output_tokens`, but Codex models silently ignore
    // unknown fields. We log the requested cap for parity with other providers but don't
    // forward it, since the Codex backend rejects strict caps for reasoning models.
    let _ = max_tokens;

    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let request_started = std::time::Instant::now();
    let resp = client
        .post(CODEX_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("chatgpt-account-id", &account_id)
        .header("originator", ORIGINATOR)
        .header("User-Agent", user_agent())
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .header("OpenAI-Beta", "responses=experimental")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to OpenAI Codex: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        error!(
            "OpenAI Codex Responses returned HTTP {}: {}",
            status.as_u16(),
            text
        );
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(format!(
                "ChatGPT subscription auth failed ({}). Sign in again from Settings. Body: {}",
                status.as_u16(),
                text
            ));
        }
        return Err(format!(
            "OpenAI Codex error {}: {}",
            status.as_u16(),
            text
        ));
    }

    let mut stream = resp.bytes_stream().eventsource();

    // Stream observability — surface enough state in error logs to actually
    // diagnose stalls/timeouts in the field (otherwise "stream error" tells us
    // nothing about *when* it died or whether the server sent anything).
    let stream_started = std::time::Instant::now();
    let mut event_count: u64 = 0;
    let mut delta_count: u64 = 0;
    let mut bytes_seen: u64 = 0;
    let mut last_event_type = String::from("<none>");
    let mut last_event_at: Option<std::time::Instant> = None;

    let mut text = String::new();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut reasoning_tokens: u32 = 0;
    let mut cached_tokens: u32 = 0;
    let mut saw_completed = false;

    loop {
        // Outer wall-clock guard — if the server goes silent past the per-read
        // timeout AND the loop somehow doesn't error out, this catches it.
        let elapsed = stream_started.elapsed();
        if elapsed > STREAM_WALL_CAP {
            warn!(
                "OpenAI Codex stream exceeded wall-clock cap of {:?} after {} events ({} deltas, {} bytes); last event '{}' at {:?} ago",
                STREAM_WALL_CAP,
                event_count,
                delta_count,
                bytes_seen,
                last_event_type,
                last_event_at.map(|t| t.elapsed())
            );
            return Err(format!(
                "OpenAI Codex stream exceeded wall-clock cap of {:?} (events={}, last_type='{}')",
                STREAM_WALL_CAP, event_count, last_event_type
            ));
        }

        let next = stream.next().await;
        let event = match next {
            Some(Ok(e)) => e,
            Some(Err(e)) => {
                warn!(
                    "OpenAI Codex SSE stream error after {:.1}s, {} event(s) ({} delta(s), {} bytes), last_type='{}' (last event {:?} ago): {}",
                    stream_started.elapsed().as_secs_f64(),
                    event_count,
                    delta_count,
                    bytes_seen,
                    last_event_type,
                    last_event_at.map(|t| t.elapsed()),
                    e
                );
                return Err(format!(
                    "OpenAI Codex SSE stream error after {:.1}s, {} event(s), last='{}' (req total {:.1}s): {}",
                    stream_started.elapsed().as_secs_f64(),
                    event_count,
                    last_event_type,
                    request_started.elapsed().as_secs_f64(),
                    e
                ));
            }
            None => break, // stream ended cleanly
        };
        event_count += 1;
        bytes_seen = bytes_seen.saturating_add(event.data.len() as u64);
        last_event_at = Some(std::time::Instant::now());

        // The Codex Responses stream emits events like:
        //   event: response.output_text.delta
        //   data: {"type":"response.output_text.delta","delta":"hello"}
        //   event: response.completed
        //   data: {"type":"response.completed","response":{...,"usage":{...}}}
        let payload: serde_json::Value = match serde_json::from_str(&event.data) {
            Ok(v) => v,
            Err(_) => continue, // ignore non-JSON keepalives
        };
        let kind = payload
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !kind.is_empty() {
            last_event_type = kind.to_string();
        }
        match kind {
            "response.output_text.delta" => {
                delta_count += 1;
                if let Some(delta) = payload.get("delta").and_then(|v| v.as_str()) {
                    text.push_str(delta);
                }
            }
            "response.completed" => {
                saw_completed = true;
                if let Some(response) = payload.get("response") {
                    if let Some(usage) = response.get("usage") {
                        if let Ok(parsed) =
                            serde_json::from_value::<ResponseUsage>(usage.clone())
                        {
                            input_tokens = parsed.input_tokens;
                            output_tokens = parsed.output_tokens;
                            reasoning_tokens = parsed
                                .output_tokens_details
                                .as_ref()
                                .map(|d| d.reasoning_tokens)
                                .unwrap_or(0);
                            cached_tokens = parsed
                                .input_tokens_details
                                .as_ref()
                                .map(|d| d.cached_tokens)
                                .unwrap_or(0);
                        }
                    }
                    // Fallback: some payloads put the full text under output[].content[].text.
                    if text.is_empty() {
                        if let Some(arr) = response.get("output").and_then(|v| v.as_array()) {
                            for item in arr {
                                if let Some(content) =
                                    item.get("content").and_then(|v| v.as_array())
                                {
                                    for c in content {
                                        if let Some(t) =
                                            c.get("text").and_then(|v| v.as_str())
                                        {
                                            text.push_str(t);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "response.failed" | "response.error" => {
                let msg = payload
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("OpenAI Codex returned a failed response");
                error!(
                    "OpenAI Codex stream failure after {:.1}s, {} event(s): {} (full payload: {})",
                    stream_started.elapsed().as_secs_f64(),
                    event_count,
                    msg,
                    payload
                );
                return Err(format!("OpenAI Codex stream failure: {}", msg));
            }
            _ => {}
        }
    }

    if !saw_completed && text.is_empty() {
        warn!(
            "OpenAI Codex stream ended after {:.1}s, {} event(s) ({} bytes), last_type='{}' — no completion or text",
            stream_started.elapsed().as_secs_f64(),
            event_count,
            bytes_seen,
            last_event_type
        );
        return Err(format!(
            "OpenAI Codex stream ended without any completion event ({} events seen, last='{}', {:.1}s)",
            event_count,
            last_event_type,
            stream_started.elapsed().as_secs_f64()
        ));
    }

    info!(
        "OpenAI Codex stream OK in {:.1}s ({} event(s), {} delta(s), {} bytes); tokens: input={} output={} reasoning={} cached={}",
        stream_started.elapsed().as_secs_f64(),
        event_count,
        delta_count,
        bytes_seen,
        input_tokens,
        output_tokens,
        reasoning_tokens,
        cached_tokens,
    );

    Ok((
        text.trim().to_string(),
        input_tokens,
        output_tokens,
        reasoning_tokens,
        cached_tokens,
    ))
}

/// Stateless text completion. Drop-in compatible with `openai::call_llm_api`.
/// `api_key` here is the OAuth access token from `crate::auth::openai_codex_oauth`;
/// the `chatgpt-account-id` header is decoded from its JWT payload internally.
pub async fn call_llm_api(
    api_key: &str,
    prompt: String,
    system_prompt: &str,
    max_tokens: usize,
) -> Result<(String, u32, u32), String> {
    let model = crate::engine::provider_config::get_model("openai-codex");
    let input = vec![build_text_item("user", prompt)];
    let (text, input_tokens, output_tokens, _r, cached) = run_codex(
        api_key,
        model,
        system_prompt,
        input,
        max_tokens,
    )
    .await?;

    // Fold into active LLM_SESSION (or standalone tally if none).
    crate::engine::usage_tracker::record_stateless_call(
        "openai-codex",
        input_tokens,
        output_tokens,
        cached,
        0,
    );

    Ok((text, input_tokens, output_tokens))
}

/// Session-aware completion. Drop-in compatible with `openai::call_llm_api_with_session`.
pub async fn call_llm_api_with_session(
    api_key: &str,
    session: &mut crate::engine::types::LLMSession,
    incremental_update: String,
    max_tokens: usize,
) -> Result<(String, u32, u32), String> {
    session.add_user_message(incremental_update);

    let mut input: Vec<InputItem> = Vec::new();
    for msg in session.get_messages_for_api() {
        input.push(build_text_item(&msg.role, msg.content));
    }

    let model = crate::engine::provider_config::get_model("openai-codex");
    let system_prompt = session.get_system_prompt();
    let (text, input_tokens, output_tokens, _r, cached) = run_codex(
        api_key,
        model,
        &system_prompt,
        input,
        max_tokens,
    )
    .await?;

    session.total_input_tokens += input_tokens;
    session.total_output_tokens += output_tokens;
    session.cache_read_tokens += cached;
    session.api_calls += 1;
    session.add_assistant_response(text.clone());

    Ok((text, input_tokens, output_tokens))
}
