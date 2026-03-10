use rusqlite_from_row::FromRow;
use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, FromRow, Clone)]
pub struct AutomationLog {
    pub id: i64,
    pub automation_id: i64,
    pub timestamp: String,
    pub action_type: String,
    pub element_role: Option<String>,
    pub element_value: Option<String>,
    pub element_description: Option<String>,
    pub window_title: String,
    pub window_app_name: String,
    pub importance_score: i32,
    pub raw_event_data: Option<String>,
    pub sequence_number: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AutomationStep {
    pub step_number: i32,
    pub description: String,
    pub action_type: String,
    pub target: Option<String>,
    pub parameters: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AutomationSummary {
    pub automation_id: i64,
    pub name: String,
    pub objective: String,
    pub steps: Vec<AutomationStep>,
    pub duration_seconds: i64,
    pub app_count: i32,
    pub created_at: String,
} 