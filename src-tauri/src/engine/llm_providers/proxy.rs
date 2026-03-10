// Stub proxy module for removed cloud functionality
// The proxy LLM provider has been removed in the open source version
// Users should use direct API providers instead (Claude, OpenAI, Grok, Gemini)

use crate::engine::types::LLMSession;

/// Stub function - proxy removed in open source version
pub async fn call_llm_api(
    _api_key: &str,
    _prompt: String,
    _system_prompt: &str,
    _max_tokens: usize,
) -> Result<(String, u32, u32), String> {
    Err("Proxy LLM provider has been removed in the open source version. Please configure a direct API provider (Claude, OpenAI, Grok, or Gemini) in settings.".to_string())
}

/// Stub function - proxy removed in open source version
pub async fn call_llm_api_with_session(
    _api_key: &str,
    _session: &mut LLMSession,
    _incremental_update: String,
    _max_tokens: usize,
    _use_pro_model: bool,
) -> Result<(String, u32, u32), String> {
    Err("Proxy LLM provider has been removed in the open source version. Please configure a direct API provider (Claude, OpenAI, Grok, or Gemini) in settings.".to_string())
}
