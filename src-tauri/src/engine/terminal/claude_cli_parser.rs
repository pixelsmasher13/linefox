//! Claude CLI JSONL output parser
//! 
//! Parses the streaming JSONL output from Claude CLI and extracts:
//! - Thinking/reasoning content
//! - Text content (assistant response)
//! - Tool use (bash commands, file operations, etc.)
//! - Tool results
//!
//! Claude CLI outputs events in this format:
//! - `{"type":"system","subtype":"init",...}` - Initialization
//! - `{"type":"stream_event","event":{...}}` - Wrapped streaming events
//! - `{"type":"assistant","message":{...}}` - Complete assistant message
//! - `{"type":"result","subtype":"success",...}` - Final result

use log::{debug, trace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ===== CLAUDE CLI WRAPPER TYPES =====

/// Top-level Claude CLI event wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCliEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub subtype: Option<String>,
    /// For stream_event type, contains the inner streaming event
    #[serde(default)]
    pub event: Option<InnerStreamEvent>,
    /// For assistant type, contains the full message
    #[serde(default)]
    pub message: Option<AssistantMessage>,
    /// For result type, contains the final result text
    #[serde(default)]
    pub result: Option<String>,
    /// Session ID
    #[serde(default)]
    pub session_id: Option<String>,
    /// For system init, contains available tools
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    /// For system init, contains the model
    #[serde(default)]
    pub model: Option<String>,
    /// Error information
    #[serde(default)]
    pub error: Option<serde_json::Value>,
}

/// Assistant message with content blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

// ===== INNER STREAMING EVENT TYPES =====

/// Inner streaming event (unwrapped from stream_event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerStreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub index: Option<usize>,
    #[serde(default)]
    pub content_block: Option<ContentBlock>,
    #[serde(default)]
    pub delta: Option<ContentDelta>,
    #[serde(default)]
    pub message: Option<serde_json::Value>,
}

/// Types of content blocks in Claude's output
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContentBlockType {
    Text,
    Thinking,
    ToolUse,
    ToolResult,
    #[serde(other)]
    Unknown,
}

/// A content block from Claude's response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
}

/// Delta update for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentDelta {
    #[serde(rename = "type")]
    pub delta_type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub partial_json: Option<String>,
}

/// A streaming event from Claude CLI (legacy, kept for compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeStreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub index: Option<usize>,
    #[serde(default)]
    pub content_block: Option<ContentBlock>,
    #[serde(default)]
    pub delta: Option<ContentDelta>,
    #[serde(default)]
    pub message: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<serde_json::Value>,
}

/// Structured output from parsing Claude CLI events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeStreamOutput {
    /// Type of the parsed output
    pub output_type: ClaudeOutputType,
    /// Index of the content block (if applicable)
    pub block_index: Option<usize>,
    /// The content/data
    pub content: String,
    /// Tool name (for tool_use events)
    pub tool_name: Option<String>,
    /// Tool ID (for tool_use events)
    pub tool_id: Option<String>,
    /// Tool input (for tool_use events)
    pub tool_input: Option<serde_json::Value>,
    /// Whether this is a delta (partial) update
    pub is_delta: bool,
}

/// Type of Claude output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeOutputType {
    Thinking,
    ThinkingDelta,
    Content,
    ContentDelta,
    ToolStart,
    ToolInputDelta,
    ToolEnd,
    ToolResult,
    MessageStart,
    MessageEnd,
    Error,
    Unknown,
}

/// State for accumulating streaming content
#[derive(Debug, Default)]
pub struct ClaudeStreamState {
    /// Accumulated thinking text
    pub thinking: String,
    /// Accumulated content text
    pub content: String,
    /// Active tool calls by ID
    pub tool_calls: HashMap<String, ToolCallState>,
    /// Current block types by index
    block_types: HashMap<usize, String>,
    /// Accumulated tool input JSON strings by block index
    tool_input_buffers: HashMap<usize, String>,
}

/// State of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallState {
    pub id: String,
    pub name: String,
    pub input: Option<serde_json::Value>,
    pub status: ToolCallStatus,
    pub result: Option<String>,
}

/// Status of a tool call
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Started,
    Running,
    Completed,
    Error,
}

impl ClaudeStreamState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the state for a new conversation
    pub fn reset(&mut self) {
        self.thinking.clear();
        self.content.clear();
        self.tool_calls.clear();
        self.block_types.clear();
        self.tool_input_buffers.clear();
    }

    /// Process a streaming event and update state
    pub fn process_event(&mut self, event: &ClaudeStreamEvent) -> Option<ClaudeStreamOutput> {
        match event.event_type.as_str() {
            "message_start" => {
                self.reset();
                Some(ClaudeStreamOutput {
                    output_type: ClaudeOutputType::MessageStart,
                    block_index: None,
                    content: String::new(),
                    tool_name: None,
                    tool_id: None,
                    tool_input: None,
                    is_delta: false,
                })
            }
            
            "content_block_start" => {
                self.handle_content_block_start(event)
            }
            
            "content_block_delta" => {
                self.handle_content_block_delta(event)
            }
            
            "content_block_stop" => {
                self.handle_content_block_stop(event)
            }
            
            "message_delta" | "message_stop" => {
                Some(ClaudeStreamOutput {
                    output_type: ClaudeOutputType::MessageEnd,
                    block_index: None,
                    content: String::new(),
                    tool_name: None,
                    tool_id: None,
                    tool_input: None,
                    is_delta: false,
                })
            }
            
            "error" => {
                let error_msg = event.error
                    .as_ref()
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                    
                Some(ClaudeStreamOutput {
                    output_type: ClaudeOutputType::Error,
                    block_index: None,
                    content: error_msg,
                    tool_name: None,
                    tool_id: None,
                    tool_input: None,
                    is_delta: false,
                })
            }
            
            _ => {
                trace!("Unknown Claude event type: {}", event.event_type);
                None
            }
        }
    }

    fn handle_content_block_start(&mut self, event: &ClaudeStreamEvent) -> Option<ClaudeStreamOutput> {
        let index = event.index?;
        let block = event.content_block.as_ref()?;
        
        // Store the block type for later delta processing
        self.block_types.insert(index, block.block_type.clone());
        
        match block.block_type.as_str() {
            "thinking" => {
                let text = block.thinking.as_deref().unwrap_or("");
                if !text.is_empty() {
                    self.thinking.push_str(text);
                }
                Some(ClaudeStreamOutput {
                    output_type: ClaudeOutputType::Thinking,
                    block_index: Some(index),
                    content: text.to_string(),
                    tool_name: None,
                    tool_id: None,
                    tool_input: None,
                    is_delta: false,
                })
            }
            
            "text" => {
                let text = block.text.as_deref().unwrap_or("");
                if !text.is_empty() {
                    self.content.push_str(text);
                }
                Some(ClaudeStreamOutput {
                    output_type: ClaudeOutputType::Content,
                    block_index: Some(index),
                    content: text.to_string(),
                    tool_name: None,
                    tool_id: None,
                    tool_input: None,
                    is_delta: false,
                })
            }
            
            "tool_use" => {
                let tool_id = block.id.clone().unwrap_or_default();
                let tool_name = block.name.clone().unwrap_or_default();
                
                // Initialize tool input buffer for this block
                self.tool_input_buffers.insert(index, String::new());
                
                let tool_state = ToolCallState {
                    id: tool_id.clone(),
                    name: tool_name.clone(),
                    input: block.input.clone(),
                    status: ToolCallStatus::Started,
                    result: None,
                };
                
                self.tool_calls.insert(tool_id.clone(), tool_state);
                
                Some(ClaudeStreamOutput {
                    output_type: ClaudeOutputType::ToolStart,
                    block_index: Some(index),
                    content: String::new(),
                    tool_name: Some(tool_name),
                    tool_id: Some(tool_id),
                    tool_input: block.input.clone(),
                    is_delta: false,
                })
            }
            
            _ => None,
        }
    }

    fn handle_content_block_delta(&mut self, event: &ClaudeStreamEvent) -> Option<ClaudeStreamOutput> {
        let index = event.index?;
        let delta = event.delta.as_ref()?;
        
        // Get the block type from our stored state
        let _block_type = self.block_types.get(&index).cloned();
        
        match delta.delta_type.as_str() {
            "thinking_delta" => {
                let text = delta.thinking.as_deref().unwrap_or("");
                if !text.is_empty() {
                    self.thinking.push_str(text);
                }
                Some(ClaudeStreamOutput {
                    output_type: ClaudeOutputType::ThinkingDelta,
                    block_index: Some(index),
                    content: text.to_string(),
                    tool_name: None,
                    tool_id: None,
                    tool_input: None,
                    is_delta: true,
                })
            }
            
            "text_delta" => {
                let text = delta.text.as_deref().unwrap_or("");
                if !text.is_empty() {
                    self.content.push_str(text);
                }
                Some(ClaudeStreamOutput {
                    output_type: ClaudeOutputType::ContentDelta,
                    block_index: Some(index),
                    content: text.to_string(),
                    tool_name: None,
                    tool_id: None,
                    tool_input: None,
                    is_delta: true,
                })
            }
            
            "input_json_delta" => {
                // Accumulate partial JSON for tool input
                let partial = delta.partial_json.as_deref().unwrap_or("");
                if !partial.is_empty() {
                    if let Some(buffer) = self.tool_input_buffers.get_mut(&index) {
                        buffer.push_str(partial);
                    }
                }
                
                // Find the tool call for this block
                let tool_info = self.find_tool_for_block(index);
                
                Some(ClaudeStreamOutput {
                    output_type: ClaudeOutputType::ToolInputDelta,
                    block_index: Some(index),
                    content: partial.to_string(),
                    tool_name: tool_info.as_ref().map(|t| t.0.clone()),
                    tool_id: tool_info.as_ref().map(|t| t.1.clone()),
                    tool_input: None,
                    is_delta: true,
                })
            }
            
            _ => {
                trace!("Unknown delta type: {}", delta.delta_type);
                None
            }
        }
    }

    fn handle_content_block_stop(&mut self, event: &ClaudeStreamEvent) -> Option<ClaudeStreamOutput> {
        let index = event.index?;
        let block_type = self.block_types.get(&index)?;
        
        match block_type.as_str() {
            "tool_use" => {
                // Parse the accumulated JSON input
                let input = self.tool_input_buffers
                    .get(&index)
                    .and_then(|s| serde_json::from_str(s).ok());
                
                // Update the tool call with parsed input and mark as running
                if let Some(tool_info) = self.find_tool_for_block(index) {
                    if let Some(tool) = self.tool_calls.get_mut(&tool_info.1) {
                        if input.is_some() {
                            tool.input = input.clone();
                        }
                        tool.status = ToolCallStatus::Running;
                    }
                    
                    return Some(ClaudeStreamOutput {
                        output_type: ClaudeOutputType::ToolEnd,
                        block_index: Some(index),
                        content: String::new(),
                        tool_name: Some(tool_info.0),
                        tool_id: Some(tool_info.1),
                        tool_input: input,
                        is_delta: false,
                    });
                }
                None
            }
            
            _ => None,
        }
    }

    /// Find tool name and ID for a given block index
    #[allow(dead_code)]
    fn find_tool_for_block(&self, _block_index: usize) -> Option<(String, String)> {
        // Look through tool calls to find one that matches this block
        // In practice, we track this by the order tools appear
        for (id, tool) in &self.tool_calls {
            if tool.status == ToolCallStatus::Started || tool.status == ToolCallStatus::Running {
                return Some((tool.name.clone(), id.clone()));
            }
        }
        None
    }

    /// Mark a tool call as completed with a result
    #[allow(dead_code)]
    pub fn complete_tool(&mut self, tool_id: &str, result: String) {
        if let Some(tool) = self.tool_calls.get_mut(tool_id) {
            tool.status = ToolCallStatus::Completed;
            tool.result = Some(result);
        }
    }

    /// Mark a tool call as errored
    #[allow(dead_code)]
    pub fn error_tool(&mut self, tool_id: &str, error: String) {
        if let Some(tool) = self.tool_calls.get_mut(tool_id) {
            tool.status = ToolCallStatus::Error;
            tool.result = Some(error);
        }
    }
}

/// Parse a single line of Claude CLI JSONL output
/// Returns the unwrapped inner event if it's a stream_event wrapper
pub fn parse_claude_jsonl_line(line: &str) -> Option<ClaudeStreamEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    
    // Skip non-JSON lines (like status messages)
    if !trimmed.starts_with('{') {
        return None;
    }
    
    // First try to parse as Claude CLI wrapper format
    if let Ok(cli_event) = serde_json::from_str::<ClaudeCliEvent>(trimmed) {
        return convert_cli_event_to_stream_event(cli_event);
    }
    
    // Fallback: try to parse as raw stream event
    match serde_json::from_str::<ClaudeStreamEvent>(trimmed) {
        Ok(event) => {
            debug!("Parsed raw Claude event: type={}", event.event_type);
            Some(event)
        }
        Err(e) => {
            trace!("Failed to parse Claude JSONL: {} - line: {}", e, trimmed);
            None
        }
    }
}

/// Convert Claude CLI wrapper event to internal stream event format
fn convert_cli_event_to_stream_event(cli_event: ClaudeCliEvent) -> Option<ClaudeStreamEvent> {
    match cli_event.event_type.as_str() {
        "system" => {
            // System init event - treat as message_start
            debug!("Claude CLI system event: subtype={:?}, model={:?}", 
                   cli_event.subtype, cli_event.model);
            Some(ClaudeStreamEvent {
                event_type: "message_start".to_string(),
                index: None,
                content_block: None,
                delta: None,
                message: None,
                error: None,
            })
        }
        
        "stream_event" => {
            // Unwrap the inner event
            if let Some(inner) = cli_event.event {
                debug!("Claude CLI stream_event: inner_type={}", inner.event_type);
                Some(ClaudeStreamEvent {
                    event_type: inner.event_type,
                    index: inner.index,
                    content_block: inner.content_block,
                    delta: inner.delta,
                    message: inner.message,
                    error: None,
                })
            } else {
                None
            }
        }
        
        "assistant" => {
            // Full assistant message - extract content
            if let Some(msg) = cli_event.message {
                debug!("Claude CLI assistant message received");
                // We'll emit content from the message
                // Create a synthetic event with the full content
                if !msg.content.is_empty() {
                    // Just mark message end, content was already streamed
                    Some(ClaudeStreamEvent {
                        event_type: "message_stop".to_string(),
                        index: None,
                        content_block: None,
                        delta: None,
                        message: None,
                        error: None,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        }
        
        "result" => {
            // Final result event
            debug!("Claude CLI result: subtype={:?}", cli_event.subtype);
            Some(ClaudeStreamEvent {
                event_type: "message_stop".to_string(),
                index: None,
                content_block: None,
                delta: None,
                message: None,
                error: None,
            })
        }
        
        "error" => {
            Some(ClaudeStreamEvent {
                event_type: "error".to_string(),
                index: None,
                content_block: None,
                delta: None,
                message: None,
                error: cli_event.error,
            })
        }
        
        _ => {
            trace!("Unknown Claude CLI event type: {}", cli_event.event_type);
            None
        }
    }
}

/// Check if a line looks like Claude CLI JSONL output
pub fn is_claude_jsonl(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return false;
    }
    
    // Quick heuristic: check for Claude CLI wrapper types or inner event types
    trimmed.contains("\"type\"") && (
        // Claude CLI wrapper types
        trimmed.contains("\"system\"") ||
        trimmed.contains("\"stream_event\"") ||
        trimmed.contains("\"assistant\"") ||
        trimmed.contains("\"result\"") ||
        // Inner streaming event types
        trimmed.contains("message_start") ||
        trimmed.contains("content_block") ||
        trimmed.contains("message_delta") ||
        trimmed.contains("message_stop") ||
        trimmed.contains("\"error\"")
    )
}

/// Check if a command is a Claude CLI command
pub fn is_claude_cli_command(command: &str) -> bool {
    let cmd = command.trim();
    cmd.starts_with("claude ") ||
    cmd.starts_with("claude\t") ||
    cmd == "claude" ||
    cmd.contains(" claude ") ||
    cmd.contains("/claude ")
}

/// Extract the session_id from a raw Claude CLI JSONL line.
/// Claude CLI includes `"session_id"` on every event; we read it directly from
/// the wrapper (`ClaudeCliEvent`) before the converter discards it.
pub fn extract_session_id_from_jsonl(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    serde_json::from_str::<ClaudeCliEvent>(trimmed)
        .ok()
        .and_then(|e| e.session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cli_system_init() {
        let line = r#"{"type":"system","subtype":"init","session_id":"123","tools":["Bash","Read"],"model":"claude-sonnet-4-20250514"}"#;
        let event = parse_claude_jsonl_line(line).unwrap();
        assert_eq!(event.event_type, "message_start");
    }

    #[test]
    fn test_parse_cli_stream_event_text_delta() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}},"session_id":"123"}"#;
        let event = parse_claude_jsonl_line(line).unwrap();
        assert_eq!(event.event_type, "content_block_delta");
        assert!(event.delta.is_some());
        let delta = event.delta.unwrap();
        assert_eq!(delta.delta_type, "text_delta");
        assert_eq!(delta.text.unwrap(), "Hello");
    }

    #[test]
    fn test_parse_cli_stream_event_content_block_start() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"session_id":"123"}"#;
        let event = parse_claude_jsonl_line(line).unwrap();
        assert_eq!(event.event_type, "content_block_start");
        assert!(event.content_block.is_some());
    }

    #[test]
    fn test_parse_cli_result() {
        let line = r#"{"type":"result","subtype":"success","result":"Hello world","session_id":"123"}"#;
        let event = parse_claude_jsonl_line(line).unwrap();
        assert_eq!(event.event_type, "message_stop");
    }

    #[test]
    fn test_parse_raw_text_delta() {
        // Also support raw format for backwards compatibility
        let line = r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hello!"}}"#;
        let event = parse_claude_jsonl_line(line).unwrap();
        let delta = event.delta.unwrap();
        assert_eq!(delta.delta_type, "text_delta");
        assert_eq!(delta.text.unwrap(), "Hello!");
    }

    #[test]
    fn test_is_claude_jsonl() {
        // Claude CLI wrapper types
        assert!(is_claude_jsonl(r#"{"type":"system","subtype":"init"}"#));
        assert!(is_claude_jsonl(r#"{"type":"stream_event","event":{}}"#));
        assert!(is_claude_jsonl(r#"{"type":"assistant","message":{}}"#));
        assert!(is_claude_jsonl(r#"{"type":"result","subtype":"success"}"#));
        // Inner event types
        assert!(is_claude_jsonl(r#"{"type":"content_block_delta"}"#));
        // Non-Claude
        assert!(!is_claude_jsonl("Hello world"));
        assert!(!is_claude_jsonl(""));
        assert!(!is_claude_jsonl(r#"{"foo":"bar"}"#));
    }

    #[test]
    fn test_is_claude_cli_command() {
        assert!(is_claude_cli_command("claude"));
        assert!(is_claude_cli_command("claude --help"));
        assert!(is_claude_cli_command("claude --print --output-format stream-json"));
        assert!(is_claude_cli_command("claude chat"));
        assert!(!is_claude_cli_command("echo hello"));
        assert!(!is_claude_cli_command("claudette"));
    }

    #[test]
    fn test_stream_state_accumulation() {
        let mut state = ClaudeStreamState::new();
        
        // Simulate content block start
        let event1 = ClaudeStreamEvent {
            event_type: "content_block_start".to_string(),
            index: Some(0),
            content_block: Some(ContentBlock {
                block_type: "text".to_string(),
                text: Some("".to_string()),
                thinking: None,
                id: None,
                name: None,
                input: None,
            }),
            delta: None,
            message: None,
            error: None,
        };
        
        state.process_event(&event1);
        
        // Add text delta
        let event2 = ClaudeStreamEvent {
            event_type: "content_block_delta".to_string(),
            index: Some(0),
            content_block: None,
            delta: Some(ContentDelta {
                delta_type: "text_delta".to_string(),
                text: Some("Hello".to_string()),
                thinking: None,
                partial_json: None,
            }),
            message: None,
            error: None,
        };
        
        state.process_event(&event2);
        assert_eq!(state.content, "Hello");
        
        // Add more text
        let event3 = ClaudeStreamEvent {
            event_type: "content_block_delta".to_string(),
            index: Some(0),
            content_block: None,
            delta: Some(ContentDelta {
                delta_type: "text_delta".to_string(),
                text: Some(" World".to_string()),
                thinking: None,
                partial_json: None,
            }),
            message: None,
            error: None,
        };
        
        state.process_event(&event3);
        assert_eq!(state.content, "Hello World");
    }
}
