use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationExecutionRun {
    pub id: i64,
    pub automation_id: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String, // running, completed, failed, stopped
    pub additional_instructions: Option<String>,
    pub error_message: Option<String>,
    pub clipboard: Option<String>, // Data collected during execution via MEMORY_SAVE
    pub completion_message: Option<String>, // LLM-generated closing message summarizing results
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationExecutionStep {
    pub id: i64,
    pub execution_run_id: i64,
    pub step_number: i32,
    pub timestamp: String,
    pub explanation: Option<String>,
    pub next_step: Option<String>,
    pub status: String, // pending, executing, completed, error
    pub error_message: Option<String>,
    pub created_at: String,
    pub step_type: String, // action (default), user_prompt, completion
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRunWithSteps {
    pub run: AutomationExecutionRun,
    pub steps: Vec<AutomationExecutionStep>,
}