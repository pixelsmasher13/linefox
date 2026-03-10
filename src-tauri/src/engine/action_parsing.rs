use log::{info, warn, error};
use std::collections::HashMap;

use crate::engine::types::{ActionType, AgentAction};

// Platform-specific element cache access
#[cfg(target_os = "macos")]
use crate::window_details_collector::macos::macos_action_detector_engine::ActionableElement;

/// Get the current app state from the global state
/// This is a helper to avoid circular dependencies
fn get_app_state() -> Option<crate::engine::types::AppState> {
    super::automation_agent_engine::get_current_app_state()
}

/// Get element by index from the cache
/// This delegates to the main engine's cache management
#[cfg(target_os = "macos")]
fn get_element_by_index(index: usize) -> Option<ActionableElement> {
    super::automation_agent_engine::get_element_by_index_public(index)
}

#[cfg(target_os = "windows")]
fn get_element_by_index(index: usize) -> Option<serde_json::Value> {
    super::automation_agent_engine::get_element_by_index_public(index)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn get_element_by_index(_index: usize) -> Option<serde_json::Value> {
    None
}

// List of Excel-specific AppleScript commands
const EXCEL_COMMANDS: &[&str] = &[
    "EXCEL_RENAME_SHEET:",
    "EXCEL_NEW_SHEET:",
    "EXCEL_DELETE_SHEET:",
    "EXCEL_SELECT_SHEET:",
    "EXCEL_SET_CELL:",
    "EXCEL_SET_FORMULA:",
    "EXCEL_FORMAT_CELLS:",
    "EXCEL_AUTOFIT_COLUMNS:",
    "EXCEL_SET_COLUMN_WIDTH:",
    "EXCEL_BOLD:",
    "EXCEL_ITALIC:",
    "EXCEL_UNDERLINE:",
    "EXCEL_SET_FONT:",
    "EXCEL_SET_FONT_SIZE:",
    "EXCEL_SET_FILL_COLOR:",
    "EXCEL_SET_FONT_COLOR:",
    "EXCEL_WRAP_TEXT:",
    "EXCEL_ADD_BORDER:",
    "EXCEL_ALIGN:",
    "EXCEL_VERTICAL_ALIGN:",
    "EXCEL_MERGE_CELLS:",
    "EXCEL_UNMERGE_CELLS:",
    "EXCEL_SET_ROW_HEIGHT:",
    "EXCEL_CLEAR_RANGE:",
    "EXCEL_COPY_RANGE:",
    "EXCEL_GET_CELL_VALUE:",
    "EXCEL_GET_RANGE_VALUES:",
    "EXCEL_GET_FORMULA:",
    "EXCEL_GET_RANGE_FORMULAS:",
    "EXCEL_GET_SHEET_INFO",
    "EXCEL_GET_STRUCTURE:",
];

// List of Word-specific commands (AppleScript on macOS, COM on Windows)
const WORD_COMMANDS: &[&str] = &[
    "WORD_NEW_DOCUMENT",
    "WORD_OPEN:",
    "WORD_CLOSE",
    "WORD_INSERT_TEXT:",
    "WORD_INSERT_PARAGRAPH:",
    "WORD_FIND_REPLACE:",
    "WORD_DELETE",
    "WORD_SET_FONT:",
    "WORD_SET_FONT_SIZE:",
    "WORD_BOLD",
    "WORD_ITALIC",
    "WORD_UNDERLINE",
    "WORD_ALIGN:",
    "WORD_INSERT_TABLE:",
    "WORD_GET_TEXT",
    "WORD_GET_SELECTION",
    "WORD_GET_FORMATTING",
    "WORD_GET_SELECTION_FORMAT",
    "WORD_GET_WORD_COUNT",
    "WORD_GET_DOCUMENT_INFO",
    "WORD_SELECT_ALL",
    "WORD_SELECT_TEXT:",
    "WORD_SELECT_PARAGRAPH:",
    "WORD_SELECT_BETWEEN:",
    "WORD_SELECT_TABLE:",
    "WORD_MOVE_AFTER_TEXT:",
    // Cursor positioning (Windows COM)
    "WORD_DESELECT",
    "WORD_GOTO_START",
    "WORD_GOTO_END",
    "WORD_SET_CURSOR:",
    "WORD_INSERT_AT_START:",
    "WORD_INSERT_AT_END:",
    "WORD_INSERT_AFTER:",
    "WORD_INSERT_BEFORE:",
];

// List of PowerPoint-specific COM commands (Windows only)
const POWERPOINT_COMMANDS: &[&str] = &[
    "POWERPOINT_NEW_PRESENTATION",
    "POWERPOINT_SAVE",
    "POWERPOINT_SAVE_AS:",
    "POWERPOINT_ADD_SLIDE:",
    "POWERPOINT_GO_TO_SLIDE:",
    "POWERPOINT_DELETE_SLIDE:",
    "POWERPOINT_DUPLICATE_SLIDE",
    "POWERPOINT_GET_SLIDE_COUNT",
    "POWERPOINT_SET_TITLE:",
    "POWERPOINT_SET_SUBTITLE:",
    "POWERPOINT_SET_BODY:",
    "POWERPOINT_ADD_NOTE:",
    "POWERPOINT_ADD_TABLE:",
    "POWERPOINT_SET_TABLE_CELL:",
    "POWERPOINT_SET_TABLE_ROW:",
    "POWERPOINT_APPLY_THEME:",
    "POWERPOINT_SET_BACKGROUND_COLOR:",
    "POWERPOINT_FORMAT_TITLE:",
    "POWERPOINT_FORMAT_BODY:",
    "POWERPOINT_SET_FONT:",
    "POWERPOINT_ALIGN_TEXT:",
    "POWERPOINT_ADD_SMARTART:",
    "POWERPOINT_ADD_CHART:",
    "POWERPOINT_ADD_SLIDE_NUMBERS",
    "POWERPOINT_SET_TRANSITION:",
    "POWERPOINT_SET_ALL_TRANSITIONS:",
    "POWERPOINT_GET_SLIDE_INFO",
    "POWERPOINT_GET_PRESENTATION_INFO",
];

/// Check if the action contains any Excel-specific command
fn contains_excel_command(action: &str) -> bool {
    EXCEL_COMMANDS.iter().any(|cmd| action.contains(cmd))
}

/// Check if the action contains any Word-specific command
fn contains_word_command(action: &str) -> bool {
    WORD_COMMANDS.iter().any(|cmd| action.contains(cmd))
}

/// Check if the action contains any PowerPoint-specific command
fn contains_powerpoint_command(action: &str) -> bool {
    POWERPOINT_COMMANDS.iter().any(|cmd| action.contains(cmd))
}

// Terminal command patterns
const TERMINAL_COMMANDS: &[&str] = &[
    "TERMINAL_RUN:",
    "TERMINAL_BACKGROUND:",
    "TERMINAL_CHECK:",
    "TERMINAL_KILL:",
    "TERMINAL_READ:",
];

/// Check if the action contains any terminal command
fn contains_terminal_command(action: &str) -> bool {
    TERMINAL_COMMANDS.iter().any(|cmd| action.contains(cmd))
}

/// Parse the LLM response into an AgentAction
pub fn parse_agent_action(action_json: &str) -> Result<AgentAction, String> {
    // First try to parse as simple command format like "CLICK:5 | justification" or "LAUNCH:Safari | justification"
    if action_json.contains("CLICK:") || action_json.contains("LAUNCH:") ||
       action_json.contains("TYPE:") || action_json.contains("WAIT:") ||
       action_json.contains("PRESS:") || action_json.contains("COMPLETE") ||
       action_json.contains("REQUEST_TAKEOVER:") || action_json.contains("ASK_CLARIFICATION:") ||
       action_json.contains("ALL_ELEMENTS") || action_json.contains("FULL_TEXT") ||
       action_json.contains("MEMORY_SAVE:") || action_json.contains("URL:") ||
       action_json.contains("EXCEL_TYPE:") || action_json.contains("STUCK") ||
       contains_excel_command(action_json) ||
       contains_word_command(action_json) || contains_powerpoint_command(action_json) ||
       contains_terminal_command(action_json) || action_json.contains("GOOGLE_SEARCH:") {

        // Special handling for MEMORY_SAVE to preserve multi-line content
        if action_json.contains("MEMORY_SAVE:") {
            // Find the MEMORY_SAVE: prefix
            if let Some(mem_start) = action_json.find("MEMORY_SAVE:") {
                let after_prefix = &action_json[mem_start + 12..]; // Skip "MEMORY_SAVE:"

                // For MEMORY_SAVE, the format is: MEMORY_SAVE:content || justification
                // The memory content can be multi-line, but it should end at the LAST " || "
                // Since memory content might contain " || ", we use rfind to get the last one
                let (mem_content, justification) = if let Some(pipe_pos) = after_prefix.rfind(" || ") {
                    // Split at the last " || " - everything before is memory, everything after is justification
                    let memory_part = after_prefix[..pipe_pos].trim();
                    let justification_part = after_prefix[pipe_pos + 4..].trim(); // Skip " || "

                    // The memory content should not include the justification
                    (memory_part.to_string(), justification_part.to_string())
                } else {
                    // No separator found, treat entire content as memory
                    (after_prefix.trim().to_string(), "Storing memory for later use".to_string())
                };

                let mut params = HashMap::new();
                params.insert("memory".to_string(), mem_content.to_string());

                return Ok(AgentAction {
                    action_type: ActionType::MemorySave,
                    app_name: None,
                    target_element: None,
                    parameters: Some(params),
                    reasoning: justification,
                });
            }
        }

        // Special handling for commands with potentially long/multi-line text content
        // We need to find the LAST " || " in the entire response because the text content may contain newlines
        // This applies to: TYPE, WORD_INSERT_TEXT, WORD_INSERT_PARAGRAPH, WORD_SELECT_TEXT, and similar text-heavy commands
        let trimmed = action_json.trim();
        let needs_rfind_parsing = trimmed.starts_with("TYPE:") ||
                                  trimmed.starts_with("WORD_INSERT_TEXT:") ||
                                  trimmed.starts_with("WORD_INSERT_PARAGRAPH:") ||
                                  trimmed.starts_with("WORD_SELECT_TEXT:") ||
                                  trimmed.starts_with("WORD_MOVE_AFTER_TEXT:") ||
                                  trimmed.starts_with("WORD_FIND_REPLACE:") ||
                                  trimmed.starts_with("WORD_SELECT_BETWEEN:") ||
                                  trimmed.starts_with("EXCEL_TYPE:");

        if needs_rfind_parsing {
            let (full_command, full_justification) = if let Some(pipe_pos) = action_json.rfind(" || ") {
                (action_json[..pipe_pos].trim(), action_json[pipe_pos + 4..].trim().to_string())
            } else {
                (action_json.trim(), String::new())
            };

            // If reasoning is missing, warn but don't fail — use a default
            let full_justification = if full_justification.is_empty() {
                warn!("LLM response missing ' || ' reasoning separator: {}",
                    full_command.lines().next().unwrap_or(full_command));
                format!("Executing: {}", full_command.chars().take(50).collect::<String>())
            } else {
                full_justification
            };

            // Route to appropriate parser based on command type
            if trimmed.starts_with("TYPE:") {
                let command_part = full_command.trim_start_matches("TYPE:").trim();
                let is_multi_element = command_part.contains(":::");
                if is_multi_element {
                    return parse_multi_element_type(command_part, full_justification);
                } else {
                    return parse_single_element_type(command_part, full_justification);
                }
            } else if trimmed.starts_with("WORD_INSERT_TEXT:") ||
                      trimmed.starts_with("WORD_INSERT_PARAGRAPH:") ||
                      trimmed.starts_with("WORD_SELECT_TEXT:") ||
                      trimmed.starts_with("WORD_MOVE_AFTER_TEXT:") ||
                      trimmed.starts_with("WORD_FIND_REPLACE:") ||
                      trimmed.starts_with("WORD_SELECT_BETWEEN:") {
                // Parse Word commands with potentially long text content
                return parse_word_applescript_command(full_command, full_justification)
                    .unwrap_or_else(|| Err(format!("Failed to parse Word command: {}", full_command)));
            } else if trimmed.starts_with("EXCEL_TYPE:") {
                return parse_excel_type_command(full_command, full_justification);
            }
        }

        // Simple parsing for command format
        let lines: Vec<&str> = action_json.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();

        for line in lines {
            // Parse justification if present (using double pipe separator)
            let (command_part, justification) = if let Some(pipe_index) = line.find(" || ") {
                (line[..pipe_index].trim(), line[pipe_index + 4..].trim().to_string())
            } else {
                (line, String::new())
            };

            // Validate that reasoning is included for all commands (except ALL_ELEMENTS and FULL_TEXT which have acceptable defaults)
            // Commands that don't require reasoning validation
            let skip_validation = command_part.starts_with("ALL_ELEMENTS") ||
                                 command_part.contains("ALL_ELEMENTS") ||
                                 command_part.starts_with("FULL_TEXT") ||
                                 command_part.contains("FULL_TEXT");

            // If reasoning is missing, warn but don't fail — use the command as-is
            // with a default reasoning to avoid killing the automation over formatting
            let justification = if !skip_validation && justification.is_empty() {
                warn!("LLM response missing ' || ' reasoning separator: {}", line);
                format!("Executing: {}", command_part.chars().take(50).collect::<String>())
            } else {
                justification
            };

            if command_part.starts_with("ALL_ELEMENTS") || command_part.contains("ALL_ELEMENTS") {
                let reasoning = if !justification.is_empty() {
                    justification
                } else {
                    "Need to see all available UI elements".to_string()
                };

                return Ok(AgentAction {
                    action_type: ActionType::AllElements,
                    app_name: None,
                    target_element: None,
                    parameters: None,
                    reasoning,
                });
            }
            else if command_part.starts_with("FULL_TEXT") || command_part.contains("FULL_TEXT") {
                let reasoning = if !justification.is_empty() {
                    justification
                } else {
                    "Need to see complete text content from the page".to_string()
                };

                return Ok(AgentAction {
                    action_type: ActionType::FullText,
                    app_name: None,
                    target_element: None,
                    parameters: None,
                    reasoning,
                });
            }
            else if command_part.starts_with("CLICK:") {
                return parse_click_command(command_part, justification);
            }
            else if command_part.starts_with("LAUNCH:") {
                let app_name = command_part.replace("LAUNCH:", "").trim().to_string();
                let reasoning = if !justification.is_empty() {
                    justification
                } else {
                    format!("Launching application '{}'", app_name)
                };

                return Ok(AgentAction {
                    action_type: ActionType::LaunchApp,
                    app_name: Some(app_name.clone()),
                    target_element: None,
                    parameters: None,
                    reasoning,
                });
            }
            else if command_part.starts_with("TYPE:") {
                return parse_type_command(command_part, justification);
            }
            else if command_part.starts_with("PRESS:") {
                return parse_press_command(command_part, justification);
            }
            else if command_part.starts_with("WAIT:") {
                let wait_time = command_part.replace("WAIT:", "").trim().to_string();
                let mut params = HashMap::new();
                params.insert("wait_time".to_string(), wait_time.clone());

                let reasoning = if !justification.is_empty() {
                    justification
                } else {
                    format!("Waiting for {} seconds", wait_time)
                };

                return Ok(AgentAction {
                    action_type: ActionType::WaitTime,
                    app_name: None,
                    target_element: None,
                    parameters: Some(params),
                    reasoning,
                });
            }
            else if command_part.starts_with("COMPLETE") || command_part.contains("COMPLETE") {
                let reasoning = if !justification.is_empty() {
                    justification
                } else {
                    "Marking current task as completed".to_string()
                };

                return Ok(AgentAction {
                    action_type: ActionType::Complete,
                    app_name: None,
                    target_element: None,
                    parameters: None,
                    reasoning,
                });
            }
            else if command_part.starts_with("STUCK") || command_part.contains("STUCK") {
                // STUCK command - LLM signals it's stuck and needs replanning
                let reasoning = if !justification.is_empty() {
                    justification
                } else {
                    "Stuck and unable to make progress with current plan".to_string()
                };

                return Ok(AgentAction {
                    action_type: ActionType::Stuck,
                    app_name: None,
                    target_element: None,
                    parameters: None,
                    reasoning,
                });
            }
            else if command_part.starts_with("REQUEST_TAKEOVER:") {
                let instructions = command_part.replace("REQUEST_TAKEOVER:", "").trim().to_string();

                let mut params = HashMap::new();
                params.insert("instructions".to_string(), instructions.clone());

                let reasoning = if !justification.is_empty() {
                    justification
                } else {
                    "Manual user intervention required".to_string()
                };

                return Ok(AgentAction {
                    action_type: ActionType::RequestTakeover,
                    app_name: None,
                    target_element: None,
                    parameters: Some(params),
                    reasoning,
                });
            }
            else if command_part.starts_with("ASK_CLARIFICATION:") {
                let question = command_part.replace("ASK_CLARIFICATION:", "").trim().to_string();

                let mut params = HashMap::new();
                params.insert("question".to_string(), question.clone());

                let reasoning = if !justification.is_empty() {
                    justification
                } else {
                    "Need clarification to proceed correctly".to_string()
                };

                return Ok(AgentAction {
                    action_type: ActionType::AskClarification,
                    app_name: None,
                    target_element: None,
                    parameters: Some(params),
                    reasoning,
                });
            }
            // MEMORY_SAVE is handled specially above to preserve multi-line content
            // So we skip it here in the line-by-line processing
            else if command_part.starts_with("EXCEL_TYPE:") {
                return parse_excel_type_command(command_part, justification);
            }
            else if command_part.starts_with("URL:") {
                return parse_url_command(command_part, justification);
            }
            // Excel-specific AppleScript commands (macOS only)
            else if let Some(excel_cmd) = parse_excel_applescript_command(command_part, justification.clone()) {
                return excel_cmd;
            }
            // Word-specific AppleScript commands (macOS only)
            else if let Some(word_cmd) = parse_word_applescript_command(command_part, justification.clone()) {
                return word_cmd;
            }
            // PowerPoint-specific COM commands (Windows only)
            else if let Some(ppt_cmd) = parse_powerpoint_command(command_part, justification.clone()) {
                return ppt_cmd;
            }
            // Terminal commands
            else if let Some(terminal_cmd) = parse_terminal_command(command_part, justification.clone()) {
                return terminal_cmd;
            }
            // Google Search acceleration
            else if command_part.starts_with("GOOGLE_SEARCH:") {
                return parse_google_search_command(command_part, justification);
            }
        }
    }

    // Extract the JSON portion if needed (fall back to JSON parsing for backward compatibility)
    let json_str = if let Some(start) = action_json.find('{') {
        if let Some(end) = action_json.rfind('}') {
            &action_json[start..=end]
        } else {
            action_json
        }
    } else {
        action_json
    };

    // Try to parse the action
    match serde_json::from_str::<AgentAction>(json_str) {
        Ok(action) => Ok(action),
        Err(e) => {
            error!("Failed to parse agent action: {}", e);

            // Return error so the LLM receives feedback about the invalid command
            Err(format!("Invalid command format. Response: '{}'. Error: {}", action_json, e))
        }
    }
}

/// Parse CLICK command
fn parse_click_command(command_part: &str, justification: String) -> Result<AgentAction, String> {
    let element_id_str = command_part.replace("CLICK:", "").trim().to_string();

    // Extract just the number part (handle cases like "55 Link:..." or just "55")
    let element_id = element_id_str.split_whitespace()
        .next()
        .unwrap_or(&element_id_str)
        .to_string();

    // Try to parse as number (index)
    if let Ok(index) = element_id.parse::<usize>() {
        // IMPORTANT: Validate element index before using it
        // We need current app state for validation
        let app_state = get_app_state()
            .ok_or("No application state available")?;

        // Get the element directly from the cache by index
        if let Some(element) = get_element_by_index(index) {
            // Get element path for macOS
            #[cfg(target_os = "macos")]
            let element_path = element.path.clone();

            #[cfg(target_os = "windows")]
            let element_path = if let Some(ref_id) = element["element_ref"].as_str() {
                ref_id.to_string()
            } else {
                element["element_path"].as_str().unwrap_or("").to_string()
            };

            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let element_path = String::new();

            // Retrieve the current app name from APP_STATE
            let app_name = app_state.current_app.clone().unwrap_or_default();

            let reasoning = if !justification.is_empty() {
                justification
            } else {
                #[cfg(target_os = "macos")]
                let reason = format!("Clicking element {} (Type: {}, Title: {})",
                    index, &element.role, &element.title);
                #[cfg(target_os = "windows")]
                let reason = format!("Clicking element {} (Type: {}, Title: {})",
                    index, element["role"].as_str().unwrap_or(""), element["title"].as_str().unwrap_or(""));
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                let reason = format!("Clicking element {}", index);
                reason
            };

            return Ok(AgentAction {
                action_type: ActionType::ClickElement,
                app_name: Some(app_name),
                target_element: Some(element_path),
                parameters: None,
                reasoning,
            });
        } else {
            return Err(format!("No element found at index {}", index));
        }
    }

    // If not a valid index, return error
    Err(format!("Invalid element reference '{}'. Please use a number to reference an element from the list.", element_id))
}

/// Parse TYPE command (single or multi-element)
fn parse_type_command(command_part: &str, justification: String) -> Result<AgentAction, String> {
    let payload = command_part.trim_start_matches("TYPE:").trim();

    // Check for multiple element:text pairs using ::: separator
    let is_multi_element = payload.contains(":::");

    if is_multi_element {
        parse_multi_element_type(payload, justification)
    } else {
        parse_single_element_type(payload, justification)
    }
}

/// Parse multi-element TYPE command: TYPE:5:text1:::8:text2:::12:text3
fn parse_multi_element_type(payload: &str, justification: String) -> Result<AgentAction, String> {
    let mut element_paths = Vec::new();
    let mut texts = Vec::new();
    let mut error_messages = Vec::new();

    // Split by ::: and parse each element:text pair
    for pair in payload.split(":::") {
        let pair = pair.trim();
        let mut parts = pair.splitn(2, ':');

        if let (Some(element_str), Some(text_str)) = (parts.next(), parts.next()) {
            let element_str = element_str.trim();
            let text_str = text_str.trim();

            // Parse element number
            match element_str.parse::<usize>() {
                Ok(index) => {
                    // Get the element from cache
                    if let Some(element) = get_element_by_index(index) {
                        #[cfg(target_os = "macos")]
                        let element_path = element.path.clone();

                        #[cfg(target_os = "windows")]
                        let element_path = if let Some(ref_id) = element["element_ref"].as_str() {
                            ref_id.to_string()
                        } else {
                            element["element_path"].as_str().unwrap_or("").to_string()
                        };

                        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                        let element_path = String::new();

                        element_paths.push(element_path);
                        texts.push(text_str.to_string());
                    } else {
                        error_messages.push(format!("No element found at index {}", index));
                    }
                },
                Err(_) => {
                    error_messages.push(format!("Invalid element reference '{}'. Please use a number to reference an element from the list.", element_str));
                }
            }
        } else {
            error_messages.push(format!("Invalid element:text pair format: '{}'", pair));
        }
    }

    // If any errors, return the first error
    if !error_messages.is_empty() {
        return Err(error_messages[0].clone());
    }

    // If no valid pairs found, return error
    if element_paths.is_empty() {
        return Err("No valid element:text pairs found".to_string());
    }

    // Resolve app name
    let app_state = get_app_state()
        .ok_or("No application state available")?;
    let app_name = app_state.current_app.clone().unwrap_or_default();

    // Store multiple targets and texts with ||| delimiter to avoid conflicts with commas in text
    let mut params = HashMap::new();
    params.insert("text".into(), texts.join("|||"));

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        format!("Typing into {} elements: {}", element_paths.len(), texts.join(", "))
    };

    Ok(AgentAction {
        action_type: ActionType::TypeText,
        app_name: Some(app_name),
        target_element: Some(element_paths.join("|||")),
        parameters: Some(params),
        reasoning,
    })
}

/// Parse single element TYPE command: TYPE:5:text
fn parse_single_element_type(payload: &str, justification: String) -> Result<AgentAction, String> {
    let mut parts = payload.splitn(2, ':');
    let first_part = parts.next().unwrap().trim();
    let second_part = parts.next();

    let (element_index, text) = match second_part {
        // New syntax: TYPE:<element_number>:<text>
        Some(txt) => {
            // Try to parse element number
            match first_part.parse::<usize>() {
                Ok(index) => {
                    // Get the element from cache
                    if let Some(element) = get_element_by_index(index) {
                        #[cfg(target_os = "macos")]
                        let element_path = element.path.clone();

                        #[cfg(target_os = "windows")]
                        let element_path = if let Some(ref_id) = element["element_ref"].as_str() {
                            ref_id.to_string()
                        } else {
                            element["element_path"].as_str().unwrap_or("").to_string()
                        };

                        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                        let element_path = String::new();

                        info!("🔍 DEBUG: Element {} resolved to ID/path: '{}'", index, element_path);
                        (Some(element_path), txt.trim().to_string())
                    } else {
                        return Err(format!("No element found at index {}", index));
                    }
                },
                Err(_) => return Err(format!("Invalid element reference '{}'. Please use a number to reference an element from the list.", first_part))
            }
        },
        // Legacy syntax: TYPE:<text> - return error as we now require an element
        None => return Err("Element number is required for TYPE command. Use TYPE:<element_number>:<text> format.".to_string()),
    };

    // Resolve app name
    let app_state = get_app_state()
        .ok_or("No application state available")?;
    let app_name = app_state.current_app.clone().unwrap_or_default();

    let mut params = HashMap::new();
    params.insert("text".into(), text.clone());

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        format!("Typing '{}' in element", text)
    };

    Ok(AgentAction {
        action_type: ActionType::TypeText,
        app_name: Some(app_name),
        target_element: element_index,
        parameters: Some(params),
        reasoning,
    })
}

/// Parse PRESS command
fn parse_press_command(command_part: &str, justification: String) -> Result<AgentAction, String> {
    let key = command_part.replace("PRESS:", "").trim().to_string();
    let app_state = get_app_state()
        .ok_or("No application state available")?;

    let app_name = app_state.current_app.clone().unwrap_or_default();
    let mut params = HashMap::new();
    params.insert("key".to_string(), key.clone());

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        format!("Pressing key: '{}'", key)
    };

    Ok(AgentAction {
        action_type: ActionType::PressKey,
        app_name: Some(app_name),
        target_element: None,
        parameters: Some(params),
        reasoning,
    })
}

/// Parse EXCEL_TYPE command
fn parse_excel_type_command(command_part: &str, justification: String) -> Result<AgentAction, String> {
    let payload = command_part.trim_start_matches("EXCEL_TYPE:").trim();

    // Parse cell:value pairs separated by :::
    let mut cell_data = Vec::new();
    for pair in payload.split(":::") {
        let pair = pair.trim();
        let mut parts = pair.splitn(2, ':');

        if let (Some(cell), Some(value)) = (parts.next(), parts.next()) {
            cell_data.push(format!("{}:{}", cell.trim(), value.trim()));
        } else {
            return Err(format!("Invalid cell:value pair format: '{}'", pair));
        }
    }

    if cell_data.is_empty() {
        return Err("No valid cell:value pairs found for EXCEL_TYPE".to_string());
    }

    // Store the cell data as a parameter
    let mut params = HashMap::new();
    params.insert("cell_data".to_string(), cell_data.join(":::"));

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        format!("Entering data into {} Excel cells", cell_data.len())
    };

    Ok(AgentAction {
        action_type: ActionType::ExcelType,
        app_name: Some("Microsoft Excel".to_string()), // Default to Excel
        target_element: None,
        parameters: Some(params),
        reasoning,
    })
}

/// Parse URL command
fn parse_url_command(command_part: &str, justification: String) -> Result<AgentAction, String> {
    let payload = command_part.trim_start_matches("URL:").trim();

    // Parse URL:<element_number>:<url>
    let mut parts = payload.splitn(2, ':');
    let element_str = parts.next().unwrap().trim();
    let url = parts.next().ok_or("URL parameter is missing. Use URL:<element_number>:<url> format.")?;

    // Parse element number
    let element_index = element_str.parse::<usize>()
        .map_err(|_| format!("Invalid element reference '{}'. Please use a number to reference an element from the list.", element_str))?;

    // Get the element from cache
    let element = get_element_by_index(element_index)
        .ok_or(format!("No element found at index {}", element_index))?;

    #[cfg(target_os = "macos")]
    let element_path = element.path.clone();

    #[cfg(target_os = "windows")]
    let element_path = if let Some(ref_id) = element["element_ref"].as_str() {
        ref_id.to_string()
    } else {
        element["element_path"].as_str().unwrap_or("").to_string()
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let element_path = String::new();

    // Get current app state
    let app_state = get_app_state()
        .ok_or("No application state available")?;
    let app_name = app_state.current_app.clone().unwrap_or_default();

    let mut params = HashMap::new();
    params.insert("url".to_string(), url.to_string());

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        format!("Navigating to {} via element {}", url, element_index)
    };

    Ok(AgentAction {
        action_type: ActionType::NavigateURL,
        app_name: Some(app_name),
        target_element: Some(element_path),
        parameters: Some(params),
        reasoning,
    })
}

/// Parse Excel AppleScript commands (macOS only)
/// Returns Some(Result) if this is an Excel command, None otherwise
fn parse_excel_applescript_command(command_part: &str, justification: String) -> Option<Result<AgentAction, String>> {
    // Check if this matches any Excel command pattern
    let excel_cmd = EXCEL_COMMANDS.iter().find(|&&cmd| command_part.starts_with(cmd))?;

    // Extract the command name (without trailing colon if present)
    let cmd_name = excel_cmd.trim_end_matches(':');

    // Parse the parameters (everything after the command prefix)
    let params_str = command_part[excel_cmd.len()..].trim();

    // Build the parameters HashMap
    let mut params = HashMap::new();
    params.insert("command".to_string(), cmd_name.to_string());

    // Parse parameters based on command type
    match cmd_name {
        "EXCEL_RENAME_SHEET" => {
            // Format: EXCEL_RENAME_SHEET:old_name:new_name
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("EXCEL_RENAME_SHEET requires old_name:new_name".to_string()));
            }
            params.insert("old_name".to_string(), parts[0].trim().to_string());
            params.insert("new_name".to_string(), parts[1].trim().to_string());
        },
        "EXCEL_NEW_SHEET" | "EXCEL_DELETE_SHEET" | "EXCEL_SELECT_SHEET" => {
            // Format: COMMAND:sheet_name
            if params_str.is_empty() {
                return Some(Err(format!("{} requires a sheet name", cmd_name)));
            }
            params.insert("sheet_name".to_string(), params_str.to_string());
        },
        "EXCEL_SET_CELL" | "EXCEL_SET_FORMULA" => {
            // Format: COMMAND:cell:value
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err(format!("{} requires cell:value", cmd_name)));
            }
            params.insert("cell".to_string(), parts[0].trim().to_string());
            params.insert("value".to_string(), parts[1].trim().to_string());
        },
        "EXCEL_FORMAT_CELLS" => {
            // Format: EXCEL_FORMAT_CELLS:range:format_type (e.g., A1:G23:currency)
            // Use rsplitn to split from the end since range contains ':'
            let parts: Vec<&str> = params_str.rsplitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("EXCEL_FORMAT_CELLS requires range:format_type".to_string()));
            }
            params.insert("range".to_string(), parts[1].trim().to_string());
            params.insert("format".to_string(), parts[0].trim().to_string());
        },
        "EXCEL_AUTOFIT_COLUMNS" | "EXCEL_BOLD" | "EXCEL_ITALIC" | "EXCEL_UNDERLINE" |
        "EXCEL_WRAP_TEXT" | "EXCEL_MERGE_CELLS" | "EXCEL_UNMERGE_CELLS" |
        "EXCEL_CLEAR_RANGE" | "EXCEL_GET_CELL_VALUE" | "EXCEL_GET_RANGE_VALUES" |
        "EXCEL_GET_FORMULA" | "EXCEL_GET_RANGE_FORMULAS" => {
            // Format: COMMAND:range
            if params_str.is_empty() {
                return Some(Err(format!("{} requires a range", cmd_name)));
            }
            params.insert("range".to_string(), params_str.to_string());
        },
        "EXCEL_GET_SHEET_INFO" => {
            // No parameters needed
        },
        "EXCEL_GET_STRUCTURE" => {
            // Format: EXCEL_GET_STRUCTURE:range (e.g., A1:P30)
            if params_str.is_empty() {
                return Some(Err("EXCEL_GET_STRUCTURE requires a range".to_string()));
            }
            params.insert("range".to_string(), params_str.to_string());
        },
        "EXCEL_SET_FILL_COLOR" | "EXCEL_SET_FONT_COLOR" => {
            // Format: COMMAND:range:color (e.g., A1:G23:yellow)
            // Use rsplitn to split from the end since range contains ':'
            let parts: Vec<&str> = params_str.rsplitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err(format!("{} requires range:color", cmd_name)));
            }
            params.insert("range".to_string(), parts[1].trim().to_string());
            params.insert("color".to_string(), parts[0].trim().to_string());
        },
        "EXCEL_SET_COLUMN_WIDTH" => {
            // Format: EXCEL_SET_COLUMN_WIDTH:column:width
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("EXCEL_SET_COLUMN_WIDTH requires column:width".to_string()));
            }
            params.insert("column".to_string(), parts[0].trim().to_string());
            params.insert("width".to_string(), parts[1].trim().to_string());
        },
        "EXCEL_COPY_RANGE" => {
            // Format: EXCEL_COPY_RANGE:source:destination
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("EXCEL_COPY_RANGE requires source:destination".to_string()));
            }
            params.insert("source".to_string(), parts[0].trim().to_string());
            params.insert("dest".to_string(), parts[1].trim().to_string());
        },
        "EXCEL_SET_FONT" => {
            // Format: EXCEL_SET_FONT:range:font_name (e.g., A1:G23:Arial)
            // Use rsplitn to split from the end since range contains ':'
            let parts: Vec<&str> = params_str.rsplitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("EXCEL_SET_FONT requires range:font_name".to_string()));
            }
            params.insert("range".to_string(), parts[1].trim().to_string());
            params.insert("font".to_string(), parts[0].trim().to_string());
        },
        "EXCEL_SET_FONT_SIZE" => {
            // Format: EXCEL_SET_FONT_SIZE:range:size (e.g., A1:G23:12)
            // Use rsplitn to split from the end since range contains ':'
            let parts: Vec<&str> = params_str.rsplitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("EXCEL_SET_FONT_SIZE requires range:size".to_string()));
            }
            params.insert("range".to_string(), parts[1].trim().to_string());
            params.insert("size".to_string(), parts[0].trim().to_string());
        },
        "EXCEL_ADD_BORDER" => {
            // Format: EXCEL_ADD_BORDER:range:style (e.g., A1:G23:outline)
            // Use rsplitn to split from the end since range contains ':'
            let parts: Vec<&str> = params_str.rsplitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("EXCEL_ADD_BORDER requires range:style".to_string()));
            }
            // rsplitn returns [last_part, rest] so parts[1] is range, parts[0] is style
            params.insert("range".to_string(), parts[1].trim().to_string());
            params.insert("style".to_string(), parts[0].trim().to_string());
        },
        "EXCEL_ALIGN" | "EXCEL_VERTICAL_ALIGN" => {
            // Format: COMMAND:range:alignment (e.g., A1:G23:center)
            // Use rsplitn to split from the end since range contains ':'
            let parts: Vec<&str> = params_str.rsplitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err(format!("{} requires range:alignment", cmd_name)));
            }
            params.insert("range".to_string(), parts[1].trim().to_string());
            params.insert("align".to_string(), parts[0].trim().to_string());
        },
        "EXCEL_SET_ROW_HEIGHT" => {
            // Format: EXCEL_SET_ROW_HEIGHT:row:height
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("EXCEL_SET_ROW_HEIGHT requires row:height".to_string()));
            }
            params.insert("row".to_string(), parts[0].trim().to_string());
            params.insert("height".to_string(), parts[1].trim().to_string());
        },
        _ => {
            return Some(Err(format!("Unknown Excel command: {}", cmd_name)));
        }
    }

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        format!("Executing Excel command: {}", cmd_name)
    };

    Some(Ok(AgentAction {
        action_type: ActionType::ExcelCommand,
        app_name: Some("Microsoft Excel".to_string()),
        target_element: None,
        parameters: Some(params),
        reasoning,
    }))
}

/// Parse Word AppleScript commands (macOS only)
/// Returns Some(Result) if this is a Word command, None otherwise
fn parse_word_applescript_command(command_part: &str, justification: String) -> Option<Result<AgentAction, String>> {
    // Check if this matches any Word command pattern
    let word_cmd = WORD_COMMANDS.iter().find(|&&cmd| command_part.starts_with(cmd))?;

    // Extract the command name (without trailing colon if present)
    let cmd_name = word_cmd.trim_end_matches(':');

    // Parse the parameters (everything after the command prefix)
    let params_str = command_part[word_cmd.len()..].trim();

    // Build the parameters HashMap
    let mut params = HashMap::new();
    params.insert("command".to_string(), cmd_name.to_string());

    // Parse parameters based on command type
    match cmd_name {
        "WORD_NEW_DOCUMENT" | "WORD_CLOSE" |
        "WORD_BOLD" | "WORD_ITALIC" | "WORD_UNDERLINE" | "WORD_DELETE" |
        "WORD_GET_TEXT" | "WORD_GET_SELECTION" | "WORD_GET_SELECTION_FORMAT" | 
        "WORD_GET_FORMATTING" | "WORD_GET_WORD_COUNT" | "WORD_GET_DOCUMENT_INFO" | 
        "WORD_SELECT_ALL" | "WORD_DESELECT" | "WORD_GOTO_START" | "WORD_GOTO_END" => {
            // No parameters needed
        },
        "WORD_OPEN" => {
            // Format: WORD_OPEN:filepath
            if params_str.is_empty() {
                return Some(Err("WORD_OPEN requires a file path".to_string()));
            }
            params.insert("filepath".to_string(), params_str.to_string());
        },
        "WORD_INSERT_TEXT" => {
            // Format: WORD_INSERT_TEXT:text
            if params_str.is_empty() {
                return Some(Err("WORD_INSERT_TEXT requires text".to_string()));
            }
            params.insert("text".to_string(), params_str.to_string());
        },
        "WORD_INSERT_PARAGRAPH" => {
            // Format: WORD_INSERT_PARAGRAPH:text (text is optional - empty creates blank line)
            params.insert("text".to_string(), params_str.to_string());
        },
        "WORD_FIND_REPLACE" => {
            // Format: WORD_FIND_REPLACE:find_text:replace_text
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("WORD_FIND_REPLACE requires find_text:replace_text".to_string()));
            }
            params.insert("find".to_string(), parts[0].trim().to_string());
            params.insert("replace".to_string(), parts[1].trim().to_string());
        },
        "WORD_SET_FONT" => {
            // Format: WORD_SET_FONT:font_name
            if params_str.is_empty() {
                return Some(Err("WORD_SET_FONT requires a font name".to_string()));
            }
            params.insert("font".to_string(), params_str.to_string());
        },
        "WORD_SET_FONT_SIZE" => {
            // Format: WORD_SET_FONT_SIZE:size
            if params_str.is_empty() {
                return Some(Err("WORD_SET_FONT_SIZE requires a size".to_string()));
            }
            params.insert("size".to_string(), params_str.to_string());
        },
        "WORD_ALIGN" => {
            // Format: WORD_ALIGN:alignment
            if params_str.is_empty() {
                return Some(Err("WORD_ALIGN requires an alignment (left, center, right, justify)".to_string()));
            }
            params.insert("alignment".to_string(), params_str.to_string());
        },
        "WORD_INSERT_TABLE" => {
            // Format: WORD_INSERT_TABLE:rows:cols
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("WORD_INSERT_TABLE requires rows:cols".to_string()));
            }
            params.insert("rows".to_string(), parts[0].trim().to_string());
            params.insert("cols".to_string(), parts[1].trim().to_string());
        },
        "WORD_SELECT_TEXT" => {
            // Format: WORD_SELECT_TEXT:text to find and select
            if params_str.is_empty() {
                return Some(Err("WORD_SELECT_TEXT requires text to find".to_string()));
            }
            params.insert("text".to_string(), params_str.to_string());
        },
        "WORD_SELECT_PARAGRAPH" => {
            // Format: WORD_SELECT_PARAGRAPH:paragraph_number
            if params_str.is_empty() {
                return Some(Err("WORD_SELECT_PARAGRAPH requires a paragraph number".to_string()));
            }
            params.insert("paragraph".to_string(), params_str.to_string());
        },
        "WORD_SELECT_TABLE" => {
            // Format: WORD_SELECT_TABLE:table_number
            if params_str.is_empty() {
                return Some(Err("WORD_SELECT_TABLE requires a table number".to_string()));
            }
            params.insert("number".to_string(), params_str.to_string());
        },
        "WORD_SELECT_BETWEEN" => {
            // Format: WORD_SELECT_BETWEEN:start_text:end_text
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("WORD_SELECT_BETWEEN requires start_text:end_text".to_string()));
            }
            params.insert("start_text".to_string(), parts[0].trim().to_string());
            params.insert("end_text".to_string(), parts[1].trim().to_string());
        },
        "WORD_MOVE_AFTER_TEXT" => {
            // Format: WORD_MOVE_AFTER_TEXT:text to find
            if params_str.is_empty() {
                return Some(Err("WORD_MOVE_AFTER_TEXT requires text to find".to_string()));
            }
            params.insert("text".to_string(), params_str.to_string());
        },
        // Windows COM cursor positioning commands
        "WORD_SET_CURSOR" => {
            // Format: WORD_SET_CURSOR:position
            if params_str.is_empty() {
                return Some(Err("WORD_SET_CURSOR requires a position".to_string()));
            }
            params.insert("position".to_string(), params_str.to_string());
        },
        "WORD_INSERT_AT_START" | "WORD_INSERT_AT_END" => {
            // Format: COMMAND:text
            if params_str.is_empty() {
                return Some(Err(format!("{} requires text", cmd_name)));
            }
            params.insert("text".to_string(), params_str.to_string());
        },
        "WORD_INSERT_AFTER" | "WORD_INSERT_BEFORE" => {
            // Format: COMMAND:search_text:insert_text
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err(format!("{} requires search_text:insert_text", cmd_name)));
            }
            params.insert("search".to_string(), parts[0].trim().to_string());
            params.insert("text".to_string(), parts[1].trim().to_string());
        },
        _ => {
            return Some(Err(format!("Unknown Word command: {}", cmd_name)));
        }
    }

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        format!("Executing Word command: {}", cmd_name)
    };

    Some(Ok(AgentAction {
        action_type: ActionType::WordCommand,
        app_name: Some("Microsoft Word".to_string()),
        target_element: None,
        parameters: Some(params),
        reasoning,
    }))
}

/// Parse PowerPoint COM commands (Windows only)
/// Returns Some(Result) if this is a PowerPoint command, None otherwise
fn parse_powerpoint_command(command_part: &str, justification: String) -> Option<Result<AgentAction, String>> {
    // Check if this matches any PowerPoint command pattern
    let ppt_cmd = POWERPOINT_COMMANDS.iter().find(|&&cmd| command_part.starts_with(cmd))?;

    // Extract the command name (without trailing colon if present)
    let cmd_name = ppt_cmd.trim_end_matches(':');

    // Parse the parameters (everything after the command prefix)
    let params_str = command_part[ppt_cmd.len()..].trim();

    // Build the parameters HashMap
    let mut params = HashMap::new();
    params.insert("command".to_string(), cmd_name.to_string());

    // Parse parameters based on command type
    match cmd_name {
        // No parameters
        "POWERPOINT_NEW_PRESENTATION" | "POWERPOINT_SAVE" | "POWERPOINT_DUPLICATE_SLIDE" |
        "POWERPOINT_GET_SLIDE_COUNT" | "POWERPOINT_GET_SLIDE_INFO" | "POWERPOINT_GET_PRESENTATION_INFO" => {
            // No parameters needed
        },
        // Single path parameter
        "POWERPOINT_SAVE_AS" => {
            if params_str.is_empty() {
                return Some(Err("POWERPOINT_SAVE_AS requires a file path".to_string()));
            }
            params.insert("path".to_string(), params_str.to_string());
        },
        // Single layout/number parameter
        "POWERPOINT_ADD_SLIDE" => {
            let layout = if params_str.is_empty() { "content" } else { params_str };
            params.insert("layout".to_string(), layout.to_string());
        },
        "POWERPOINT_GO_TO_SLIDE" | "POWERPOINT_DELETE_SLIDE" => {
            if params_str.is_empty() {
                return Some(Err(format!("{} requires a slide number", cmd_name)));
            }
            params.insert("number".to_string(), params_str.to_string());
        },
        // Single text parameter
        "POWERPOINT_SET_TITLE" | "POWERPOINT_SET_SUBTITLE" | "POWERPOINT_SET_BODY" | "POWERPOINT_ADD_NOTE" => {
            if params_str.is_empty() {
                return Some(Err(format!("{} requires text", cmd_name)));
            }
            params.insert("text".to_string(), params_str.to_string());
        },
        // Single theme/color/transition parameter
        "POWERPOINT_APPLY_THEME" => {
            if params_str.is_empty() {
                return Some(Err("POWERPOINT_APPLY_THEME requires a theme name".to_string()));
            }
            params.insert("theme".to_string(), params_str.to_string());
        },
        "POWERPOINT_SET_BACKGROUND_COLOR" => {
            if params_str.is_empty() {
                return Some(Err("POWERPOINT_SET_BACKGROUND_COLOR requires a color".to_string()));
            }
            params.insert("color".to_string(), params_str.to_string());
        },
        "POWERPOINT_SET_TRANSITION" | "POWERPOINT_SET_ALL_TRANSITIONS" => {
            if params_str.is_empty() {
                return Some(Err(format!("{} requires a transition type", cmd_name)));
            }
            params.insert("type".to_string(), params_str.to_string());
        },
        // Two parameters (rows:cols)
        "POWERPOINT_ADD_TABLE" => {
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("POWERPOINT_ADD_TABLE requires rows:cols".to_string()));
            }
            params.insert("rows".to_string(), parts[0].trim().to_string());
            params.insert("cols".to_string(), parts[1].trim().to_string());
        },
        // Two parameters (row:values)
        "POWERPOINT_SET_TABLE_ROW" => {
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("POWERPOINT_SET_TABLE_ROW requires row:values".to_string()));
            }
            params.insert("row".to_string(), parts[0].trim().to_string());
            params.insert("values".to_string(), parts[1].trim().to_string());
        },
        // Three parameters (row:col:text)
        "POWERPOINT_SET_TABLE_CELL" => {
            let parts: Vec<&str> = params_str.splitn(3, ':').collect();
            if parts.len() != 3 {
                return Some(Err("POWERPOINT_SET_TABLE_CELL requires row:col:text".to_string()));
            }
            params.insert("row".to_string(), parts[0].trim().to_string());
            params.insert("col".to_string(), parts[1].trim().to_string());
            params.insert("text".to_string(), parts[2].trim().to_string());
        },
        // Three parameters (bold:size:color)
        "POWERPOINT_FORMAT_TITLE" | "POWERPOINT_FORMAT_BODY" => {
            let parts: Vec<&str> = params_str.splitn(3, ':').collect();
            if parts.len() != 3 {
                return Some(Err(format!("{} requires bold:size:color", cmd_name)));
            }
            params.insert("bold".to_string(), parts[0].trim().to_string());
            params.insert("size".to_string(), parts[1].trim().to_string());
            params.insert("color".to_string(), parts[2].trim().to_string());
        },
        // Two parameters (font:target)
        "POWERPOINT_SET_FONT" => {
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.is_empty() || parts[0].is_empty() {
                return Some(Err("POWERPOINT_SET_FONT requires font:target".to_string()));
            }
            params.insert("font".to_string(), parts[0].trim().to_string());
            params.insert("target".to_string(), parts.get(1).unwrap_or(&"title").trim().to_string());
        },
        // Two parameters (alignment:target)
        "POWERPOINT_ALIGN_TEXT" => {
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.is_empty() || parts[0].is_empty() {
                return Some(Err("POWERPOINT_ALIGN_TEXT requires alignment:target".to_string()));
            }
            params.insert("align".to_string(), parts[0].trim().to_string());
            params.insert("target".to_string(), parts.get(1).unwrap_or(&"title").trim().to_string());
        },
        // Two parameters (type:items)
        "POWERPOINT_ADD_SMARTART" => {
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("POWERPOINT_ADD_SMARTART requires type:items".to_string()));
            }
            params.insert("type".to_string(), parts[0].trim().to_string());
            params.insert("items".to_string(), parts[1].trim().to_string());
        },
        // Two parameters (type:data)
        "POWERPOINT_ADD_CHART" => {
            let parts: Vec<&str> = params_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Some(Err("POWERPOINT_ADD_CHART requires type:data".to_string()));
            }
            params.insert("type".to_string(), parts[0].trim().to_string());
            params.insert("data".to_string(), parts[1].trim().to_string());
        },
        // No parameters
        "POWERPOINT_ADD_SLIDE_NUMBERS" => {
            // No parameters needed
        },
        _ => {
            return Some(Err(format!("Unknown PowerPoint command: {}", cmd_name)));
        }
    }

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        format!("Executing PowerPoint command: {}", cmd_name)
    };

    Some(Ok(AgentAction {
        action_type: ActionType::PowerPointCommand,
        app_name: Some("Microsoft PowerPoint".to_string()),
        target_element: None,
        parameters: Some(params),
        reasoning,
    }))
}

/// Parse terminal commands
/// Returns Some(Result) if this is a terminal command, None otherwise
fn parse_terminal_command(command_part: &str, justification: String) -> Option<Result<AgentAction, String>> {
    // Check if this matches any terminal command pattern
    let terminal_cmd = TERMINAL_COMMANDS.iter().find(|&&cmd| command_part.starts_with(cmd))?;

    // Extract the command name (without trailing colon)
    let cmd_name = terminal_cmd.trim_end_matches(':');

    // Parse the parameters (everything after the command prefix)
    let params_str = command_part[terminal_cmd.len()..].trim();

    // Build the parameters HashMap
    let mut params = HashMap::new();

    // Determine the action type and parse parameters
    let action_type = match cmd_name {
        "TERMINAL_RUN" => {
            // Format: TERMINAL_RUN:command
            // or: TERMINAL_RUN:command:cwd
            if params_str.is_empty() {
                return Some(Err("TERMINAL_RUN requires a command".to_string()));
            }
            
            // Check if there's a working directory specified (last :: separated part)
            if let Some(last_colon) = params_str.rfind("::") {
                let command = params_str[..last_colon].trim();
                let cwd = params_str[last_colon + 2..].trim();
                params.insert("command".to_string(), command.to_string());
                if !cwd.is_empty() {
                    params.insert("cwd".to_string(), cwd.to_string());
                }
            } else {
                params.insert("command".to_string(), params_str.to_string());
            }
            
            ActionType::TerminalRun
        },
        "TERMINAL_BACKGROUND" => {
            // Format: TERMINAL_BACKGROUND:command
            // or: TERMINAL_BACKGROUND:command::cwd
            if params_str.is_empty() {
                return Some(Err("TERMINAL_BACKGROUND requires a command".to_string()));
            }
            
            if let Some(last_colon) = params_str.rfind("::") {
                let command = params_str[..last_colon].trim();
                let cwd = params_str[last_colon + 2..].trim();
                params.insert("command".to_string(), command.to_string());
                if !cwd.is_empty() {
                    params.insert("cwd".to_string(), cwd.to_string());
                }
            } else {
                params.insert("command".to_string(), params_str.to_string());
            }
            
            ActionType::TerminalBackground
        },
        "TERMINAL_CHECK" => {
            // Format: TERMINAL_CHECK:process_id
            if params_str.is_empty() {
                return Some(Err("TERMINAL_CHECK requires a process_id".to_string()));
            }
            params.insert("process_id".to_string(), params_str.to_string());
            
            ActionType::TerminalCheck
        },
        "TERMINAL_KILL" => {
            // Format: TERMINAL_KILL:process_id
            if params_str.is_empty() {
                return Some(Err("TERMINAL_KILL requires a process_id".to_string()));
            }
            params.insert("process_id".to_string(), params_str.to_string());
            
            ActionType::TerminalKill
        },
        "TERMINAL_READ" => {
            // Format: TERMINAL_READ:process_id
            if params_str.is_empty() {
                return Some(Err("TERMINAL_READ requires a process_id".to_string()));
            }
            params.insert("process_id".to_string(), params_str.to_string());
            
            ActionType::TerminalRead
        },
        _ => {
            return Some(Err(format!("Unknown terminal command: {}", cmd_name)));
        }
    };

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        match action_type {
            ActionType::TerminalRun => format!("Running terminal command: {}", params.get("command").unwrap_or(&String::new())),
            ActionType::TerminalBackground => format!("Starting background process: {}", params.get("command").unwrap_or(&String::new())),
            ActionType::TerminalCheck => format!("Checking process status: {}", params.get("process_id").unwrap_or(&String::new())),
            ActionType::TerminalKill => format!("Killing process: {}", params.get("process_id").unwrap_or(&String::new())),
            ActionType::TerminalRead => format!("Reading process output: {}", params.get("process_id").unwrap_or(&String::new())),
            _ => "Executing terminal command".to_string(),
        }
    };

    Some(Ok(AgentAction {
        action_type,
        app_name: Some("Terminal".to_string()),
        target_element: None,
        parameters: Some(params),
        reasoning,
    }))
}

/// Parse GOOGLE_SEARCH command
/// Format: GOOGLE_SEARCH:<query> || justification
fn parse_google_search_command(command_part: &str, justification: String) -> Result<AgentAction, String> {
    let query = command_part.trim_start_matches("GOOGLE_SEARCH:").trim();

    if query.is_empty() {
        return Err("GOOGLE_SEARCH requires a search query".to_string());
    }

    let mut params = HashMap::new();
    params.insert("query".to_string(), query.to_string());

    let reasoning = if !justification.is_empty() {
        justification
    } else {
        format!("Searching Google for: {}", query)
    };

    Ok(AgentAction {
        action_type: ActionType::GoogleSearch,
        app_name: Some("Google Chrome".to_string()),
        target_element: None,
        parameters: Some(params),
        reasoning,
    })
}
