use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Mutex;

pub const DEFAULT_CLAUDE_MODEL: &str = "claude-sonnet-4-5-20250929";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-5";
pub const DEFAULT_GROK_MODEL: &str = "grok-3";
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";
pub const DEFAULT_DEEPSEEK_MODEL: &str = "deepseek-chat";

lazy_static! {
    static ref MODEL_OVERRIDES: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
}

/// Store a model override for a provider. Empty string clears the override.
pub fn set_model(provider: &str, model: &str) {
    let mut map = MODEL_OVERRIDES.lock().unwrap();
    if model.is_empty() {
        map.remove(provider);
    } else {
        map.insert(provider.to_string(), model.to_string());
    }
}

/// Returns the active model for a provider: override if set, otherwise the hardcoded default.
pub fn get_model(provider: &str) -> String {
    let overrides = MODEL_OVERRIDES.lock().unwrap();
    if let Some(m) = overrides.get(provider) {
        if !m.is_empty() {
            return m.clone();
        }
    }
    match provider {
        "openai"    => DEFAULT_OPENAI_MODEL.to_string(),
        "grok"      => DEFAULT_GROK_MODEL.to_string(),
        "gemini"    => DEFAULT_GEMINI_MODEL.to_string(),
        "deepseek"  => DEFAULT_DEEPSEEK_MODEL.to_string(),
        _           => DEFAULT_CLAUDE_MODEL.to_string(),
    }
}

/// Bulk-initialise from DB settings on startup (empty/missing values are ignored).
pub fn init_from_settings(
    model_claude: &str,
    model_openai: &str,
    model_grok: &str,
    model_gemini: &str,
    model_deepseek: &str,
) {
    set_model("claude",   model_claude);
    set_model("openai",   model_openai);
    set_model("grok",     model_grok);
    set_model("gemini",   model_gemini);
    set_model("deepseek", model_deepseek);
}
