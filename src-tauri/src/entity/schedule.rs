use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AutomationSchedule {
    pub id: i64,
    pub automation_id: i64,
    pub recurrence_type: String,       // "daily" | "weekdays" | "weekly"
    pub recurrence_days: Option<String>, // JSON array e.g. "[1,3,5]" for weekly
    pub execution_hour: i32,
    pub execution_minute: i32,
    pub timezone: String,
    pub is_active: bool,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub persistent_run_id: Option<i64>,   // If set, continue from this execution run
    pub continuation_prompt: Option<String>, // Custom prompt for continuation mode
}

/// Lightweight version sent to the frontend (includes automation name)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScheduleWithName {
    pub id: i64,
    pub automation_id: i64,
    pub automation_name: String,
    pub recurrence_type: String,
    pub recurrence_days: Option<Vec<i32>>,
    pub execution_hour: i32,
    pub execution_minute: i32,
    pub timezone: String,
    pub is_active: bool,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
}
