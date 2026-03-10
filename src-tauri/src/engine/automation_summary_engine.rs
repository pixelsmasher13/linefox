use log::{info, error};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use chrono::DateTime;

use crate::configuration::state::ServiceAccess;
use crate::repository::settings_repository::get_setting;
use crate::entity::automation::Automation;
use crate::entity::activity_item::ActivityItem;

// New struct for LLM-filtered steps
#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FilteredAutomationStep {
    pub step_number: i32,
    pub app_name: String,
    pub action: String,
    pub element_info: Option<String>,
    pub screen_context: Option<String>,
    pub raw_action: Option<String>,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FilteredAutomation {
    pub automation_id: i64,
    pub name: String,
    pub steps: Vec<FilteredAutomationStep>,
}

// Enhanced struct to include both filtered automation and intake questions
#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AutomationAnalysisResult {
    pub automation_id: i64,
    pub name: String,
    pub steps: Vec<FilteredAutomationStep>,
    pub suggested_questions: Option<Vec<String>>,
}

// Structs for Claude API
#[allow(dead_code)]
#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<Message>,
    system: String,
    stream: bool,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<Content>,  
    usage: Usage,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Content {
    text: String,
}

#[allow(dead_code)]
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
#[allow(dead_code)]
const ANTHROPIC_MODEL: &str = "claude-3-5-haiku-20241022";
#[allow(dead_code)]
const ANTHROPIC_LARGER_MODEL: &str = "claude-3-7-sonnet-20250219";

/// Generate a human-readable summary for an automation using LLM
pub async fn generate_automation_summary_with_llm(
    app_handle: &AppHandle,
    automation_id: i64,
) -> Result<String, String> {
    // Get the automation details
    let automation = app_handle
        .db(|db| crate::repository::automation_repository::get_automation_by_id(db, automation_id))
        .map_err(|e| format!("Failed to get automation: {}", e))?
        .ok_or_else(|| format!("Automation with ID {} not found", automation_id))?;
    
    // Check if we have a generalized script to use
    if automation.generalized_script.is_empty() || automation.generalized_script == "[]" {
        // If no generalized script exists, try to process the raw events to create one
        info!("No generalized script found for automation {}. Attempting to process raw events.", automation_id);
        
        match process_automation_with_llm(app_handle, automation_id).await {
            Ok((_script, _questions)) => {
                info!("Successfully generated generalized script for automation {}", automation_id);
                
                // Now that we have a script, continue with summary generation
                let nl_description = generate_nl_description_from_script(app_handle, &automation).await?;
                return Ok(nl_description);
            },
            Err(e) => {
                return Err(format!("Failed to generate generalized script for automation {}: {}", automation_id, e));
            }
        }
    }
    
    // Call LLM to get a natural language description based on the generalized script
    let nl_description = generate_nl_description_from_script(app_handle, &automation).await?;
    
    info!("Generated human-readable description for automation {}", automation_id);
    
    Ok(nl_description)
}

/// Call Claude to generate a natural language description using the existing generalized script
async fn generate_nl_description_from_script(
    app_handle: &AppHandle,
    automation: &Automation,
) -> Result<String, String> {
    // Get API choice setting
    let api_choice = app_handle
        .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
        .setting_value;

    // Get the appropriate API key based on the choice
    let api_key = match api_choice.as_str() {
        "gemini" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_gemini").expect("Failed to get Gemini API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Gemini API key is not configured".to_string());
            }
            key
        },
        "proxy" => {
            // Get JWT token for proxy authentication
            let user_id = app_handle
                .db(|db| match get_setting(db, "user_id") {
                    Ok(setting) => Some(setting.setting_value),
                    Err(_) => None,
                })
                .unwrap_or_default();

            if user_id.is_empty() {
                return Err("User not authenticated - please login first".to_string());
            }

            // Get valid auth token for the user
            let jwt_token = app_handle
                .db(|db| crate::repository::user_auth_repository::get_valid_auth_token(db, &user_id))
                .map_err(|e| format!("Failed to get auth token: {}", e))?
                .ok_or_else(|| "Authentication expired - please login again".to_string())?;

            jwt_token
        },
        "openai" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_open_ai").expect("Failed to get OpenAI API key"))
                .setting_value;
            if key.is_empty() {
                return Err("OpenAI API key is not configured".to_string());
            }
            key
        },
        "grok" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_grok").expect("Failed to get Grok API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Grok API key is not configured".to_string());
            }
            key
        },
        "deepseek" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_deepseek").expect("Failed to get DeepSeek API key"))
                .setting_value;
            if key.is_empty() {
                return Err("DeepSeek API key is not configured".to_string());
            }
            key
        },
        "claude" | _ => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_claude").expect("Failed to get Claude API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Claude API key is not configured".to_string());
            }
            key
        }
    };
    
    // Create LLM prompt using the generalized_script
    let prompt = format!(
        "# Automation Summary Request\n\n## Automation Details\nName: {}\nUser Objective: {}\n\n## Task\nCreate a step-by-step human-readable summary of the key interactions captured in the generalized script. The user defined their objective as '{}', and the script below represents the key actions that were recorded to accomplish this. Create a clear, numbered list of steps that describes these actions in natural language.\n\n## Generalized Script:\n```\n{}\n```",
        automation.name,
        automation.objective,
        automation.objective,
        automation.generalized_script
    );
    
    let system_prompt = r#"You are an expert automation script analyzer. Your task is to:

1. Create a clear, concise list of steps describing what this automation does in human-readable language.
2. Each step should be numbered and written in imperative form (e.g., "1. Open Google Chrome").
3. Make the steps specific but understandable to non-technical users.
4. Combine small, related actions into logical steps (e.g., combine "Click search box" + "Type query" into "Search for [query]").
5. Include all important actions but omit technical details unless necessary for clarity.
6. Focus only on the actual user-visible actions that contribute to the automation objective.
7. Use simple, direct language that describes what the automation does from a user perspective.
8. IMPORTANT: When you see generalized placeholders (like HEADLINE_1, EMAIL_ADDRESS, SEARCH_TERM), keep them as placeholders in brackets for now. They will be replaced with specific values later through user clarification.
9. Be as specific as possible about:
   - Exact button names and UI elements
   - Specific websites or applications mentioned in the objective
10. Handling of COPY operations
   - If copying entire contents of the page: "Copy the full [document type] content"
   - If extracting specific data: "Extract [specific data] and save to MEMORY"
11. Handling of PASTE operations
   - if users inserted the contents using Paste command / CMD +V, etc - keep if user used CMD+A / CTRL+A / Select All (full contents copy). OTHERWISE change to TYPE (because we will want that same action to be executed via TYPE the AI agent later)

CRITICAL FOR CLARITY - AVOID AMBIGUITY:
- For time-based objectives: Explicitly state date/time filtering criteria
  BAD: "Message recent LinkedIn connections"
  GOOD: "Message LinkedIn connections from the past 7 days by checking 'Connected on' date before sending"
- For repeated steps: Always specify completion conditions
  BAD: "Repeat steps 4-9 for each contact"  
  GOOD: "Repeat steps 4-9 for each contact that has 'Director' or 'VP' in their title, continuing until the last visible contact is processed"
- For "all items" objectives: Define what constitutes completion
  BAD: "Save all matching profiles"
  GOOD: "Save all profiles where location includes 'San Francisco', scrolling down to load more until 'No more results' appears"

CRITICAL: Detect and add missing intermediate steps that are logically required:
- If script shows "Open Word" then "Paste text", add "Create new blank document" or "Click Continue if prompted"
- If script shows "Open app" then interaction with specific UI, add navigation steps like "Navigate to [expected view]"
- If script shows typing followed by form submission but no Enter/Submit, add "Press Enter" or "Click Submit"
- If script shows opening a save dialog then typing filename but no save action, add "Click Save button"
- If script shows repeated copying of specific items, clarify: "Extract [item type] to memory" vs "Copy full content"
- Think: What intermediate UI states would a human encounter between the recorded actions?

Common patterns to watch for:
- App launch → Working state (may need: dismiss welcome screens, create new file, navigate to main view)
- Copy action → Paste action (may need: switch apps, create/open destination document)
- Search/type → Results (may need: press Enter, click Search button)
- Fill form → Submit (may need: click Submit/Save/OK button)

The input is a JSON structure containing the filtered key interactions that were recorded. Your task is to translate this technical representation into a clear human-readable list that an end user can understand, filling in obvious gaps.
The output should be well-formatted Markdown where each step is clearly numbered.
Limit your response to exactly the steps, without introductions, conclusions or explanations."#;
    
    // Call the appropriate LLM API based on api_choice
    match api_choice.as_str() {
        "gemini" => {
            info!("Using Gemini API for automation summary generation");
            let (response_text, input_tokens, output_tokens) =
                crate::engine::llm_providers::gemini::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000
                ).await?;

            info!(
                "Gemini token usage for summary - Input: {}, Output: {}",
                input_tokens, output_tokens
            );

            Ok(response_text.trim().to_string())
        },
        "proxy" => {
            info!("Using Heelix Cloud proxy for automation summary generation");
            let (response_text, input_tokens, output_tokens) =
                crate::engine::llm_providers::proxy::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000
                ).await?;

            info!(
                "Proxy token usage for summary - Input: {}, Output: {}",
                input_tokens, output_tokens
            );

            Ok(response_text.trim().to_string())
        },
        "openai" => {
            info!("Using OpenAI API for automation summary generation");
            let (response_text, input_tokens, output_tokens) =
                crate::engine::llm_providers::openai::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000
                ).await?;

            info!(
                "OpenAI token usage for summary - Input: {}, Output: {}",
                input_tokens, output_tokens
            );

            Ok(response_text.trim().to_string())
        },
        "grok" => {
            info!("Using Grok API for automation summary generation");
            let (response_text, input_tokens, output_tokens) =
                crate::engine::llm_providers::grok::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000
                ).await?;

            info!(
                "Grok token usage for summary - Input: {}, Output: {}",
                input_tokens, output_tokens
            );

            Ok(response_text.trim().to_string())
        },
        "deepseek" => {
            info!("Using DeepSeek API for automation summary generation");
            let (response_text, input_tokens, output_tokens) =
                crate::engine::llm_providers::deepseek::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000
                ).await?;

            info!(
                "DeepSeek token usage for summary - Input: {}, Output: {}",
                input_tokens, output_tokens
            );

            Ok(response_text.trim().to_string())
        },
        "claude" | _ => {
            info!("Using Claude API for automation summary generation");
            let (response_text, input_tokens, output_tokens) =
                crate::engine::llm_providers::claude::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000
                ).await?;

            info!(
                "Claude token usage for summary - Input: {}, Output: {}",
                input_tokens, output_tokens
            );

            Ok(response_text.trim().to_string())
        }
    }
}

/// Combine raw events into a script for future LLM processing
pub async fn update_all_automations_with_summaries(app_handle: &AppHandle) -> Result<usize, String> {
    // Get all automations
    let automations = app_handle
        .db(|db| crate::repository::automation_repository::get_all_automations(db))
        .map_err(|e| format!("Failed to get automations: {}", e))?;
    
    let mut success_count = 0;
    
    // Process each automation
    for automation in automations {
        if automation.generalized_script.is_empty() || automation.generalized_script == "[]" {
            match generate_automation_summary_with_llm(app_handle, automation.id).await {
                Ok(_) => {
                    success_count += 1;
                    info!("Successfully updated automation {}: {}", automation.id, automation.name);
                }
                Err(e) => {
                    error!("Failed to update automation {}: {}", automation.id, e);
                }
            }
        }
    }
    
    Ok(success_count)
}

/// Process raw activity logs using LLM to filter and format important events
/// 
/// This function creates a generalized automation script that can be reused for similar objectives.
/// It intelligently preserves objective-specific elements (like specific websites mentioned in the objective)
/// while generalizing dynamic content (like headlines, search terms, email addresses, etc.) to make
/// the automation more reusable.
/// 
/// Example: For objective "Read headlines on CNN"
/// - Preserves: "Navigate to cnn.com" (part of objective)  
/// - Generalizes: "Click headline 'Breaking News'" → "Click on HEADLINE_1"
/// 
/// Also generates suggested clarification questions based on the analysis
pub async fn process_automation_with_llm(
    app_handle: &AppHandle,
    automation_id: i64
) -> Result<(String, Vec<String>), String> {
    // Get the automation details
    let automation = app_handle
        .db(|db| crate::repository::automation_repository::get_automation_by_id(db, automation_id))
        .map_err(|e| format!("Failed to get automation: {}", e))?
        .ok_or_else(|| format!("Automation with ID {} not found", automation_id))?;
    
    // Fetch all raw activity logs for this automation
    let activity_logs = app_handle
        .db(|db| {
            db.prepare(
                "SELECT * FROM activity_logs 
                 WHERE automation_id = ? 
                 ORDER BY timestamp ASC"
            )
            .and_then(|mut stmt| {
                let rows = stmt.query_map([automation_id], |row| {
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
                
                let mut logs = Vec::new();
                for row in rows {
                    logs.push(row?);
                }
                Ok(logs)
            })
        })
        .map_err(|e| format!("Failed to fetch activity logs: {}", e))?;
    
    info!("Found {} raw activity logs for automation {}", activity_logs.len(), automation_id);
    
    if activity_logs.is_empty() {
        return Err(format!("No activity logs found for automation {}", automation_id));
    }
    
    // Prepare the data for LLM analysis
    let prompt = prepare_activity_logs_for_llm(&activity_logs, &automation);
    
    // Call LLM to analyze the events and generate questions
    let (filtered_json, questions) = filter_events_with_llm(app_handle, &prompt).await?;
    
    // Save the filtered JSON as the generalized script
    app_handle
        .db_mut(|db| {
            db.execute(
                "UPDATE automations SET generalized_script = ?, updated_at = datetime('now') WHERE id = ?",
                [&filtered_json, &automation_id.to_string()]
            )
            .map(|_| ())
            .map_err(|e| format!("Failed to update automation: {}", e))
        })
        .map_err(|e| format!("Database error: {}", e))?;
    
    info!("Successfully saved LLM-filtered events for automation {} with {} suggested questions", 
          automation_id, questions.len());
    
    Ok((filtered_json, questions))
}

/// Prepare activity logs for LLM processing
fn prepare_activity_logs_for_llm(
    activity_logs: &[ActivityItem],
    automation: &Automation
) -> String {
    let mut prompt = format!(
        "# Automation Recording Analysis\n\n## Automation Info\nName: {}\nObjective: {}\n",
        automation.name,
        automation.objective
    );
    
    // Add more specific context about this automation's purpose and generalization needs
    prompt.push_str("\n## Recording Purpose & Generalization Guidelines\n");
    prompt.push_str(&format!("This recording captures user actions to automate the following task: \"{}\". ", 
                            automation.objective));
    prompt.push_str("When creating the generalized script, focus on identifying key interactions ");
    prompt.push_str("that directly contribute to achieving this specific objective. ");
    prompt.push_str("Actions that seem exploratory, mistaken, or unrelated to the objective should be filtered out.\n\n");
    
    // Add objective-specific generalization guidance
    prompt.push_str("**IMPORTANT FOR CONTENT GENERALIZATION**: ");
    prompt.push_str(&format!("Based on the objective \"{}\", preserve any specific applications, websites, or services mentioned in the objective, ", automation.objective));
    prompt.push_str("but generalize dynamic content like specific text, names, dates, search terms, headlines, etc. ");
    prompt.push_str("that would change between different runs of this automation.\n\n");
    
    // Analyze objective to provide specific guidance
    let objective_lower = automation.objective.to_lowercase();
    if objective_lower.contains("email") {
        prompt.push_str("**Email Automation Detected**: Generalize recipient addresses, subject lines, and message content.\n");
    }
    if objective_lower.contains("search") {
        prompt.push_str("**Search Automation Detected**: Generalize search terms unless they're core to the objective.\n");
    }
    if objective_lower.contains("read") || objective_lower.contains("news") || objective_lower.contains("article") {
        prompt.push_str("**Content Reading Automation Detected**: Generalize article titles, headlines, and content text.\n");
    }
    if objective_lower.contains("shop") || objective_lower.contains("buy") || objective_lower.contains("purchase") {
        prompt.push_str("**Shopping Automation Detected**: Generalize product names, prices, and specific item details.\n");
    }
    
    prompt.push_str("\n## Raw Activity Logs:\n\n");
    
    // Process logs in chronological order
    for (i, log) in activity_logs.iter().enumerate() {
        // Add time marker
        if let Ok(dt) = DateTime::parse_from_rfc3339(&log.timestamp) {
            prompt.push_str(&format!("### Time: {} | App: {}\n", 
                dt.format("%H:%M:%S"),
                log.window_app_name
            ));
        }
        
        // Add window title
        prompt.push_str(&format!("Window: {}\n", log.window_title));
        
        // Add detected actions (if any)
        if !log.detected_actions.is_empty() && log.detected_actions != "/" {
            prompt.push_str("Actions:\n");
            for line in log.detected_actions.lines() {
                if !line.trim().is_empty() {
                    prompt.push_str(&format!("- {}\n", line.trim()));
                }
            }
        }
        
        // Include condensed element tree info (every 5th entry or on app change)
        // This provides context about what's visible on screen
        if i == 0 || i % 5 == 0 || (i > 0 && activity_logs[i-1].window_app_name != log.window_app_name) {
            // Limit element tree to a reasonable size
            let context = if log.element_tree_dump.len() > 500 {
                // Take a representative sample
                format!("{}\n... [truncated] ...", log.element_tree_dump[..500].to_string())
            } else {
                log.element_tree_dump.clone()
            };
            
            prompt.push_str("Screen Content (sample):\n");
            prompt.push_str(&format!("```\n{}\n```\n", context));
        }
        
        prompt.push_str("\n");
    }
    
    prompt
}

/// Use LLM to filter and format the important events
async fn filter_events_with_llm(
    app_handle: &AppHandle,
    prompt: &str
) -> Result<(String, Vec<String>), String> {
    // Get API choice setting
    let api_choice = app_handle
        .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
        .setting_value;

    // Get the appropriate API key based on the choice
    let api_key = match api_choice.as_str() {
        "gemini" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_gemini").expect("Failed to get Gemini API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Gemini API key is not configured".to_string());
            }
            key
        },
        "proxy" => {
            // Get JWT token for proxy authentication
            let user_id = app_handle
                .db(|db| match get_setting(db, "user_id") {
                    Ok(setting) => Some(setting.setting_value),
                    Err(_) => None,
                })
                .unwrap_or_default();

            if user_id.is_empty() {
                return Err("User not authenticated - please login first".to_string());
            }

            // Get valid auth token for the user
            let jwt_token = app_handle
                .db(|db| crate::repository::user_auth_repository::get_valid_auth_token(db, &user_id))
                .map_err(|e| format!("Failed to get auth token: {}", e))?
                .ok_or_else(|| "Authentication expired - please login again".to_string())?;

            jwt_token
        },
        "openai" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_open_ai").expect("Failed to get OpenAI API key"))
                .setting_value;
            if key.is_empty() {
                return Err("OpenAI API key is not configured".to_string());
            }
            key
        },
        "grok" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_grok").expect("Failed to get Grok API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Grok API key is not configured".to_string());
            }
            key
        },
        "deepseek" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_deepseek").expect("Failed to get DeepSeek API key"))
                .setting_value;
            if key.is_empty() {
                return Err("DeepSeek API key is not configured".to_string());
            }
            key
        },
        "claude" | _ => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_claude").expect("Failed to get Claude API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Claude API key is not configured".to_string());
            }
            key
        }
    };
    
    
    // System prompt to guide the LLM
    let system_prompt = r#"You are analyzing a recording of user actions to create a structured representation of what the user did to accomplish a specific automation objective. Follow these instructions:

## CORE ANALYSIS PRINCIPLES
1. Review the raw accessibility events and create a structured list preserving the EXACT element descriptions, values, and action types from the raw data with the exception of converting Copy and Paste actions as appripriate based on description below
2. Focus on capturing intentional user behavior (clicks, typing, navigation) not random focus changes or UI updates
3. IMPORTANT: Include ALL navigation steps such as opening websites, entering URLs, and searching for terms
4. Include detailed browser actions such as typing in search boxes, address bars, and form fields
5. Capture complete workflow sequences - do not skip intermediate steps even if they seem obvious
6. Include context about what was visible on screen at key points (especially when switching applications or before important interactions)
7. **PRESERVE RAW DATA**: When creating the action descriptions, use the EXACT element descriptions and values from the raw accessibility events
   - If raw shows: AXTextField with Description: "Address and search bar", use EXACTLY "Address and search bar"
   - If raw shows: AXButton with Title: "Search", use EXACTLY "Search"
   - Do NOT interpret or rewrite these into more human-friendly descriptions
8. Group related actions (e.g., clicking a field and typing) into single logical steps
   - IMPORTANT: When multiple form fields are filled sequentially, combine them into a single TYPE action with ::: separated element:text pairs
   - Example: Instead of "TYPE:5:john@email.com" then "TYPE:8:password123", use "TYPE:5:john@email.com:::8:password123"
9. For the "action" field, format it as: "{ACTION_TYPE} on {element_description/value}" where ACTION_TYPE matches the raw event (e.g., "AXValueChanged" → "TYPE", "AXFocusedUIElementChanged" → "FOCUS", etc.)
10. Include timestamp, app name, and window title to maintain context
11. Only remove actions that are clearly mistakes or completely unrelated to the objective
12. **COPY ACTIONS**: Convert PARTIAL data copy actions to MEMORY_SAVE actions; keep full-screen copies as actual user steps. i.e if a user copies a name from the page - convert that action to MEMORY_SAVE. If user copies entire book visible on web page - use the command they used (i.e CMD+C)
13. CMD+V, CTRL+V or Paste keyboard actions: Convert to TYPE actions IF user just used CMD+A, CTRL+A / Select ALL  and Copy (and therefore we used MEMORY_SAVE to save that data). i.e i.e if user copied a section of LinkedIN profile and is now pasting it. Leave as is for actions where user copied full text (generall through Selecting CMD+A, CTRL+A first)

## CRITICAL: HANDLING TIME-BASED AND "ALL ITEMS" OBJECTIVES
When the objective mentions time constraints or processing "all" items:
- **Time-based filtering**: If objective says "today", "current day", "this week", etc., include explicit filtering steps
  Example: "Message LinkedIn contacts who joined THIS WEEK" → Add step: "Check each contact's 'Connected on' date and only proceed if within last 7 days"
- **"All items" completion**: When objective says "all", "every", "each", define clear completion criteria
  Example: "Send connection requests to ALL marketing managers" → Add steps for: identifying total matches, tracking who was contacted, scrolling to load more results
- **Loop termination**: For repeated sequences, specify WHEN to stop repeating
  BAD: "Repeat steps 3-8"
  GOOD: "Repeat steps 3-8 for each LinkedIn contact that matches 'Software Engineer' title until no more contacts appear when scrolling"

## INTELLIGENT CONTENT GENERALIZATION
**CRITICAL**: Use reasoning to intelligently determine what content should be generalized vs preserved based on the automation objective.

### GENERALIZATION REASONING FRAMEWORK:
Analyze each piece of content and ask:
1. **Is this mentioned in or core to the objective?** → KEEP SPECIFIC
2. **Will this be different each time the automation runs?** → GENERALIZE  
3. **Is this a UI element or navigation path needed for the workflow?** → KEEP SPECIFIC
4. **Is this dynamic user data or time-sensitive content?** → GENERALIZE

**→ CONVERT TO MEMORY_SAVE** when copying:
- Select contents of the page
Action: "Extract [data] to memory" | Raw: "MEMORY_SAVE:[content]"

**→ KEEP AS CLIPBOARD** when copying:
- Entire documents (generally with cmd+A first pressed)
- For immediate paste elsewhere
Action: "Copy entire content" | Raw: "PRESS:cmd+C"

### DECISION PROCESS:
For each piece of content, reason through:
- **Static vs Dynamic**: Will this content change between automation runs?
- **Objective Relevance**: Is this specifically mentioned in the user's stated goal?
- **Structural vs Content**: Is this part of the interface/workflow or the data being processed?

### GENERALIZATION PATTERNS:
When you determine content should be generalized, use these numbered patterns:
- Sequential items: ITEM_1, ITEM_2, ITEM_3 (for lists, search results, etc.)
- User input: USER_INPUT, EMAIL_ADDRESS, FILENAME, etc.
- Time-sensitive: CURRENT_DATE, RECENT_ITEM, etc.
- Form fields: When multiple fields filled sequentially, combine: "TYPE:5:EMAIL_ADDRESS:::8:PASSWORD:::12:USER_NAME"

### REASONING EXAMPLES:
**Objective: "Find top 5 articles on tech news site"**
- ✅ KEEP: "technews.com" (mentioned in objective)
- ✅ KEEP: "top 5" (specified quantity in objective)  
- ❌ GENERALIZE: "AI Revolution in 2024" → "ARTICLE_1" (dynamic content, changes daily)
- ❌ GENERALIZE: "New iPhone Features" → "ARTICLE_2" (dynamic content)

**Objective: "Send email to my manager about project status"**
- ✅ KEEP: "Gmail", "Compose" (workflow elements)
- ❌ GENERALIZE: "john@company.com" → "MANAGER_EMAIL" (varies by user)
- ❌ GENERALIZE: "Project Alpha completed" → "PROJECT_UPDATE" (user-specific content)

**Objective: "Read latest news on CNN"**
- ✅ KEEP: "cnn.com" (specifically mentioned)
- ❌ GENERALIZE: "Breaking: Election Results" → "HEADLINE_1" (changes constantly)

## CLARIFICATION QUESTIONS
Based on your analysis, generate 0-4 clarification questions ONLY for aspects that are:
1. **Outcome-critical**: Would change WHAT gets accomplished, not HOW
2. **Genuinely ambiguous content questions**: Cannot be inferred from the objective AND the recorded steps
3. **User-variable**: Would differ between users or runs
4. **Not already clear from the script**: If the steps show exactly what was done, don't ask about it

### DO NOT ASK ABOUT:
- **Navigation paths**: If objective is "go to CNN.com", don't ask "Should I go through Google first?" - the path doesn't matter
- **UI interactions**: Don't ask "Should I click the cookie banner?" - handle these automatically
- **Obvious prerequisites**: Don't ask "Should I open the browser first?" - of course you should
- **Implementation details**: The specific steps to achieve the goal (user cares about outcome, not method)

### DO ASK ABOUT:
- **Data specifics** when ambiguous: "Which report to download?" (if multiple exist) or which price to use (if user says "find me expensive homes")
- **Scope boundaries**: if user says find me a bunch of Linkedin contacts.
- **Failure handling**: "If no results found, try alternative search?"
- **Data variations**: "Overwrite existing files or rename?"

### DECISION FRAMEWORK:
For each potential question, ask yourself:
1. Would different answers lead to DIFFERENT END RESULTS?
2. Is this something the user would actually care about?
3. Can a reasonable default be assumed?

If answer to #1 is NO or #3 is YES → DON'T generate the question

### EXAMPLES:
**Objective: "Go to CNN.com and read the news"**
**Script shows: Navigate to cnn.com → Scroll through headlines**
→ ZERO questions (objective + steps are completely clear)

**Objective: "Download sales report"**
**Script shows: Login → Navigate to reports → Download REPORT_FILE**
❌ "Should I navigate via menu or search?" → Implementation already recorded
✅ "Which date range?" → Script shows generic REPORT_FILE, date range unclear

**Objective: "Send LinkedIn invites"**
**Script shows: Search "SOFTWARE_ENGINEER" → Send invites to RESULT_1, RESULT_2...**
❌ "What search terms to use?" → Script already shows the pattern
✅ "Include personalized message?" → Script doesn't show if message was added
✅ "Maximum number to send?" → Script shows pattern but no limit specified


Return ONLY a JSON object in this exact format:
{
  "automation_id": 123, // Use the actual ID from the data
  "name": "Example Automation", // Use the actual name from the data
  "steps": [
    {
      "step_number": 1,
      "app_name": "Google Chrome",
      "action": "TYPE on Address and search bar: google.com",
      "element_info": "AXTextField - Description: \"Address and search bar\"",
      "screen_context": "New tab page",
      "raw_action": "TYPE:Address and search bar:google.com"
    },
    {
      "step_number": 2,
      "app_name": "Google Chrome", 
      "action": "TYPE on Search: cnn.com",
      "element_info": "AXTextArea - Description: \"Search\"",
      "screen_context": "Google homepage",
      "raw_action": "TYPE:Search:cnn.com"
    },
    {
      "step_number": 3,
      "app_name": "Google Chrome",
      "action": "CLICK on CNN: Breaking News, Latest News and Videos",
      "element_info": "AXLink - Description: \"CNN: Breaking News, Latest News and Videos CNN https://www.cnn.com\"",
      "screen_context": "Google search results",
      "raw_action": "CLICK:CNN: Breaking News, Latest News and Videos"
    },
    {
      "step_number": 4,
      "app_name": "Google Chrome",
      "action": "Extract headline and save to memory",
      "element_info": "AXStaticText - Value: \"HEADLINE_1\"",
      "screen_context": "CNN homepage with multiple headlines",
      "raw_action": "MEMORY_SAVE:HEADLINE_1"
    },
    {
      "step_number": 5,
      "app_name": "Google Chrome",
      "action": "Copy entire article content",
      "element_info": "Full page selection",
      "screen_context": "Article page",
      "raw_action": "PRESS:cmd+a then PRESS:cmd+c"
    }
    // Additional steps...
  ],
  "suggested_questions": [
    "What email address should receive the exported file?",
    "Which date range should be used for the report?",
    "Should existing files be overwritten or renamed?"
  ]
}

Important:
- Return ONLY well-formed JSON, no explanation text before or after
- **PRESERVE EXACT ELEMENT DESCRIPTIONS**: Use the exact text from Description, Title, or Value fields as shown in raw events
- Map accessibility events to action types: AXValueChanged → TYPE, AXFocusedUIElementChanged with click → CLICK, keyboard shortcuts → KEYBOARD
- For the "action" field, use format: "{ACTION} on {exact_element_description}: {value}" (e.g., "TYPE on Address and search bar: google.com")
- For "element_info", preserve the raw element type and description (e.g., "AXTextField - Description: \"Address and search bar\"")
- Apply content generalization ONLY to dynamic values that change between runs (headlines, dates, etc.) but NEVER to UI element descriptions
- Do not skip or omit any steps required to completely reproduce the automation
- Include all navigation steps, URL entries, and search terms (with appropriate generalization of dynamic content only)
- Make sure the JSON is valid - all keys in proper quotes, commas where needed, etc.
- For screen_context, provide just a brief 5-10 word summary
- The generalized script should be reusable for similar objectives with different specific content
- Only include questions that are truly necessary - avoid generic questions if the automation is straightforward"#;
    
    // Call the appropriate LLM API based on api_choice
    let (response_text, input_tokens, output_tokens) = match api_choice.as_str() {
        "gemini" => {
            info!("Using Gemini API for automation event filtering");
            crate::engine::llm_providers::gemini::call_llm_api(
                &api_key, prompt.to_string(), &system_prompt, 8000
            ).await?
        },
        "proxy" => {
            info!("Using Heelix Cloud proxy for automation event filtering");
            crate::engine::llm_providers::proxy::call_llm_api(
                &api_key, prompt.to_string(), &system_prompt, 8000
            ).await?
        },
        "openai" => {
            info!("Using OpenAI API for automation event filtering");
            crate::engine::llm_providers::openai::call_llm_api(
                &api_key, prompt.to_string(), &system_prompt, 8000
            ).await?
        },
        "grok" => {
            info!("Using Grok API for automation event filtering");
            crate::engine::llm_providers::grok::call_llm_api(
                &api_key, prompt.to_string(), &system_prompt, 8000
            ).await?
        },
        "deepseek" => {
            info!("Using DeepSeek API for automation event filtering");
            crate::engine::llm_providers::deepseek::call_llm_api(
                &api_key, prompt.to_string(), &system_prompt, 8000
            ).await?
        },
        "claude" | _ => {
            info!("Using Claude API for automation event filtering");
            crate::engine::llm_providers::claude::call_llm_api(
                &api_key, prompt.to_string(), &system_prompt, 8000
            ).await?
        }
    };

    // Log token usage
    info!(
        "{} token usage for filtering - Input: {}, Output: {}",
        api_choice, input_tokens, output_tokens
    );

    // Extract the generated JSON
    let filtered_json = response_text.trim().to_string();
        
    // Log generalization insights
    let generalized_indicators = [
        "HEADLINE_", "SEARCH_TERM", "EMAIL_ADDRESS", "RECIPIENT_EMAIL",
        "NAME", "CONTACT_NAME", "DATE", "SELECTED_DATE", "FILENAME",
        "DOCUMENT_NAME", "TEXT_CONTENT", "MESSAGE_CONTENT", "TARGET_URL",
        "PRODUCT_NAME", "ITEM_NAME"
    ];

    let generalization_count = generalized_indicators.iter()
        .map(|indicator| filtered_json.matches(indicator).count())
        .sum::<usize>();

    if generalization_count > 0 {
        info!(
            "Content generalization applied: {} generalized elements detected in automation script",
            generalization_count
        );
    } else {
        info!("No content generalization indicators found - script may be highly specific to this run");
    }

    // Verify it's valid JSON before returning
    match serde_json::from_str::<serde_json::Value>(&filtered_json) {
        Ok(parsed_json) => {
            // Extract suggested questions
            let questions = extract_suggested_questions(&filtered_json);

            // Remove the suggested_questions field to get just the steps JSON
            let mut json_obj = parsed_json.clone();
            if let Some(obj) = json_obj.as_object_mut() {
                obj.remove("suggested_questions");
            }

            // Convert back to string for storage
            let steps_only_json = serde_json::to_string(&json_obj)
                .unwrap_or_else(|_| filtered_json.clone());

            Ok((steps_only_json, questions))
        },
        Err(e) => {
            error!("LLM returned invalid JSON: {}", e);
            // Extract just the JSON part if the LLM added any explanatory text
            if let Some(start) = filtered_json.find('{') {
                if let Some(end) = filtered_json.rfind('}') {
                    let json_part = &filtered_json[start..=end];
                    match serde_json::from_str::<serde_json::Value>(json_part) {
                        Ok(parsed_json) => {
                            // Extract suggested questions
                            let questions = extract_suggested_questions(json_part);

                            // Remove the suggested_questions field to get just the steps JSON
                            let mut json_obj = parsed_json.clone();
                            if let Some(obj) = json_obj.as_object_mut() {
                                obj.remove("suggested_questions");
                            }

                            // Convert back to string for storage
                            let steps_only_json = serde_json::to_string(&json_obj)
                                .unwrap_or_else(|_| json_part.to_string());

                            return Ok((steps_only_json, questions));
                        },
                        Err(_) => {}
                    }
                }
            }
            Err(format!("LLM returned invalid JSON: {}", e))
        }
    }
}

/// Extract suggested questions based on the filtered JSON
fn extract_suggested_questions(json: &str) -> Vec<String> {
    // Try to parse the JSON to extract suggested_questions
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => {
            // Look for suggested_questions array in the JSON
            if let Some(questions) = value.get("suggested_questions") {
                if let Some(questions_array) = questions.as_array() {
                    // Convert each question to a string
                    questions_array.iter()
                        .filter_map(|q| q.as_str().map(|s| s.to_string()))
                        .collect()
                } else {
                    vec![]
                }
            } else {
                // No questions field found
                vec![]
            }
        },
        Err(_) => {
            // Failed to parse JSON, return empty vector
            vec![]
        }
    }
} 