use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ===== EXECUTION STATE =====

/// Simple execution state for tracking automation progress
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecutionState {
    pub automation_id: i64,
    pub name: String,
    pub objective: String,
    pub status: String,
    pub started_at: String,
    /// High-level progress markers (e.g., "logged in", "saved file")
    pub milestones: Vec<String>,
}

// ===== ACTION TYPES =====

/// Types of actions the agent can perform
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ActionType {
    LaunchApp,
    ClickElement,
    TypeText,
    NavigateURL,
    WaitTime,
    Complete,
    PressKey,
    RequestTakeover,
    AskClarification,
    AllElements,
    MemorySave,
    FullText,
    ExcelType,
    ExcelCommand,      // App-specific Excel commands (AppleScript on macOS, COM on Windows)
    WordCommand,       // App-specific Word commands (AppleScript on macOS, COM on Windows)
    PowerPointCommand, // App-specific PowerPoint commands (COM on Windows)
    Stuck,             // LLM signals it's stuck and needs replanning
    ParseError,        // Used internally when LLM response can't be parsed
    // Terminal execution actions
    TerminalRun,       // Run a command and wait for completion
    TerminalBackground,// Start a long-running process in background
    TerminalCheck,     // Check status of a background process
    TerminalKill,      // Kill a background process
    TerminalRead,      // Read output from a process
    GoogleSearch,      // Quick Google search acceleration (opens Chrome, navigates to results)
}

/// An action decided by the agent
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentAction {
    pub action_type: ActionType,
    pub app_name: Option<String>,
    pub target_element: Option<String>,
    pub parameters: Option<HashMap<String, String>>,
    pub reasoning: String,
}

// ===== APPLICATION STATE =====

/// Max recent actions before compression kicks in
const MAX_RECENT_ACTIONS: usize = 100;
/// How many recent actions to keep after compression
const COMPRESSED_ACTIONS_KEEP: usize = 50;

/// Current application state
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppState {
    pub current_app: Option<String>,
    pub current_pid: Option<String>,
    pub accessible_elements: Vec<String>,
    /// Text content from non-actionable elements (truncated for efficiency)
    pub text_content: Option<String>,
    /// Full text content without truncation (only sent when explicitly requested)
    pub full_text_content: Option<String>,
    /// Word document info (cursor position + paragraph structure) — auto-fetched when Word is active
    #[serde(default)]
    pub word_document_info: Option<String>,
    pub recent_actions: Vec<EnhancedAction>,
    /// Raw element tree for LLM context
    pub element_tree: Option<String>,
    /// Track screen states
    pub content_map: Vec<ScreenState>,
    /// Summary of compressed older actions
    #[serde(default)]
    pub action_history_summary: Option<String>,
}

impl AppState {
    /// Add an action and compress history if needed
    pub fn add_action(&mut self, action: EnhancedAction) {
        self.recent_actions.push(action);
        self.compress_if_needed();
    }

    /// Compress action history if it exceeds the limit
    pub fn compress_if_needed(&mut self) {
        if self.recent_actions.len() <= MAX_RECENT_ACTIONS {
            return;
        }

        // Calculate how many to compress
        let compress_count = self.recent_actions.len() - COMPRESSED_ACTIONS_KEEP;
        let actions_to_compress: Vec<_> = self.recent_actions.drain(..compress_count).collect();

        // Create summary of compressed actions
        let mut summary_parts = Vec::new();

        // Group by action type
        let mut action_counts: HashMap<String, usize> = HashMap::new();
        for action in &actions_to_compress {
            *action_counts.entry(action.action_type.clone()).or_insert(0) += 1;
        }

        // Build summary
        for (action_type, count) in action_counts {
            summary_parts.push(format!("{} x{}", action_type, count));
        }

        let new_summary = format!("Earlier: {}", summary_parts.join(", "));

        // Append to existing summary or create new
        self.action_history_summary = Some(match &self.action_history_summary {
            Some(existing) => format!("{} | {}", existing, new_summary),
            None => new_summary,
        });

        // Limit summary length
        if let Some(ref mut summary) = self.action_history_summary {
            if summary.len() > 500 {
                *summary = format!("{}...", &summary[..500]);
            }
        }
    }

    /// Get formatted recent actions for LLM prompt (command, justification pairs)
    pub fn get_recent_actions_for_prompt(&self) -> Vec<(String, String)> {
        self.recent_actions
            .iter()
            .map(|a| (a.command.clone(), a.justification.clone()))
            .collect()
    }

    /// Get action history summary including compressed history
    pub fn get_action_summary(&self) -> String {
        let mut summary = String::new();

        if let Some(ref compressed) = self.action_history_summary {
            summary.push_str(compressed);
            summary.push('\n');
        }

        for action in self.recent_actions.iter().rev().take(5) {
            summary.push_str(&format!("- {}: {}\n", action.command, action.justification));
        }

        summary
    }
}

/// Enhanced action with justification
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnhancedAction {
    /// e.g., "CLICK:42"
    pub command: String,
    /// e.g., "ClickElement"
    pub action_type: String,
    /// 20-30 word explanation including next 1-2 planned steps
    pub justification: String,
    /// Brief description of what was on screen
    pub screen_context: String,
    pub timestamp: String,
}

/// Screen state tracking
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenState {
    pub app_name: String,
    pub timestamp: String,
    pub element_count: usize,
    /// Top 5-10 most important elements
    pub key_elements: Vec<String>,
    /// e.g., "Page 1 of 10", "Showing 1-50 of 300"
    pub content_indicators: Vec<String>,
    /// e.g., "Next button", "Load more", "Scroll available"
    pub navigation_available: Vec<String>,
}

// ===== LLM INTERACTION =====

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Session-based LLM interaction structures
#[derive(Debug, Clone)]
pub struct LLMSession {
    pub session_id: String,
    pub messages: Vec<Message>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub consecutive_parse_errors: u32,
}

impl LLMSession {
    pub fn new(session_id: String, system_prompt: String, initial_context: String) -> Self {
        let messages = vec![
            Message {
                role: "system".to_string(),
                content: system_prompt,
            },
            Message {
                role: "user".to_string(),
                content: initial_context,
            }
        ];
        
        Self {
            session_id,
            messages,
            total_input_tokens: 0,
            total_output_tokens: 0,
            consecutive_parse_errors: 0,
        }
    }
    
    pub fn add_assistant_response(&mut self, response: String) {
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: response,
        });
    }
    
    pub fn add_user_message(&mut self, message: String) {
        self.messages.push(Message {
            role: "user".to_string(),
            content: message,
        });
    }
    
    pub fn get_messages_for_api(&self) -> Vec<Message> {
        // Skip the system message as it goes in a separate field
        // Implement sliding window to prevent token explosion
        
        // Keep only the last N messages (e.g., last 4 = 2 user/assistant pairs)
        const MAX_MESSAGES: usize = 2;
        
        // Handle MAX_MESSAGES = 0 specially: always return just the most recent message
        let start_idx = if MAX_MESSAGES == 0 && self.messages.len() > 1 {
            // Return only the most recent message (which should be a user message)
            self.messages.len() - 1
        } else if self.messages.len() > MAX_MESSAGES + 1 {
            // Normal sliding window logic
            // +1 to account for system message at index 0
            self.messages.len() - MAX_MESSAGES
        } else {
            1 // Skip system message
        };
        
        self.messages[start_idx..].to_vec()
    }
    
    pub fn get_system_prompt(&self) -> String {
        // The first message is always the system prompt
        self.messages[0].content.clone()
    }
}

// ===== METRICS =====

/// Metrics for tracking refresh effectiveness
#[derive(Default, Debug)]
pub struct RefreshMetrics {
    pub event_driven_refreshes: u32,
    pub low_count_refreshes: u32,
    pub successful_refreshes: u32,
    pub failed_refreshes: u32,
} 