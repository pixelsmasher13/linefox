#![allow(dead_code)]

use chrono::{DateTime, Local};
use log::info;
use rusqlite::{params, Connection, Error};
use std::collections::HashSet;

use crate::entity::activity_item::ActivityItem;
use crate::entity::automation_log::{AutomationLog, AutomationStep, AutomationSummary};

/// Saves a filtered automation log entry
pub fn save_automation_log(
    db: &Connection,
    automation_log: &AutomationLog,
) -> Result<i64, Error> {
    let mut statement = db.prepare(
        "INSERT INTO automation_logs
        (automation_id, timestamp, action_type, element_role, element_value, element_description,
         window_title, window_app_name, importance_score, raw_event_data, sequence_number)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )?;

    statement.execute(params![
        automation_log.automation_id,
        automation_log.timestamp,
        automation_log.action_type,
        automation_log.element_role,
        automation_log.element_value,
        automation_log.element_description,
        automation_log.window_title,
        automation_log.window_app_name,
        automation_log.importance_score,
        automation_log.raw_event_data,
        automation_log.sequence_number,
    ])?;

    Ok(db.last_insert_rowid())
}

/// Gets all automation logs for a specific automation
pub fn get_automation_logs(
    db: &Connection,
    automation_id: i64,
) -> Result<Vec<AutomationLog>, Error> {
    let mut statement = db.prepare(
        "SELECT * FROM automation_logs 
         WHERE automation_id = ? 
         ORDER BY sequence_number ASC",
    )?;

    let automation_logs_iter = statement.query_map(params![automation_id], |row| {
        Ok(AutomationLog {
            id: row.get("id")?,
            automation_id: row.get("automation_id")?,
            timestamp: row.get("timestamp")?,
            action_type: row.get("action_type")?,
            element_role: row.get("element_role")?,
            element_value: row.get("element_value")?,
            element_description: row.get("element_description")?,
            window_title: row.get("window_title")?,
            window_app_name: row.get("window_app_name")?,
            importance_score: row.get("importance_score")?,
            raw_event_data: row.get("raw_event_data")?,
            sequence_number: row.get("sequence_number")?,
        })
    })?;

    let mut automation_logs = Vec::new();
    for log in automation_logs_iter {
        automation_logs.push(log?);
    }

    Ok(automation_logs)
}

/// Process raw activity logs for an automation and save filtered events
/// Returns the count of logs processed and saved
pub fn process_activity_logs_for_automation(
    db: &mut Connection,
    automation_id: i64,
) -> Result<usize, Error> {
    // First, check if logs were already processed - avoid duplicates
    let existing_count: i64 = db.query_row(
        "SELECT COUNT(*) FROM automation_logs WHERE automation_id = ?",
        params![automation_id],
        |row| row.get(0),
    )?;

    if existing_count > 0 {
        info!("Skipping processing - {} filtered logs already exist for automation {}", existing_count, automation_id);
        return Ok(existing_count as usize);
    }

    // First fetch all activity logs for this automation in a separate scope
    // This ensures the statement is dropped before we create a transaction
    let activities = {
        let mut statement = db.prepare(
            "SELECT * FROM activity_logs 
             WHERE automation_id = ? 
             ORDER BY timestamp ASC",
        )?;

        let activities_iter = statement.query_map(params![automation_id], |row| {
            Ok(ActivityItem {
                timestamp: row.get("timestamp")?,
                user_id: row.get("user_id")?,
                window_title: row.get("window_title")?,
                window_app_name: row.get("window_app_name")?,
                ocr_text: row.get("ocr_text")?,
                original_ocr_text: row.get("original_ocr_text")?,
                interval_length: row.get("interval_length")?,
                os_details: row.get("os_details")?,
                similarity_percentage_to_previous_ocr_text: row.get("similarity_percentage_to_previous_ocr_text")?,
                full_activity_text: row.get("full_activity_text")?,
                editing_mode: row.get("editing_mode")?,
                keypress_count: row.get("keypress_count")?,
                element_tree_dump: row.get("element_tree_dump")?,
                detected_actions: row.get("detected_actions")?,
                automation_id: Some(row.get("automation_id")?),
            })
        })?;

        let mut activities = Vec::new();
        for activity in activities_iter {
            activities.push(activity?);
        }
        activities
    }; // statement is dropped here

    let activities_count = activities.len();
    info!("Processing {} raw activity logs for automation {}", activities_count, automation_id);
    
    // Process each activity and extract important events
    let mut filtered_logs = Vec::new();
    let mut sequence_number = 0;
    let mut seen_actions = HashSet::new();

    for activity in &activities {
        // Skip if no detected actions
        if activity.detected_actions.is_empty() || activity.detected_actions == "/" {
            continue;
        }

        // Parse and filter actions
        for line in activity.detected_actions.lines() {
            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Basic parsing of action line
            let action_type = if line.contains("AXValueChanged") {
                "ValueChanged"
            } else if line.contains("AXFocusedUIElementChanged") {
                "FocusChanged"
            } else if line.contains("click") || line.contains("mouseDown") {
                "Click"
            } else if line.contains("keyPress") || line.contains("keyDown") {
                "KeyPress"
            } else {
                "Other"
            };

            // Extract role
            let element_role = extract_between(line, "on ", " ").unwrap_or_else(|| 
                if line.contains("Role:") {
                    extract_between(line, "Role: \"", "\"").unwrap_or("")
                } else {
                    ""
                }
            ).to_string();

            // Extract value if present
            let element_value = if line.contains("Value:") {
                extract_between(line, "Value: \"", "\"").map(String::from)
            } else {
                None
            };

            // Extract description if present
            let element_description = if line.contains("Description:") {
                extract_between(line, "Description: \"", "\"").map(String::from)
            } else if line.contains("Title:") {
                extract_between(line, "Title: \"", "\"").map(String::from)
            } else {
                None
            };

            // Calculate importance score (1-10)
            // More important: clicks, typing in text fields, button actions
            // Less important: focus changes, hovering
            let importance_score = match (action_type, element_role.as_str()) {
                ("Click", _) => 9, // Clicks are almost always important
                ("ValueChanged", "TextField") | ("ValueChanged", "TextArea") => 8, // Typing text
                ("ValueChanged", "Button") => 7, // Button state changes
                ("KeyPress", _) => 7, // Key presses
                ("FocusChanged", "Button") | ("FocusChanged", "TextField") => 5, // Focus on interactive elements
                ("FocusChanged", _) => 3, // General focus changes
                (_, _) => 4, // Other events
            };

            // Create a unique signature for this action to avoid duplicates
            let action_signature = format!(
                "{}|{}|{}|{}|{}", 
                action_type, 
                element_role, 
                element_value.as_deref().unwrap_or(""),
                element_description.as_deref().unwrap_or(""),
                activity.window_title
            );

            // Skip if this exact action was already seen (avoid duplicates)
            if seen_actions.contains(&action_signature) && 
               // Allow duplicates for text input (typing) and clicks, as these might be intentional
               !(action_type == "ValueChanged" && 
                 (element_role == "TextField" || element_role == "TextArea")) &&
               !(action_type == "Click") {
                continue;
            }

            // Add to seen actions
            seen_actions.insert(action_signature);

            // Skip low importance events
            if importance_score <= 2 {
                continue;
            }

            // Parse timestamp
            let timestamp = match DateTime::parse_from_rfc3339(&activity.timestamp) {
                Ok(dt) => dt.with_timezone(&Local),
                Err(_) => Local::now(), // Fallback to current time
            };

            // Create and add the automation log
            let automation_log = AutomationLog {
                id: 0, // Will be set by the database
                automation_id,
                timestamp: timestamp.to_rfc3339(),
                action_type: action_type.to_string(),
                element_role: if element_role.is_empty() { None } else { Some(element_role) },
                element_value,
                element_description,
                window_title: activity.window_title.clone(),
                window_app_name: activity.window_app_name.clone(),
                importance_score,
                raw_event_data: Some(line.to_string()),
                sequence_number,
            };

            filtered_logs.push(automation_log);
            sequence_number += 1;
        }
    }

    info!("Filtered {} events down to {} important actions", activities_count, filtered_logs.len());

    // Save all filtered logs to the database
    let transaction = db.transaction()?;
    for log in &filtered_logs {
        save_automation_log(&transaction, log)?;
    }
    transaction.commit()?;

    Ok(filtered_logs.len())
}

/// Helper function to extract text between two markers
fn extract_between<'a>(text: &'a str, start_marker: &str, end_marker: &str) -> Option<&'a str> {
    text.find(start_marker)
        .and_then(|start_idx| {
            let start = start_idx + start_marker.len();
            text[start..].find(end_marker)
                .map(|end_idx| &text[start..start + end_idx])
        })
}

/// Generate a human-readable summary of an automation based on filtered logs
pub fn generate_automation_summary(
    db: &mut Connection,
    automation_id: i64,
) -> Result<AutomationSummary, Error> {
    // Get the automation details
    let automation = db.query_row(
        "SELECT name, objective, created_at FROM automations WHERE id = ?",
        params![automation_id],
        |row| {
            Ok((
                row.get::<_, String>("name")?,
                row.get::<_, String>("objective")?,
                row.get::<_, String>("created_at")?,
            ))
        },
    )?;

    // Get the automation logs
    let logs = get_automation_logs(db, automation_id)?;

    if logs.is_empty() {
        // If no logs exist, first process them
        process_activity_logs_for_automation(db, automation_id)?;
        // Try fetching logs again
        let logs = get_automation_logs(db, automation_id)?;
        if logs.is_empty() {
            return Err(Error::QueryReturnedNoRows);
        }
    }

    // Calculate duration
    let start_time = DateTime::parse_from_rfc3339(&logs.first().unwrap().timestamp)
        .map_err(|_| Error::InvalidParameterName("Invalid timestamp format".to_string()))?;
    let end_time = DateTime::parse_from_rfc3339(&logs.last().unwrap().timestamp)
        .map_err(|_| Error::InvalidParameterName("Invalid timestamp format".to_string()))?;
    let duration_seconds = (end_time - start_time).num_seconds();

    // Count unique applications
    let unique_apps: HashSet<String> = logs.iter()
        .map(|log| log.window_app_name.clone())
        .collect();
    let app_count = unique_apps.len() as i32;

    // Generate human-readable steps from logs
    let steps = generate_steps(&logs);

    Ok(AutomationSummary {
        automation_id,
        name: automation.0,
        objective: automation.1,
        steps,
        duration_seconds,
        app_count,
        created_at: automation.2,
    })
}

/// Generate human-readable steps from filtered logs
fn generate_steps(logs: &[AutomationLog]) -> Vec<AutomationStep> {
    let mut steps = Vec::new();
    let mut current_app = String::new();
    let mut step_number = 1;
    let mut current_window = String::new();

    for (i, log) in logs.iter().enumerate() {
        // Check if app changed
        if log.window_app_name != current_app {
            // Add a step for switching to a new application
            steps.push(AutomationStep {
                step_number,
                description: format!("Switch to {}", log.window_app_name),
                action_type: "SwitchApplication".to_string(),
                target: Some(log.window_app_name.clone()),
                parameters: None,
            });
            current_app = log.window_app_name.clone();
            step_number += 1;
            current_window = log.window_title.clone();
        }
        // Check if window changed within the same app
        else if log.window_title != current_window {
            // Add a step for navigation within the app
            steps.push(AutomationStep {
                step_number,
                description: format!("Navigate to {} in {}", log.window_title, log.window_app_name),
                action_type: "Navigate".to_string(),
                target: Some(log.window_title.clone()),
                parameters: None,
            });
            current_window = log.window_title.clone();
            step_number += 1;
        }

        // Process each action based on type
        match log.action_type.as_str() {
            "Click" => {
                let target = log.element_description.clone()
                    .or_else(|| log.element_value.clone())
                    .unwrap_or_else(|| log.element_role.clone().unwrap_or_else(|| "element".to_string()));
                
                steps.push(AutomationStep {
                    step_number,
                    description: format!("Click on {}", target),
                    action_type: "Click".to_string(),
                    target: Some(target),
                    parameters: None,
                });
                step_number += 1;
            },
            "ValueChanged" => {
                // Check if this is typing text
                if let Some(ref role) = log.element_role {
                    if role == "TextField" || role == "TextArea" {
                        if let Some(ref value) = log.element_value {
                            // Check if there's a next log that's also typing in this field
                            let mut full_text = value.clone();
                            let mut j = i + 1;
                            
                            // Combine consecutive typing actions in the same field
                            while j < logs.len() {
                                let next_log = &logs[j];
                                if next_log.action_type == "ValueChanged" && 
                                   next_log.element_role == log.element_role &&
                                   next_log.window_title == log.window_title {
                                    if let Some(ref next_value) = next_log.element_value {
                                        full_text = next_value.clone();
                                        j += 1;
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                            
                            // Skip ahead if we combined multiple typing actions
                            if j > i + 1 {
                                // We'll create just one step for this sequence
                                let field_name = log.element_description.clone()
                                    .unwrap_or_else(|| "text field".to_string());
                                steps.push(AutomationStep {
                                    step_number,
                                    description: format!("Type \"{}\" in {}", full_text, field_name),
                                    action_type: "Type".to_string(),
                                    target: Some(field_name),
                                    parameters: Some(serde_json::json!({ "text": full_text })),
                                });
                                step_number += 1;
                                continue;
                            }
                        }
                    }
                }
                
                // Handle other value changes (sliders, checkboxes, etc.)
                let element_desc = log.element_description.clone()
                    .or_else(|| log.element_role.clone())
                    .unwrap_or_else(|| "element".to_string());
                
                let value_desc = log.element_value.clone()
                    .unwrap_or_else(|| "new value".to_string());
                
                steps.push(AutomationStep {
                    step_number,
                    description: format!("Set {} to {}", element_desc, value_desc),
                    action_type: "SetValue".to_string(),
                    target: Some(element_desc),
                    parameters: Some(serde_json::json!({ "value": value_desc })),
                });
                step_number += 1;
            },
            "KeyPress" => {
                // Only include important key presses (Enter, Tab, etc.)
                if let Some(ref value) = log.element_value {
                    if value.contains("Enter") || value.contains("Return") || 
                       value.contains("Tab") || value.contains("Escape") {
                        steps.push(AutomationStep {
                            step_number,
                            description: format!("Press {} key", value),
                            action_type: "KeyPress".to_string(),
                            target: None,
                            parameters: Some(serde_json::json!({ "key": value })),
                        });
                        step_number += 1;
                    }
                }
            },
            _ => {
                // Skip other action types
            }
        }
    }

    steps
}

/// Generate a script for an LLM to execute for this automation
pub fn generate_llm_script(
    db: &mut Connection,
    automation_id: i64,
) -> Result<String, Error> {
    let logs = get_automation_logs(db, automation_id)?;
    
    if logs.is_empty() {
        return Err(Error::QueryReturnedNoRows);
    }
    
    let mut script = String::new();
    
    // Add metadata
    let automation = db.query_row(
        "SELECT name, objective FROM automations WHERE id = ?",
        params![automation_id],
        |row| {
            Ok((
                row.get::<_, String>("name")?,
                row.get::<_, String>("objective")?,
            ))
        },
    )?;
    
    script.push_str(&format!("# Automation: {}\n", automation.0));
    script.push_str(&format!("# Objective: {}\n", automation.1));
    script.push_str("# Generated Script for LLM Execution\n\n");
    
    // Group actions by application
    let mut current_app = String::new();
    let mut current_window = String::new();
    
    for log in &logs {
        // Add app switching if needed
        if log.window_app_name != current_app {
            script.push_str(&format!("\n## Open application: {}\n", log.window_app_name));
            current_app = log.window_app_name.clone();
            current_window = log.window_title.clone();
            script.push_str(&format!("# Window: {}\n", log.window_title));
        }
        // Add window navigation if needed
        else if log.window_title != current_window {
            script.push_str(&format!("\n# Navigate to window: {}\n", log.window_title));
            current_window = log.window_title.clone();
        }
        
        // Add the action details
        match log.action_type.as_str() {
            "Click" => {
                let target = log.element_description.clone()
                    .or_else(|| log.element_value.clone())
                    .unwrap_or_else(|| log.element_role.clone().unwrap_or_else(|| "unknown".to_string()));
                
                script.push_str(&format!("click(\"{}\") # Role: {}\n", 
                    target, 
                    log.element_role.clone().unwrap_or_else(|| "unknown".to_string())
                ));
            },
            "ValueChanged" => {
                if let Some(ref role) = log.element_role {
                    if role == "TextField" || role == "TextArea" {
                        if let Some(ref value) = log.element_value {
                            let field_desc = log.element_description.clone()
                                .unwrap_or_else(|| "text field".to_string());
                            
                            script.push_str(&format!("type_text(\"{}\", \"{}\") # {}\n",
                                field_desc,
                                value,
                                role
                            ));
                        }
                    } else {
                        // Handle other input types (checkboxes, radio buttons, etc.)
                        let element_desc = log.element_description.clone()
                            .or_else(|| log.element_role.clone())
                            .unwrap_or_else(|| "unknown".to_string());
                        
                        if let Some(ref value) = log.element_value {
                            script.push_str(&format!("set_value(\"{}\", \"{}\") # {}\n",
                                element_desc,
                                value,
                                role
                            ));
                        }
                    }
                }
            },
            "KeyPress" => {
                if let Some(ref value) = log.element_value {
                    script.push_str(&format!("press_key(\"{}\") # Action: KeyPress\n", value));
                }
            },
            _ => {
                // Include important but less common actions
                if log.importance_score >= 5 {
                    script.push_str(&format!("# Action: {} on {}\n", 
                        log.action_type,
                        log.element_role.clone().unwrap_or_else(|| "unknown".to_string())
                    ));
                }
            }
        }
    }
    
    Ok(script)
}

/// Updates the automation's generalized_script field with LLM-generated script
pub fn update_automation_generalized_script(
    db: &mut Connection, 
    automation_id: i64, 
    generalized_script: &str
) -> Result<(), Error> {
    db.execute(
        "UPDATE automations SET generalized_script = ?, updated_at = datetime('now') WHERE id = ?",
        params![generalized_script, automation_id],
    )?;
    
    Ok(())
} 