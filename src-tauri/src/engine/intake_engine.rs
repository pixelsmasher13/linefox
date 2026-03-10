use tauri::AppHandle;

/// Generate follow-up intake questions for an automation objective using a lightweight LLM call.
/// Returns a vector of question strings.
/// 
/// NOTE: This function is now mostly deprecated. Intake questions are generated during
/// the script analysis phase in automation_summary_engine::process_automation_with_llm
/// This remains as a fallback for when LLM-based question generation fails.
pub async fn generate_intake_questions(_app_handle: &AppHandle, objective: &str) -> Result<Vec<String>, String> {
    // For backward compatibility, just return heuristic questions
    Ok(heuristic_questions(objective))
}

fn heuristic_questions(objective: &str) -> Vec<String> {
    // Analyze the objective to generate contextual questions
    let obj_lower = objective.to_lowercase();
    
    let mut questions = Vec::new();
    
    // File/export related
    if obj_lower.contains("file") || obj_lower.contains("export") || obj_lower.contains("save") || obj_lower.contains("download") {
        questions.push("Where should the files be saved? (provide full folder path)".to_string());
        if obj_lower.contains("export") || obj_lower.contains("report") {
            questions.push("What naming convention should be used for the files?".to_string());
        }
    }
    
    // Date/time related  
    if obj_lower.contains("date") || obj_lower.contains("time") || obj_lower.contains("period") || obj_lower.contains("daily") || obj_lower.contains("weekly") {
        questions.push("What date range or time period should be used?".to_string());
    }
    
    // Search/filter related
    if obj_lower.contains("search") || obj_lower.contains("find") || obj_lower.contains("filter") || obj_lower.contains("specific") {
        questions.push("What specific criteria should be used for filtering?".to_string());
    }
    
    // Email/notification related
    if obj_lower.contains("email") || obj_lower.contains("send") || obj_lower.contains("notify") {
        questions.push("What email address(es) should be used?".to_string());
    }
    
    // Data processing related
    if obj_lower.contains("process") || obj_lower.contains("analyze") || obj_lower.contains("calculate") {
        questions.push("Are there any specific thresholds or limits to consider?".to_string());
    }
    
    // If no specific patterns found, check if it's a simple navigation task
    if questions.is_empty() {
        // Simple tasks that don't need questions
        if obj_lower.contains("open") && !obj_lower.contains("file") ||
           obj_lower.contains("launch") ||
           obj_lower.contains("close") ||
           obj_lower.contains("navigate to") && obj_lower.split_whitespace().count() < 10 {
            // No questions needed for simple navigation
            return vec![];
        }
        
        // Default question for complex tasks
        questions.push("Are there any special constraints or edge cases I should handle?".to_string());
    }
    
    // Limit to max 3 questions
    questions.truncate(3);
    questions
} 