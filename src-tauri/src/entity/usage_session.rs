use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSession {
    pub id: i64,
    pub user_id: String,
    pub automation_id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub total_seconds: i32,
    pub billed_minutes: i32,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewUsageSession {
    pub user_id: String,
    pub automation_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub total_minutes: i32,
    pub sessions_count: i32,
    pub current_month_minutes: i32,
} 