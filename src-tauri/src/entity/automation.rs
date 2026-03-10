use rusqlite_from_row::FromRow;
use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, FromRow, Clone)]
pub struct Automation {
    pub id: i64,
    pub name: String,
    pub objective: String,
    pub raw_script: String,
    pub generalized_script: String,
    pub nl_description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// This is an alternate version that includes script steps for the frontend
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AutomationScript {
    pub id: i64,
    pub name: String,
    pub objective: String,
    pub nl_description: Option<String>,
    pub steps: Vec<AutomationStep>,
    pub additional_instructions: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AutomationStep {
    pub id: i64,
    pub action: String,
    pub target: String,
    pub description: String,
    pub params: Option<serde_json::Value>,
} 