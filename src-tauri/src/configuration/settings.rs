use rusqlite_from_row::FromRow;
use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, FromRow, Clone)]
pub struct Settings {
    pub is_dev_mode: bool,
    pub interval: String,
    pub auto_start: bool,
    pub api_choice: String,
    pub api_key_claude: String,
    pub api_key_open_ai: String,
    pub api_key_grok: String,
    pub api_key_gemini: String,
    pub use_pro_model: bool,
    pub store_task_data: bool,
    pub model_claude: String,
    pub model_openai: String,
    pub model_grok: String,
    pub model_gemini: String,
    pub api_key_deepseek: String,
    pub model_deepseek: String,
}