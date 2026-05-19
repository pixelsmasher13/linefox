//! Centralized LLM usage tracking for stateless provider calls.
//!
//! Provider modules (claude.rs, openai.rs, openai_codex.rs, ...) call
//! `record_stateless_call` after a successful one-shot API request. If there's
//! a live `LLM_SESSION` (owned by `automation_agent_engine`), the call is
//! folded into that session's counters so summary/script-gen passes inside an
//! automation are correctly attributed to the same usage_session row.
//! Otherwise the call is recorded into a per-process `STANDALONE_TALLY` so
//! standalone calls aren't completely invisible to diagnostics.

use std::sync::{Arc, Mutex};

use lazy_static::lazy_static;
use log::debug;

lazy_static! {
    /// Per-process running tally of stateless calls that landed without an
    /// active session. Not written to the DB — diagnostic only.
    static ref STANDALONE_TALLY: Arc<Mutex<StandaloneTally>> =
        Arc::new(Mutex::new(StandaloneTally::default()));
}

#[derive(Default, Debug, Clone)]
pub struct StandaloneTally {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub calls: u64,
    pub last_provider: String,
}

/// Called by each provider's stateless `call_llm_api` after the API response
/// is parsed. Folds into the active `LLM_SESSION` if one exists; otherwise
/// records to `STANDALONE_TALLY`.
pub fn record_stateless_call(
    provider: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_tokens: u32,
    cache_creation_tokens: u32,
) {
    // 1) Fold into the active session if one exists.
    if let Ok(mut guard) = crate::engine::automation_agent_engine::LLM_SESSION.lock() {
        if let Some(session) = guard.as_mut() {
            session.total_input_tokens = session.total_input_tokens.saturating_add(input_tokens);
            session.total_output_tokens = session.total_output_tokens.saturating_add(output_tokens);
            session.cache_read_tokens = session.cache_read_tokens.saturating_add(cache_read_tokens);
            session.cache_creation_tokens = session
                .cache_creation_tokens
                .saturating_add(cache_creation_tokens);
            session.api_calls = session.api_calls.saturating_add(1);
            debug!(
                "usage_tracker: folded stateless {} call into active session (+{}in/+{}out/+{}cR/+{}cC)",
                provider, input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens
            );
            return;
        }
    }

    // 2) No active session — record into standalone tally so the call isn't invisible.
    if let Ok(mut tally) = STANDALONE_TALLY.lock() {
        tally.input_tokens = tally.input_tokens.saturating_add(input_tokens as u64);
        tally.output_tokens = tally.output_tokens.saturating_add(output_tokens as u64);
        tally.cache_read_tokens = tally
            .cache_read_tokens
            .saturating_add(cache_read_tokens as u64);
        tally.cache_creation_tokens = tally
            .cache_creation_tokens
            .saturating_add(cache_creation_tokens as u64);
        tally.calls = tally.calls.saturating_add(1);
        tally.last_provider = provider.to_string();
        debug!(
            "usage_tracker: standalone {} call (+{}in/+{}out) — no active session",
            provider, input_tokens, output_tokens
        );
    }
}

/// Snapshot the standalone tally for diagnostics.
#[allow(dead_code)]
pub fn snapshot_standalone_tally() -> StandaloneTally {
    STANDALONE_TALLY.lock().map(|t| t.clone()).unwrap_or_default()
}
