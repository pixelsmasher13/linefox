use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Represents a single piece of data extracted from a task execution.
/// This allows the AI to persist useful data (contacts, research results, etc.) for future reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExtractedData {
    pub id: i64,
    pub execution_run_id: i64,
    pub automation_id: i64,
    /// Human-readable identifier (e.g., "Sarah Chen", "Apple 10-K 2024")
    pub record_name: String,
    /// Category (e.g., "contact", "financial_report", "product", "article", "price_info")
    pub record_type: String,
    /// Brief context of where data came from (e.g., "LinkedIn search", "SEC filing", "Amazon product page")
    pub source_context: Option<String>,
    /// Flexible JSON data blob with structured fields
    pub data: JsonValue,
    pub created_at: String,
}

/// Input structure for creating new extracted data records
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedDataRecord {
    pub record_name: String,
    pub record_type: String,
    pub source_context: Option<String>,
    pub data: JsonValue,
}

/// LLM response structure for data extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataExtractionResponse {
    /// Whether the task produced data worth storing
    pub has_useful_data: bool,
    /// Explanation of why data is/isn't worth storing
    pub reasoning: String,
    /// The extracted records (if any)
    pub records: Vec<ExtractedDataRecord>,
}
