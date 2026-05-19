use rusqlite_from_row::FromRow;
use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, FromRow, Clone)]
pub struct Settings {
    pub is_dev_mode: bool,
    pub interval: String,
    pub auto_start: bool,
    pub api_choice: String,
    pub api_key_claude: String,
    /// OAuth token (`sk-ant-oat01-...`) for the Claude Pro/Max subscription path.
    /// Obtained by the user running `claude setup-token` in their terminal and
    /// pasting the result. When `api_choice == "claude-subscription"` this is the
    /// credential we send to Anthropic via `Authorization: Bearer ...` instead of
    /// the regular `x-api-key` API key. Distinct field so users can keep both an
    /// API key and a subscription token configured and switch between them.
    #[serde(default)]
    pub api_key_claude_oauth: String,
    pub api_key_open_ai: String,
    pub api_key_grok: String,
    pub api_key_gemini: String,
    pub use_pro_model: bool,
    pub store_task_data: bool,
    pub model_claude: String,
    pub model_openai: String,
    /// Model used when api_choice == "openai-codex" (ChatGPT subscription via Codex Responses).
    /// Defaults to gpt-5.5; only models accessible to a ChatGPT Plus/Pro plan work here.
    #[serde(default)]
    pub model_openai_codex: String,
    /// Reasoning effort for the Codex Responses backend. One of:
    /// "minimal" | "low" | "medium" | "high" | "xhigh". Empty means default (medium).
    #[serde(default)]
    pub openai_codex_reasoning_effort: String,
    pub model_grok: String,
    pub model_gemini: String,
    pub api_key_deepseek: String,
    pub model_deepseek: String,
}