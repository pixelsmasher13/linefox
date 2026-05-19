use log::{info, error, warn, debug};
use std::time::Duration;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Emitter};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::configuration::state::ServiceAccess;
use crate::repository::settings_repository::get_setting;
use crate::entity::automation::Automation;
use crate::repository::automation_repository;
use crate::engine::automation_agent_prompt;
use crate::engine::automation_agent_synthetic_prompt;
use crate::engine::automation_completion_prompt;
use crate::engine::data_extraction_prompt;
use crate::repository::task_extracted_data_repository;
use crate::entity::task_extracted_data::DataExtractionResponse;

// Import all types from the new types module
use crate::engine::types::{
    ExecutionState, ActionType, AgentAction, AppState, 
    EnhancedAction, ScreenState, LLMSession, RefreshMetrics
};

// Import wait functionality
use crate::engine::wait::{
    WaitCondition, WaitConfig, wait_for_conditions,
    smart_wait_after_action
};

// Import action parsing
use crate::engine::action_parsing::parse_agent_action;


// ===== GLOBAL STATE =====

// Global state for tracking automation execution
lazy_static::lazy_static! {
    static ref AUTOMATION_STATE: Arc<Mutex<Option<ExecutionState>>> = Arc::new(Mutex::new(None));
    static ref APP_STATE: Arc<Mutex<Option<AppState>>> = Arc::new(Mutex::new(None));
    static ref SHOULD_STOP_EXECUTION: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    // ELEMENT_CACHE defined separately for platform-specific compilation
    static ref LAST_EMITTED_PROGRESS: Arc<Mutex<Option<f32>>> = Arc::new(Mutex::new(None));
    static ref LAST_EMITTED_STATUS: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    pub static ref LLM_SESSION: Arc<Mutex<Option<LLMSession>>> = Arc::new(Mutex::new(None));
    // AUTOMATION_LOG_FILE removed - file logging disabled to prevent reverse engineering
    static ref TAKEOVER_COMPLETE: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    static ref CLARIFICATION_RESPONSE: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    static ref CURRENT_USAGE_SESSION_ID: Arc<Mutex<Option<i64>>> = Arc::new(Mutex::new(None));
    static ref SESSION_START_TIME: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    static ref CURRENT_EXECUTION_RUN_ID: Arc<Mutex<Option<i64>>> = Arc::new(Mutex::new(None));
    static ref CURRENT_STEP_ID: Arc<Mutex<Option<i64>>> = Arc::new(Mutex::new(None));
    // For continuations, this stores the continuation prompt to use for completion message
    // If None, the original automation objective is used
    static ref CURRENT_OBJECTIVE_OVERRIDE: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Track paused time for usage calculation
    static ref USAGE_PAUSED: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    static ref PAUSE_START_TIME: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    static ref TOTAL_PAUSED_DURATION: Arc<Mutex<Duration>> = Arc::new(Mutex::new(Duration::from_secs(0)));
    
    // Metrics for tracking refresh effectiveness
    static ref REFRESH_METRICS: Arc<Mutex<RefreshMetrics>> = Arc::new(Mutex::new(RefreshMetrics::default()));
        
    // Simple in-run memory store that the LLM can write to with MEMORY_SAVE
    static ref MEMORY_STORE: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    // User message injected mid-execution (user sends a correction/hint while agent is running)
    static ref PENDING_USER_MESSAGE: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Accumulated user messages sent during execution (survives sliding window)
    static ref USER_MESSAGES: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Per-run dedupe ledger for FETCH_PAGES.
    // (run_id, attempted_urls, succeeded_urls) — reset at run start. Prevents
    // the agent from re-fetching the same URL repeatedly, which is the primary
    // failure mode of FETCH_PAGES loops.
    static ref FETCHED_URLS: Arc<Mutex<Option<(i64, std::collections::HashSet<String>, std::collections::HashSet<String>)>>> =
        Arc::new(Mutex::new(None));
}

// Platform-specific element cache
#[cfg(target_os = "macos")]
lazy_static::lazy_static! {
    static ref ELEMENT_CACHE: Arc<Mutex<Vec<crate::window_details_collector::macos::macos_action_detector_engine::ActionableElement>>> =
        Arc::new(Mutex::new(Vec::new()));
    // Mapping from display numbers (what the LLM sees) to cache indices
    static ref DISPLAY_TO_CACHE_MAP: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
}

#[cfg(target_os = "windows")]
lazy_static::lazy_static! {
    static ref ELEMENT_CACHE: Arc<Mutex<Vec<serde_json::Value>>> = 
        Arc::new(Mutex::new(Vec::new()));
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
lazy_static::lazy_static! {
    static ref ELEMENT_CACHE: Arc<Mutex<Vec<serde_json::Value>>> = 
        Arc::new(Mutex::new(Vec::new()));
}

/// Safely truncate a string to at most `max_bytes` bytes without splitting
/// multi-byte UTF-8 characters. Returns a slice ending at the nearest char boundary.
fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Clean terminal output for memory storage by stripping ANSI escape codes
/// and progress lines (e.g. "Receiving objects: 45%") that are useful for
/// the UI but waste memory budget for the agent.
fn clean_terminal_output_for_memory(output: &str) -> String {
    let mut cleaned = String::with_capacity(output.len());
    
    for line in output.lines() {
        // Strip ANSI escape sequences (e.g. \x1b[K, \x1b[1m, etc.)
        let stripped = strip_ansi_escapes(line);
        let trimmed = stripped.trim();
        
        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }
        
        // Skip progress-style lines (in-place updates like "Receiving objects: 45%")
        // These are the repetitive percentage lines from git, cargo, pip, etc.
        if is_progress_line(trimmed) {
            continue;
        }
        
        // Keep this line
        if !cleaned.is_empty() {
            cleaned.push('\n');
        }
        cleaned.push_str(trimmed);
    }
    
    cleaned
}

/// Strip ANSI escape sequences from a string
fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC sequence - skip until we find the terminating letter
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next(); // consume '['
                    // CSI sequence: skip until we find a letter (@ through ~)
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch.is_ascii_alphabetic() || ch == 'K' || ch == 'J' || ch == 'H' {
                            break;
                        }
                    }
                    continue;
                }
            }
        } else if c == '\r' {
            // Carriage return - skip (used for in-place progress)
            continue;
        } else if c == '^' {
            // Skip ^D and similar control character representations
            if let Some(&next) = chars.peek() {
                if next.is_ascii_uppercase() {
                    chars.next();
                    continue;
                }
            }
            result.push(c);
        } else {
            result.push(c);
        }
    }
    
    result
}

/// Check if a line is a progress indicator (e.g. "Receiving objects: 45% (164/364)")
fn is_progress_line(line: &str) -> bool {
    // Common patterns: "Something: NN% (n/m)" or "Something: NN%"
    // Match lines that contain a percentage pattern and are repetitive progress
    let progress_patterns = [
        "Receiving objects:",
        "Resolving deltas:",
        "Counting objects:",
        "Compressing objects:",
        "remote: Counting objects:",
        "remote: Compressing objects:",
        "Downloading",
        "Unpacking",
        "Installing",
    ];
    
    for pattern in &progress_patterns {
        if line.starts_with(pattern) || line.contains(pattern) {
            // Only skip if it's a partial progress update (not the final "done" line)
            if line.ends_with("done.") || line.ends_with("done.K") {
                // Keep the final "done" summary lines
                return false;
            }
            // Skip intermediate percentage lines
            if line.contains('%') {
                return true;
            }
        }
    }
    
    false
}

// Configuration constants for timing and behavior
const AUTO_ENTER_ENABLED: bool = false; // Set to false to disable auto-enter after typing
#[allow(dead_code)]
const DETAILED_ELEMENT_LOG_COUNT: usize = 20; // How many elements to show in detail in logs

// Element count thresholds for detecting incomplete accessibility trees (macOS only)
const MIN_ELEMENTS_WEB_BROWSER: usize = 25; // Minimum expected elements for a loaded web page (macOS)
const MIN_ELEMENTS_DESKTOP_APP: usize = 10;  // Minimum expected elements for a desktop app (macOS)
const APP_SWITCH_DELAY_MS: u64 = 500;       // Delay after app switch to let accessibility tree stabilize

// Step counter for execution tracking
static STEP_NUMBER: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

// Replan tracking
static REPLAN_COUNT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
const MAX_REPLANS: i32 = 2; // Maximum number of replans to prevent infinite loops

// ===== HELPER FUNCTIONS =====

/// Finalize execution run when automation completes/stops
pub fn finalize_execution_run(app_handle: &AppHandle, status: &str, error_message: Option<String>) {
    if let Some(run_id) = *CURRENT_EXECUTION_RUN_ID.lock().unwrap() {
        // Get the clipboard contents
        let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

        if let Err(e) = app_handle.db(|db| {
            crate::repository::automation_execution_repository::update_execution_run_status_with_clipboard(
                db, run_id, status, error_message, if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
            )
        }) {
            error!("Failed to update execution run status: {}", e);
        }
    }
    
    // Clear the execution run ID
    *CURRENT_EXECUTION_RUN_ID.lock().unwrap() = None;
    *CURRENT_STEP_ID.lock().unwrap() = None;
    *FETCHED_URLS.lock().unwrap() = None;
}

/// Extract structured data from task memory and store for future reference
async fn extract_and_store_task_data(
    app_handle: &AppHandle,
    automation_id: i64,
    execution_run_id: i64,
    objective: &str,
    memory: &str,
    recent_actions: &str,
    api_choice: &str,
    api_key: &str,
) -> Result<Vec<i64>, String> {
    // Skip extraction if memory is empty or too short
    if memory.is_empty() || memory.len() < 20 {
        info!("Skipping data extraction - memory is empty or too short");
        return Ok(Vec::new());
    }

    // Create extraction prompt
    let extraction_prompt = data_extraction_prompt::get_data_extraction_prompt(
        objective,
        memory,
        recent_actions,
    );

    info!("Extracting structured data from task memory...");
    log_and_file("INFO", "Extracting structured data from memory for storage");

    // Make LLM call for extraction
    let extraction_result = match api_choice {
        "openai" => {
            crate::engine::llm_providers::openai::call_llm_api(
                api_key,
                extraction_prompt.clone(),
                "",
                2000,
            ).await
        },
        "openai-codex" => {
            crate::engine::llm_providers::openai_codex::call_llm_api(
                api_key,
                extraction_prompt.clone(),
                "",
                2000,
            ).await
        },
        "grok" => {
            crate::engine::llm_providers::grok::call_llm_api(
                api_key,
                extraction_prompt.clone(),
                "",
                2000,
            ).await
        },
        "deepseek" => {
            crate::engine::llm_providers::deepseek::call_llm_api(
                api_key,
                extraction_prompt.clone(),
                "",
                2000,
            ).await
        },
        "proxy" => {
            crate::engine::llm_providers::proxy::call_llm_api(
                api_key,
                extraction_prompt.clone(),
                "",
                2000,
            ).await
        },
        "gemini" => {
            crate::engine::llm_providers::gemini::call_llm_api(
                api_key,
                extraction_prompt.clone(),
                "",
                2000,
            ).await
        },
        "claude" | "claude-subscription" | _ => {
            crate::engine::llm_providers::claude::call_llm_api(
                api_key,
                extraction_prompt.clone(),
                "",
                2000,
            ).await
        }
    };

    match extraction_result {
        Ok((response, input_tokens, output_tokens)) => {
            log_token_usage(input_tokens, output_tokens, "DATA_EXTRACTION");
            
            // Parse the JSON response - handle markdown code blocks
            let json_str = response.trim();
            let json_str = if json_str.starts_with("```json") {
                json_str.trim_start_matches("```json").trim_end_matches("```").trim()
            } else if json_str.starts_with("```") {
                json_str.trim_start_matches("```").trim_end_matches("```").trim()
            } else {
                json_str
            };
            
            match serde_json::from_str::<DataExtractionResponse>(json_str) {
                Ok(extraction) => {
                    info!("Data extraction response: has_useful_data={}, records={}", 
                          extraction.has_useful_data, extraction.records.len());
                    
                    if extraction.has_useful_data && !extraction.records.is_empty() {
                        // Store the records in the database
                        match app_handle.db(|db| {
                            task_extracted_data_repository::insert_extracted_data_batch(
                                db,
                                execution_run_id,
                                automation_id,
                                &extraction.records,
                            )
                        }) {
                            Ok(ids) => {
                                info!("Stored {} extracted data records for execution run {}", 
                                      ids.len(), execution_run_id);
                                log_and_file("INFO", &format!(
                                    "Stored {} extracted data records: {:?}",
                                    ids.len(),
                                    extraction.records.iter().map(|r| &r.record_name).collect::<Vec<_>>()
                                ));
                                Ok(ids)
                            },
                            Err(e) => {
                                warn!("Failed to store extracted data: {}", e);
                                Err(format!("Database error: {}", e))
                            }
                        }
                    } else {
                        info!("No useful data to extract: {}", extraction.reasoning);
                        Ok(Vec::new())
                    }
                },
                Err(e) => {
                    warn!("Failed to parse data extraction response: {}. Response was: {}", e, json_str);
                    Err(format!("JSON parse error: {}", e))
                }
            }
        },
        Err(e) => {
            warn!("Data extraction LLM call failed: {}", e);
            Err(format!("LLM error: {}", e))
        }
    }
}

/// Trigger a replan when the executor is stuck (LLM issued STUCK command)
/// This regenerates the nl_description plan and resets the LLM session
async fn trigger_replan(
    app_handle: &AppHandle,
    automation: &Automation,
    current_step: i32,
    recent_actions: &[String],
    stuck_reasoning: &str,
) -> Result<String, String> {
    use std::sync::atomic::Ordering;
    
    let replan_num = REPLAN_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    info!("🔄 REPLAN #{}: LLM signaled STUCK at step {}. Reason: {}", 
          replan_num, current_step, stuck_reasoning);
    
    // Check if we've exceeded max replans
    if replan_num > MAX_REPLANS {
        warn!("Maximum replan count ({}) exceeded, continuing without replan", MAX_REPLANS);
        return Err("Maximum replan count exceeded".to_string());
    }
    
    // Build detailed action history - this is critical for the planner to understand what was tried
    let action_history = if recent_actions.is_empty() {
        "No actions were recorded".to_string()
    } else {
        recent_actions.iter()
            .enumerate()
            .map(|(i, action)| format!("{}. {}", i + 1, action))
            .collect::<Vec<_>>()
            .join("\n")
    };
    
    // Get API settings
    let api_choice = app_handle
        .db(|db| get_setting(db, "api_choice").map(|s| s.setting_value))
        .unwrap_or_else(|_| "proxy".to_string());
    
    let api_key = match api_choice.as_str() {
        "openai" => app_handle
            .db(|db| get_setting(db, "api_key_open_ai").map(|s| s.setting_value))
            .unwrap_or_default(),
        "proxy" => {
            // Get JWT token for proxy authentication
            let user_id = app_handle
                .db(|db| match get_setting(db, "user_id") {
                    Ok(setting) => Some(setting.setting_value),
                    Err(_) => None,
                })
                .unwrap_or_default();
            
            // Get the auth token for the user
            app_handle
                .db(|db| crate::repository::user_auth_repository::get_valid_auth_token(db, &user_id))
                .ok()
                .flatten()
                .unwrap_or_default()
        },
        _ => app_handle
            .db(|db| get_setting(db, "api_key_open_ai").map(|s| s.setting_value))
            .unwrap_or_default(),
    };
    
    // Create replan prompt - THE KEY IS GIVING IT THE HISTORY so it can devise a different approach
    let replan_prompt = format!(
        "REPLAN NEEDED - The executor got stuck and needs a NEW approach.\n\n\
        OBJECTIVE: {}\n\n\
        WHAT WAS TRIED (execution history - {} steps taken):\n{}\n\n\
        WHY IT GOT STUCK: {}\n\n\
        PREVIOUS PLAN THAT DIDN'T WORK:\n{}\n\n\
        YOUR TASK: Create a COMPLETELY DIFFERENT plan to achieve the objective.\n\
        - DO NOT repeat what was already tried\n\
        - Consider alternative apps, different websites, or simpler approaches\n\
        - Learn from what failed and avoid similar patterns\n\
        - If something was clicked multiple times without progress, that element probably doesn't work\n\
        - If a site/app seems unresponsive, try a different one",
        automation.objective,
        current_step,
        action_history,
        stuck_reasoning,
        automation.nl_description.as_deref().unwrap_or("(no previous plan)")
    );
    
    info!("Calling planner to regenerate plan...");
    
    // Use ScriptGenerator to create new plan
    let generator = crate::engine::script_generator::ScriptGenerator::new(api_key, api_choice);
    
    // Get installed apps for context
    let installed_apps: Vec<String> = Vec::new(); // Could be populated if needed
    
    match generator.generate_script_from_prompt(&replan_prompt, &automation.name, &installed_apps).await {
        Ok((_script, new_nl_description, _name)) => {
            info!("✅ Replan successful, got new plan with {} steps", 
                  new_nl_description.lines().count());
            
            // Update automation with new nl_description
            if let Err(e) = app_handle.db_mut(|db| {
                db.execute(
                    "UPDATE automations SET nl_description = ?, updated_at = datetime('now') WHERE id = ?",
                    [&new_nl_description, &automation.id.to_string()]
                )
                .map(|_| ())
                .map_err(|e| format!("Failed to update automation: {}", e))
            }) {
                error!("Failed to update automation with new plan: {}", e);
                return Err(e);
            }
            
            
            // Emit event to notify UI about replan
            let _ = app_handle.emit("automation_replanned", serde_json::json!({
                "automationId": automation.id,
                "replanNumber": replan_num,
                "newPlan": new_nl_description,
            }));
            
            Ok(new_nl_description)
        },
        Err(e) => {
            error!("❌ Replan failed: {}", e);
            Err(format!("Replan failed: {}", e))
        }
    }
}

/// Reset replan counter for new automation
fn reset_replan_counter() {
    use std::sync::atomic::Ordering;
    REPLAN_COUNT.store(0, Ordering::SeqCst);
}

// ===== PUBLIC API =====

/// Execute an automation using the generalized script and additional instructions
pub async fn execute_automation(
    app_handle: &AppHandle,
    automation_id: i64,
    additional_instructions: Option<String>
) -> Result<(), String> {
    // Reset stop flag
    *SHOULD_STOP_EXECUTION.lock().unwrap() = false;
    
    // Reset replan counter for new execution
    reset_replan_counter();
    
    // Clear memory store, objective override, pending user message, and terminal state for new automation
    MEMORY_STORE.lock().unwrap().clear();
    // Also initialize the structured memory_manager for task mode. Without
    // this, agent-mode runs leave the manager populated from a prior run and
    // the next task-mode run sees stale CURRENT MEMORY in its prompts.
    memory_manager::init_memory(false);
    *CURRENT_OBJECTIVE_OVERRIDE.lock().unwrap() = None;
    *PENDING_USER_MESSAGE.lock().unwrap() = None;
    USER_MESSAGES.lock().unwrap().clear();
    crate::engine::terminal::executor::reset_claude_cli_session();
    crate::engine::terminal::executor::reset_terminal_state();
    info!("Cleared MEMORY_STORE, CURRENT_OBJECTIVE_OVERRIDE, Claude CLI session, and terminal CWD for new automation execution");
    
    // Get the automation
    let automation = app_handle
        .db(|db| automation_repository::get_automation_by_id(db, automation_id))
        .map_err(|e| format!("Failed to get automation: {}", e))?
        .ok_or_else(|| format!("Automation with ID {} not found", automation_id))?;
    
    info!("Starting execution of automation: {} (ID: {})", automation.name, automation_id);
    
    // NOTE: automation_started event is emitted AFTER execution_run_id is created (see below)
    
    // Get user ID from settings or use default
    let user_id = app_handle
        .db(|db| match get_setting(db, "user_id") {
            Ok(setting) => Some(setting.setting_value),
            Err(_) => None,
        })
        .unwrap_or_else(|| "default_user".to_string());

    // Get use_pro_model setting for billing multiplier (1.5x for pro model)
    let use_pro_model_for_billing = app_handle
        .db(|db| match get_setting(db, "use_pro_model") {
            Ok(setting) => setting.setting_value == "true",
            Err(_) => false,
        });

    // Create usage session
    let session_id = app_handle
        .db(|db| {
            crate::repository::usage_session_repository::create_usage_session(
                db,
                crate::entity::usage_session::NewUsageSession {
                    user_id: user_id.clone(),
                    automation_id,
                }
            )
        })
        .map_err(|e| format!("Failed to create usage session: {}", e))?;
    
    // Store session ID and start time, reset pause tracking
    *CURRENT_USAGE_SESSION_ID.lock().unwrap() = Some(session_id);
    *SESSION_START_TIME.lock().unwrap() = Some(Instant::now());
    *USAGE_PAUSED.lock().unwrap() = false;
    *PAUSE_START_TIME.lock().unwrap() = None;
    *TOTAL_PAUSED_DURATION.lock().unwrap() = Duration::from_secs(0);

    info!("Created usage session {} for user {}", session_id, user_id);
    
    // Spawn background task to update usage periodically
    let app_handle_clone = app_handle.clone();
    // Capture billing multiplier setting for the async block (1.5x for pro model)
    let billing_multiplier_percent = if use_pro_model_for_billing { 150 } else { 100 };
    let session_update_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10)); // Update every 10 seconds
        loop {
            interval.tick().await;

            // Check if session is still active
            let session_id = match *CURRENT_USAGE_SESSION_ID.lock().unwrap() {
                Some(id) => id,
                None => break, // Session ended
            };

            // Calculate elapsed time excluding paused time
            let elapsed = match *SESSION_START_TIME.lock().unwrap() {
                Some(start) => {
                    let total_elapsed = start.elapsed();
                    let total_paused = *TOTAL_PAUSED_DURATION.lock().unwrap();

                    // Add current pause duration if currently paused
                    let current_pause = if *USAGE_PAUSED.lock().unwrap() {
                        PAUSE_START_TIME.lock().unwrap()
                            .map(|pause_start| pause_start.elapsed())
                            .unwrap_or(Duration::from_secs(0))
                    } else {
                        Duration::from_secs(0)
                    };

                    // Subtract all paused time from total elapsed
                    total_elapsed.saturating_sub(total_paused + current_pause).as_secs() as i32
                }
                None => break,
            };

            // Update session in database
            if let Err(e) = app_handle_clone.db(|db| {
                crate::repository::usage_session_repository::update_usage_session_progress(
                    db, session_id, elapsed
                )
            }) {
                error!("Failed to update usage session progress: {}", e);
            }

            // Calculate billed minutes with multiplier for pro model (1.5x)
            let base_minutes = (elapsed + 59) / 60;
            let billed_minutes = (base_minutes * billing_multiplier_percent + 99) / 100; // Round up

            // Emit progress event to UI
            let _ = app_handle_clone.emit("usage_update", serde_json::json!({
                "sessionId": session_id,
                "elapsedSeconds": elapsed,
                "billedMinutes": billed_minutes,
            }));
        }
    });
    
    // Clean up any other running executions before starting a new one
    app_handle
        .db(|db| {
            crate::repository::automation_execution_repository::cleanup_stale_running_executions(db)
        })
        .map_err(|e| format!("Failed to clean up stale executions: {}", e))?;
    
    // Create execution run in database
    let execution_run_id = app_handle
        .db(|db| {
            crate::repository::automation_execution_repository::create_execution_run(
                db,
                automation_id,
                additional_instructions.clone(),
            )
        })
        .map_err(|e| format!("Failed to create execution run: {}", e))?;
    
    info!("Created execution run {} for automation {}", execution_run_id, automation_id);
    
    // Store execution run ID in global state for use in the loop
    *CURRENT_EXECUTION_RUN_ID.lock().unwrap() = Some(execution_run_id);
    
    // Emit automation_started event with execution_run_id so UI can track this run
    let _ = app_handle.emit("automation_started", serde_json::json!({
        "automationId": automation_id,
        "automationName": automation.name.clone(),
        "execution_run_id": execution_run_id,
    }));

    // Emit playback_status so the UI shows the execution view + stop button
    // regardless of how the task was triggered (UI, Telegram, scheduled, etc.)
    let _ = app_handle.emit("playback_status", serde_json::json!({
        "isPlaying": true,
        "progress": 0,
        "statusMessage": format!("Starting automation: {}", automation.name),
    }));
    
    // Reset step counter for new execution
    use std::sync::atomic::Ordering;
    STEP_NUMBER.store(0, Ordering::SeqCst);
    
    // Initialize automation log file
    init_automation_log_file(app_handle, automation_id, &automation.name)?;
    log_and_file("INFO", &format!("Starting execution of automation: {} (ID: {})", automation.name, automation_id));
    
    // Create a minimal execution state directly
    let current_time = chrono::Utc::now().to_rfc3339();
    let execution_state = ExecutionState {
        automation_id: automation.id,
        name: automation.name.clone(),
        objective: automation.objective.clone(),
        status: "In progress".to_string(),
        started_at: current_time.clone(),
        milestones: Vec::new(),
    };
    
    // Store the execution state in global state
    *AUTOMATION_STATE.lock().unwrap() = Some(execution_state.clone());
    info!("Execution state stored in global state for automation: {} (ID: {})", automation.name, automation_id);
    
    // Initialize app state
    *APP_STATE.lock().unwrap() = Some(AppState {
        current_app: None,
        current_pid: None,
        accessible_elements: Vec::new(),
        text_content: None,
        full_text_content: None,
        word_document_info: None,
        excel_sheet_info: None,
        recent_actions: Vec::new(),
        element_tree: None,
        content_map: Vec::new(),
        action_history_summary: None,
    });
    
    // Initialize LLM session with initial context
    {
        let session_id = format!("automation_{}_{}", automation_id, current_time);
        
        // Create system prompt with automation context included
        // Use synthetic prompt for automations without a generalized script
        let trimmed_script = automation.generalized_script.trim();

        // Check if script is effectively empty (various formats)
        let is_empty_script = automation.generalized_script.is_empty() ||
                             trimmed_script.is_empty() ||
                             trimmed_script == "[]" ||
                             trimmed_script == "{}" ||
                             trimmed_script == "{ }" ||
                             trimmed_script == "[ ]" ||
                             trimmed_script == "null" ||
                             // Check if it's just an empty JSON object/array with possible whitespace
                             (trimmed_script.starts_with('{') && trimmed_script.ends_with('}') && trimmed_script.len() <= 10) ||
                             (trimmed_script.starts_with('[') && trimmed_script.ends_with(']') && trimmed_script.len() <= 10) ||
                             // Check for script with only empty SCRIPT action
                             trimmed_script.contains(r#""script":"{}"#) ||
                             trimmed_script.contains(r#""script":"[]"#) ||
                             trimmed_script.contains(r#""script":""#);

        info!("Generalized script check - Original: '{}', Trimmed: '{}', Is empty: {}",
              automation.generalized_script, trimmed_script, is_empty_script);

        let system_prompt = if is_empty_script {
            info!("Using synthetic automation prompt (no generalized script)");
            automation_agent_synthetic_prompt::get_synthetic_system_prompt_with_context(
                Some(&automation.objective),
                automation.nl_description.as_deref(),
                Some(&automation.name),
                additional_instructions.as_deref(),
                None, // App-specific commands will be added dynamically based on active app
            )
        } else {
            info!("Using standard automation prompt with generalized script");
            automation_agent_prompt::get_system_prompt_with_context(
                Some(&automation.objective),
                Some(&automation.generalized_script),
                automation.nl_description.as_deref(),
                Some(&automation.name),
                additional_instructions.as_deref(),
                None, // App-specific commands will be added dynamically based on active app
            )
        };

        // Create minimal initial context - everything important is now in the system prompt
        let initial_context = "Ready to begin automation execution.".to_string();
        
        let session = LLMSession::new(session_id, system_prompt, initial_context);
        *LLM_SESSION.lock().unwrap() = Some(session);
        
        info!("Initialized LLM session for automation {}", automation_id);
    }
    
    // Notify UI that execution has started
    update_execution_progress(app_handle, 0.0, Some("Starting automation execution...".to_string()))?;
    
    // Execute the automation in a reactive loop
    let execution_result = execute_reactive_loop(app_handle, &execution_state).await;

    // Complete or cancel the usage session based on result
    if let Some(session_id) = *CURRENT_USAGE_SESSION_ID.lock().unwrap() {
        match &execution_result {
            Ok(_) => {
                // Complete the session
                if let Err(e) = app_handle.db(|db| {
                    crate::repository::usage_session_repository::complete_usage_session(db, session_id)
                }) {
                    error!("Failed to complete usage session: {}", e);
                }
            },
            Err(_) => {
                // Cancel the session
                if let Err(e) = app_handle.db(|db| {
                    crate::repository::usage_session_repository::cancel_usage_session(db, session_id)
                }) {
                    error!("Failed to cancel usage session: {}", e);
                }
            }
        }
    }

    // Clear session tracking
    *CURRENT_USAGE_SESSION_ID.lock().unwrap() = None;
    *SESSION_START_TIME.lock().unwrap() = None;

    // Cancel the update task
    session_update_task.abort();

    // Finalize execution run and return the original result
    if let Err(e) = execution_result {
        finalize_execution_run(app_handle, "failed", Some(e.clone()));
        return Err(e);
    }

    execution_result?;
    
    // Log refresh metrics (macOS only - Windows doesn't do retries)
    #[cfg(target_os = "macos")]
    {
        let metrics = REFRESH_METRICS.lock().unwrap();
        info!("🔄 Element Refresh Metrics for automation {} (macOS):", automation_id);
        info!("  - Event-driven refreshes: {} (triggered by UI structure changes)", metrics.event_driven_refreshes);
        info!("  - Low-count refreshes: {} (triggered by element count heuristic)", metrics.low_count_refreshes);
        info!("  - Successful refreshes: {} (element count improved)", metrics.successful_refreshes);
        info!("  - Failed refreshes: {} (element count unchanged)", metrics.failed_refreshes);
        
        let total_refreshes = metrics.event_driven_refreshes + metrics.low_count_refreshes;
        if total_refreshes > 0 {
            let event_driven_percentage = (metrics.event_driven_refreshes as f32 / total_refreshes as f32 * 100.0) as u32;
            let success_rate = (metrics.successful_refreshes as f32 / total_refreshes as f32 * 100.0) as u32;
            info!("  - Event-driven: {}% of refreshes", event_driven_percentage);
            info!("  - Success rate: {}%", success_rate);
            
            log_and_file("INFO", &format!(
                "Refresh metrics: {} event-driven, {} low-count, {}% success rate",
                metrics.event_driven_refreshes, metrics.low_count_refreshes, success_rate
            ));
        }
    }
    
    // Reset metrics for next run
    {
        let mut metrics = REFRESH_METRICS.lock().unwrap();
        *metrics = RefreshMetrics::default();
    }
    
    // Log session statistics before cleanup
    if let Some(session) = LLM_SESSION.lock().unwrap().as_ref() {
        // Log detailed token summary to automation log
        log_session_token_summary(session, automation_id);
        
        let api_calls = session.messages.len() / 2; // Divide by 2 since we have user/assistant pairs
        
        info!(
            "LLM Session Statistics for automation {}:\n\
            - Total input tokens: {}\n\
            - Total output tokens: {}\n\
            - Total API calls: {}\n\
            - Average tokens per call: {:.0}",
            automation_id,
            session.total_input_tokens,
            session.total_output_tokens,
            api_calls,
            if api_calls > 0 { session.total_input_tokens as f32 / api_calls as f32 } else { 0.0 }
        );
    }
    
    // Clean up
    *AUTOMATION_STATE.lock().unwrap() = None;
    *APP_STATE.lock().unwrap() = None;
    *LLM_SESSION.lock().unwrap() = None;
    
    // Get the accumulated memory/clipboard contents
    let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

    // Emit automation_stopped event with clipboard data
    let _ = app_handle.emit("automation_stopped", serde_json::json!({
        "clipboard": if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
    }));
    
    // Log session statistics
    info!("Automation {} completed. Session cleaned up.", automation_id);
    
    // Close the automation log file
    close_automation_log_file();
    
    Ok(())
}

/// Continue an automation execution within an existing run
/// This is used for chat-style continuation where the user wants to follow up
pub async fn execute_automation_continuation(
    app_handle: &AppHandle,
    automation_id: i64,
    execution_run_id: i64,
    continuation_plan: String,
    continuation_name: String,
    synthesized_objective: String,
    continuation_prompt: String, // Raw user prompt, used for completion message
) -> Result<(), String> {
    // Reset stop flag
    *SHOULD_STOP_EXECUTION.lock().unwrap() = false;
    
    // Reset replan counter for continuation
    reset_replan_counter();
    
    // Load memory from the database for this specific execution run.
    // The global MEMORY_STORE may contain data from a completely different task
    // that ran after this one, so we must restore from the persisted clipboard.
    {
        let saved_clipboard = app_handle
            .db(|db| crate::repository::automation_execution_repository::get_execution_run_by_id(db, execution_run_id))
            .ok()
            .flatten()
            .and_then(|run| run.clipboard);

        let mut mem = MEMORY_STORE.lock().unwrap();
        mem.clear();
        if let Some(clipboard) = saved_clipboard {
            *mem = clipboard;
            info!("Restored MEMORY_STORE from database ({} chars) for run {}", mem.len(), execution_run_id);
        } else {
            info!("No saved memory found in database for run {} - starting with empty memory", execution_run_id);
        }
    }

    // Set the objective override so completion message uses the user's continuation prompt (not the synthesized objective)
    *CURRENT_OBJECTIVE_OVERRIDE.lock().unwrap() = Some(continuation_prompt.clone());
    info!("Continuing automation {} (run {}) - restored MEMORY_STORE from DB, objective override: {}",
          automation_id, execution_run_id, &continuation_prompt);
    
    // Get the automation
    let automation = app_handle
        .db(|db| automation_repository::get_automation_by_id(db, automation_id))
        .map_err(|e| format!("Failed to get automation: {}", e))?
        .ok_or_else(|| format!("Automation with ID {} not found", automation_id))?;
    
    info!("Continuing execution of automation: {} (ID: {}, Run: {})", automation.name, automation_id, execution_run_id);
    
    // Emit automation_started event with execution_run_id
    let _ = app_handle.emit("automation_started", serde_json::json!({
        "automationId": automation_id,
        "automationName": automation.name.clone(),
        "execution_run_id": execution_run_id,
        "isContinuation": true,
    }));
    
    // Get user ID from settings or use default
    let user_id = app_handle
        .db(|db| match get_setting(db, "user_id") {
            Ok(setting) => Some(setting.setting_value),
            Err(_) => None,
        })
        .unwrap_or_else(|| "default_user".to_string());

    // Get use_pro_model setting for billing multiplier (1.5x for pro model)
    let use_pro_model_for_billing = app_handle
        .db(|db| match get_setting(db, "use_pro_model") {
            Ok(setting) => setting.setting_value == "true",
            Err(_) => false,
        });

    // Create usage session for continuation
    let session_id = app_handle
        .db(|db| {
            crate::repository::usage_session_repository::create_usage_session(
                db,
                crate::entity::usage_session::NewUsageSession {
                    user_id: user_id.clone(),
                    automation_id,
                }
            )
        })
        .map_err(|e| format!("Failed to create usage session: {}", e))?;
    
    // Store session ID and start time, reset pause tracking
    *CURRENT_USAGE_SESSION_ID.lock().unwrap() = Some(session_id);
    *SESSION_START_TIME.lock().unwrap() = Some(Instant::now());
    *USAGE_PAUSED.lock().unwrap() = false;
    *PAUSE_START_TIME.lock().unwrap() = None;
    *TOTAL_PAUSED_DURATION.lock().unwrap() = Duration::from_secs(0);

    info!("Created usage session {} for continuation by user {}", session_id, user_id);
    
    // Spawn background task to update usage periodically
    let app_handle_clone = app_handle.clone();
    let billing_multiplier_percent = if use_pro_model_for_billing { 150 } else { 100 };
    let session_update_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            let session_id = match *CURRENT_USAGE_SESSION_ID.lock().unwrap() {
                Some(id) => id,
                None => break,
            };
            let elapsed = match *SESSION_START_TIME.lock().unwrap() {
                Some(start) => {
                    let total_elapsed = start.elapsed();
                    let total_paused = *TOTAL_PAUSED_DURATION.lock().unwrap();
                    let current_pause = if *USAGE_PAUSED.lock().unwrap() {
                        PAUSE_START_TIME.lock().unwrap()
                            .map(|pause_start| pause_start.elapsed())
                            .unwrap_or(Duration::from_secs(0))
                    } else {
                        Duration::from_secs(0)
                    };
                    total_elapsed.saturating_sub(total_paused + current_pause).as_secs() as i32
                }
                None => break,
            };
            if let Err(e) = app_handle_clone.db(|db| {
                crate::repository::usage_session_repository::update_usage_session_progress(
                    db, session_id, elapsed
                )
            }) {
                error!("Failed to update usage session progress: {}", e);
            }
            let base_minutes = (elapsed + 59) / 60;
            let billed_minutes = (base_minutes * billing_multiplier_percent + 99) / 100;
            let _ = app_handle_clone.emit("usage_update", serde_json::json!({
                "sessionId": session_id,
                "elapsedSeconds": elapsed,
                "billedMinutes": billed_minutes,
            }));
        }
    });
    
    // Use the existing execution run ID
    *CURRENT_EXECUTION_RUN_ID.lock().unwrap() = Some(execution_run_id);
    
    // Get current step count to continue numbering
    let current_step_count = app_handle
        .db(|db| crate::repository::automation_execution_repository::get_step_count(db, execution_run_id))
        .map_err(|e| format!("Failed to get step count: {}", e))?;
    
    // Set step counter to continue from where we left off
    use std::sync::atomic::Ordering;
    STEP_NUMBER.store(current_step_count, Ordering::SeqCst);
    info!("Continuing from step {}", current_step_count);
    
    // Initialize automation log file
    init_automation_log_file(app_handle, automation_id, &automation.name)?;
    log_and_file("INFO", &format!("Continuing execution of automation: {} (ID: {}, Run: {})", automation.name, automation_id, execution_run_id));
    
    // Create execution state with continuation name and synthesized objective
    let current_time = chrono::Utc::now().to_rfc3339();
    let execution_state = ExecutionState {
        automation_id: automation.id,
        name: continuation_name.clone(),
        objective: synthesized_objective.clone(),
        status: "Continuing".to_string(),
        started_at: current_time.clone(),
        milestones: Vec::new(),
    };
    
    *AUTOMATION_STATE.lock().unwrap() = Some(execution_state.clone());
    
    // Initialize app state with clean recent_actions (no old-turn history)
    *APP_STATE.lock().unwrap() = Some(AppState {
        current_app: None,
        current_pid: None,
        accessible_elements: Vec::new(),
        text_content: None,
        full_text_content: None,
        word_document_info: None,
        excel_sheet_info: None,
        recent_actions: Vec::new(),
        element_tree: None,
        content_map: Vec::new(),
        action_history_summary: None,
    });
    
    // Initialize LLM session for continuation
    {
        let llm_session_id = format!("continuation_{}_{}", automation_id, current_time);
        
        // Use synthesized objective, continuation plan, and continuation name
        // No continuation context blob needed — the synthesized objective carries prior context
        let system_prompt = automation_agent_synthetic_prompt::get_synthetic_system_prompt_with_context(
            Some(&synthesized_objective),
            Some(&continuation_plan),
            Some(&continuation_name),
            None, // No additional_instructions — context is in the synthesized objective
            None,
        );

        info!("Initializing LLM session for continuation with {} chars prompt", system_prompt.len());
        
        let initial_context = "Continuing from previous task execution.".to_string();
        let session = LLMSession::new(llm_session_id, system_prompt, initial_context);
        
        *LLM_SESSION.lock().unwrap() = Some(session);
    }
    
    // Emit progress event
    let _ = app_handle.emit("execution_progress", serde_json::json!({
        "automationId": automation_id,
        "executionRunId": execution_run_id,
        "progress": 0,
        "statusMessage": "Continuing previous task...",
        "isContinuation": true,
    }));
    
    // Execute the reactive loop (same as normal execution)
    let execution_result = execute_reactive_loop(app_handle, &execution_state).await;

    // Complete or cancel the usage session based on result
    if let Some(usage_session_id) = *CURRENT_USAGE_SESSION_ID.lock().unwrap() {
        match &execution_result {
            Ok(_) => {
                if let Err(e) = app_handle.db(|db| {
                    crate::repository::usage_session_repository::complete_usage_session(db, usage_session_id)
                }) {
                    error!("Failed to complete usage session: {}", e);
                }
            },
            Err(_) => {
                if let Err(e) = app_handle.db(|db| {
                    crate::repository::usage_session_repository::cancel_usage_session(db, usage_session_id)
                }) {
                    error!("Failed to cancel usage session: {}", e);
                }
            }
        }
    }

    // Clear session tracking
    *CURRENT_USAGE_SESSION_ID.lock().unwrap() = None;
    *SESSION_START_TIME.lock().unwrap() = None;

    // Cancel the update task
    session_update_task.abort();

    // Finalize execution run and return the original result
    if let Err(e) = execution_result {
        finalize_execution_run(app_handle, "failed", Some(e.clone()));
        return Err(e);
    }

    execution_result?;
    
    // Clean up
    *AUTOMATION_STATE.lock().unwrap() = None;
    *APP_STATE.lock().unwrap() = None;
    *LLM_SESSION.lock().unwrap() = None;
    
    // Get the accumulated memory/clipboard contents
    let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

    // Emit completion
    let _ = app_handle.emit("automation_completed", serde_json::json!({
        "automationId": automation_id,
        "executionRunId": execution_run_id,
        "success": true,
        "isContinuation": true,
        "clipboard": if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
    }));
    
    info!("Continuation of automation {} (run {}) completed.", automation_id, execution_run_id);
    
    close_automation_log_file();
    
    Ok(())
}

/// Check if an automation is currently running
pub fn is_running() -> bool {
    let not_stopped = !*SHOULD_STOP_EXECUTION.lock().unwrap();
    let has_run_id = CURRENT_EXECUTION_RUN_ID.lock().unwrap().is_some();
    not_stopped && has_run_id
}

/// Stop the execution of the current automation
pub fn stop_execution() -> Result<(), String> {
    info!("Stopping automation execution");
    
    // Set the stop flag
    *SHOULD_STOP_EXECUTION.lock().unwrap() = true;
    
    // Note: The usage session will be cancelled when execute_automation handles the stop
    
    Ok(())
}

/// Complete the user takeover
pub fn complete_takeover() -> Result<(), String> {
    info!("User completed takeover");
    
    // Set the takeover complete flag
    *TAKEOVER_COMPLETE.lock().unwrap() = true;
    
    Ok(())
}

/// Submit clarification response from user
pub fn submit_clarification(response: String) -> Result<(), String> {
    info!("User submitted clarification: {}", response);

    // Store the clarification response
    *CLARIFICATION_RESPONSE.lock().unwrap() = Some(response);

    Ok(())
}

/// Send a message to the agent mid-execution.
/// The message will be injected into the LLM session on the next loop iteration.
pub fn send_user_message(message: String) -> Result<(), String> {
    info!("User sent mid-execution message: {}", message);
    *PENDING_USER_MESSAGE.lock().unwrap() = Some(message);
    Ok(())
}

/// Get the current execution state
#[allow(dead_code)]
pub fn get_execution_state() -> Option<ExecutionState> {
    AUTOMATION_STATE.lock().unwrap().clone()
}

/// Get the current app state (used by action_parsing module)
pub fn get_current_app_state() -> Option<AppState> {
    APP_STATE.lock().unwrap().clone()
}

/// Public wrapper for get_element_by_index (used by action_parsing module)
#[cfg(target_os = "macos")]
pub fn get_element_by_index_public(index: usize) -> Option<crate::window_details_collector::macos::macos_action_detector_engine::ActionableElement> {
    get_element_by_index(index)
}

#[cfg(target_os = "windows")]
pub fn get_element_by_index_public(index: usize) -> Option<serde_json::Value> {
    get_element_by_index(index)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn get_element_by_index_public(_index: usize) -> Option<serde_json::Value> {
    None
}

// ===== CORE FUNCTIONALITY =====


/// Execute the automation in a reactive loop
async fn execute_reactive_loop(
    app_handle: &AppHandle,
    _execution_state: &ExecutionState
) -> Result<(), String> {
    let mut last_app_state: Option<AppState> = None; // Track the last known app state
    
    // Main execution loop
    loop {
        // Get the current state but drop the MutexGuard immediately
        let current_execution_state = {
            let guard = AUTOMATION_STATE.lock().unwrap();
            guard.clone()
        };
        
        // Check if we have a state
        let execution_state = match current_execution_state {
            Some(state) => state,
            None => {
                // If we reach here, the state was removed - consider it completed
                update_execution_progress(app_handle, 100.0, Some("Automation execution completed".to_string()))?;
                
                // Update execution run status to completed
                finalize_execution_run(app_handle, "completed", None);
                
                // Get the accumulated memory/clipboard contents
                let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

                // Emit automation_stopped event with clipboard data
                let _ = app_handle.emit("automation_stopped", serde_json::json!({
                    "clipboard": if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
                }));
                
                return Ok(());
            }
        };
        
        // Check if we should stop execution (get and drop the lock immediately)
        let should_stop = {
            *SHOULD_STOP_EXECUTION.lock().unwrap()
        };
        
        if should_stop {
            info!("Stopping automation execution as requested");
            
            // Use simple fixed progress value
            update_execution_progress(app_handle, 0.0, Some("Execution stopped as requested".to_string()))?;
            
            // Update execution run status to stopped
            finalize_execution_run(app_handle, "stopped", None);
            
            // Get the accumulated memory/clipboard contents
            let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

            // Emit automation_stopped event with clipboard data
            let _ = app_handle.emit("automation_stopped", serde_json::json!({
                "clipboard": if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
            }));
            
            return Ok(());
        }
        
        // Update progress (fixed value)
        update_execution_progress(
            app_handle, 
            0.0, 
            Some(format!("Executing automation: {}", execution_state.name))
        )?;
        
        // Get current app state - use cached if available and fresh
        let app_state = if let Some(ref cached_state) = last_app_state {
            info!("Using cached app state from recent action - {} elements", cached_state.accessible_elements.len());
            cached_state.clone()
        } else {
            info!("Updating app state before decision - no cached state available");
            update_app_state().await?
        };

        // Clear the cached state - it's only valid for one use
        last_app_state = None;

        // Check for pending user message and inject into LLM session + user messages log
        if let Some(user_msg) = PENDING_USER_MESSAGE.lock().unwrap().take() {
            info!("Injecting user message into session: {}", user_msg);
            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
            // Store in persistent user messages array (survives sliding window)
            USER_MESSAGES.lock().unwrap().push(format!("[{}] {}", timestamp, user_msg));
            if let Some(session) = LLM_SESSION.lock().unwrap().as_mut() {
                session.add_user_message(format!(
                    "⚠️ IMPORTANT: The user sent you a message while you were executing:\n\n\
                    \"{}\"\n\n\
                    STOP and re-evaluate your current approach. The user may be correcting you, \
                    giving new instructions, or changing the goal. Adjust your next action accordingly. \
                    Do NOT continue with your previous plan if it conflicts with this message.",
                    user_msg
                ));
            }
        }

        // Decide next action based on current state and task
        let action = decide_next_action(app_handle, &execution_state, &app_state, None).await?;
        
        // Check if we should stop execution AFTER getting the LLM response
        // This prevents executing an action if the user stopped while waiting for LLM
        let should_stop_after_llm = {
            *SHOULD_STOP_EXECUTION.lock().unwrap()
        };
        
        if should_stop_after_llm {
            info!("Stopping automation execution after LLM response - action will not be executed");
            
            // Use simple fixed progress value
            update_execution_progress(app_handle, 0.0, Some("Execution stopped as requested".to_string()))?;
            
            // Update execution run status to stopped
            finalize_execution_run(app_handle, "stopped", None);
            
            // Get the accumulated memory/clipboard contents
            let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

            // Emit automation_stopped event with clipboard data
            let _ = app_handle.emit("automation_stopped", serde_json::json!({
                "clipboard": if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
            }));
            
            return Ok(());
        }
        
        // Save execution step to database and emit event
        if let Some(execution_run_id) = *CURRENT_EXECUTION_RUN_ID.lock().unwrap() {
            // Get current step number
            let step_num = STEP_NUMBER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            
            // Extract explanation and next_step from reasoning (split by next step indicator)
            let (explanation, next_step) = if action.reasoning.contains("Next:") || action.reasoning.contains("next:") {
                let parts: Vec<&str> = action.reasoning.splitn(2, |c| c == '.' && action.reasoning[..action.reasoning.find('.').unwrap_or(0)].to_lowercase().contains("next")).collect();
                if parts.len() == 2 {
                    (Some(parts[0].trim().to_string()), Some(parts[1].trim().to_string()))
                } else {
                    (Some(action.reasoning.clone()), None)
                }
            } else {
                (Some(action.reasoning.clone()), None)
            };
            
            // Create step in database
            let step_id = app_handle
                .db(|db| {
                    crate::repository::automation_execution_repository::create_execution_step(
                        db,
                        execution_run_id,
                        step_num,
                        explanation.clone(),
                        next_step.clone(),
                        "executing",
                    )
                })
                .map_err(|e| format!("Failed to create execution step: {}", e))?;
            
            // Emit automation_step event for UI
            let _ = app_handle.emit("automation_step", serde_json::json!({
                "automationId": execution_state.automation_id,
                "explanation": explanation,
                "nextStep": next_step,
                "status": "executing"
            }));
            
            // Store step ID for later status update
            *CURRENT_STEP_ID.lock().unwrap() = Some(step_id);
        }
        
        // Execute the action
        info!("Executing action: {:?}", action);
        
        // Check if LLM requested ALL_ELEMENTS
        if action.action_type == ActionType::AllElements {
            info!("LLM requested full element tree - forcing fresh state update with navigation-style wait");

            // Wait for page to fully load before fetching elements (similar to NavigateURL wait)
            // This ensures we capture late-loading elements on complex sites like Amazon
            let initial_element_count = app_state.accessible_elements.len();
            info!("ALL_ELEMENTS: Initial element count before wait: {}", initial_element_count);

            // Initial wait to let any pending content load
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Wait for DOM stability with navigation-style conditions
            let wait_config = crate::engine::wait::WaitConfig {
                max_wait: std::time::Duration::from_secs(1),
                check_interval: std::time::Duration::from_millis(500),
            };

            let fresh_app_state = crate::engine::wait::wait_for_conditions(
                &[
                    crate::engine::wait::WaitCondition::DomStable { threshold_ms: 800 },
                    crate::engine::wait::WaitCondition::MinimumWait { duration: std::time::Duration::from_millis(500) },
                ],
                wait_config,
                || Box::pin(update_app_state())
            ).await.unwrap_or_else(|_| {
                // Fallback: if wait fails, do a simple state update
                info!("ALL_ELEMENTS: Wait failed, using synchronous update");
                app_state.clone()
            });

            info!("ALL_ELEMENTS: Fresh element count after wait: {} (was {})",
                fresh_app_state.accessible_elements.len(), initial_element_count);

            // Record the ALL_ELEMENTS request so LLM knows it already asked for this
            {
                let mut state_opt = APP_STATE.lock().unwrap();
                if let Some(mut state) = state_opt.clone() {
                    let enhanced_action = EnhancedAction {
                        command: "ALL_ELEMENTS".to_string(),
                        action_type: "AllElements".to_string(),
                        justification: "Requested full element tree".to_string(),
                        screen_context: summarize_current_screen(&fresh_app_state),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    state.recent_actions.push(enhanced_action);
                    // Also update the cached state with fresh elements
                    state.accessible_elements = fresh_app_state.accessible_elements.clone();
                    *state_opt = Some(state);
                }
            }

            // Create and send the full elements prompt using the FRESH state
            let all_elements_prompt = create_all_elements_prompt(&fresh_app_state);
            
            // Send to LLM using the same session
            let has_session = LLM_SESSION.lock().unwrap().is_some();
            
            if has_session {
                // Get API choice and key
                let api_choice = app_handle
                    .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
                    .setting_value;

                // Get use_pro_model setting (only relevant for proxy)
                let use_pro_model = app_handle
                    .db(|db| match get_setting(db, "use_pro_model") {
                        Ok(setting) => setting.setting_value == "true",
                        Err(_) => false,
                    });

                let api_key = match api_choice.as_str() {
                    "openai" => app_handle
                        .db(|db| get_setting(db, "api_key_open_ai").expect("Failed to get OpenAI API key"))
                        .setting_value,
                    "grok" => app_handle
                        .db(|db| get_setting(db, "api_key_grok").expect("Failed to get Grok API key"))
                        .setting_value,
                    "deepseek" => app_handle
                        .db(|db| get_setting(db, "api_key_deepseek").expect("Failed to get DeepSeek API key"))
                        .setting_value,
                    "gemini" => app_handle
                        .db(|db| get_setting(db, "api_key_gemini").expect("Failed to get Gemini API key"))
                        .setting_value,
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
                        
                        // Debug: Log token info (first 20 chars only for security)
                        let token_preview = if jwt_token.len() > 20 {
                            format!("{}...", truncate_str(&jwt_token, 20))
                        } else {
                            jwt_token.to_string()
                        };
                        info!("Retrieved JWT token from DB: {}", token_preview);
                        
                        jwt_token
                    },
                    "claude-subscription" => app_handle
                        .db(|db| get_setting(db, "api_key_claude_oauth").map(|s| s.setting_value).unwrap_or_default()),
                    "openai-codex" => crate::auth::openai_codex_oauth::load(&app_handle)
                        .map(|c| c.access)
                        .unwrap_or_default(),
                    "claude" | _ => app_handle
                        .db(|db| get_setting(db, "api_key_claude").expect("Failed to get Claude API key"))
                        .setting_value,
                };
                
                // Send the full elements prompt
                let response = match api_choice.as_str() {
                    "openai" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                // Clone the session so we can modify it
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) = 
                            crate::engine::llm_providers::openai::call_llm_api_with_session(
                                &api_key, &mut session, all_elements_prompt, 1000
                            ).await?;
                        
                        // Log token usage for ALL_ELEMENTS request
                        log_token_usage(input_tokens, output_tokens, "ALL_ELEMENTS");
                        
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        
                        response_text
                    },
                    "grok" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                // Clone the session so we can modify it
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) = 
                            crate::engine::llm_providers::grok::call_llm_api_with_session(
                                &api_key, &mut session, all_elements_prompt, 3000
                            ).await?;
                        
                        // Log token usage for ALL_ELEMENTS request
                        log_token_usage(input_tokens, output_tokens, "ALL_ELEMENTS");
                        
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        
                        response_text
                    },
                    "deepseek" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) = 
                            crate::engine::llm_providers::deepseek::call_llm_api_with_session(
                                &api_key, &mut session, all_elements_prompt, 3000
                            ).await?;
                        
                        log_token_usage(input_tokens, output_tokens, "ALL_ELEMENTS");
                        
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        
                        response_text
                    },
                    "gemini" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                // Clone the session so we can modify it
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) = 
                            crate::engine::llm_providers::gemini::call_llm_api_with_session(
                                &api_key, &mut session, all_elements_prompt, 4000
                            ).await?;
                        
                        // Log token usage for ALL_ELEMENTS request
                        log_token_usage(input_tokens, output_tokens, "ALL_ELEMENTS");
                        
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        
                        response_text
                    },
                    "proxy" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                // Clone the session so we can modify it
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };

                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }

                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) =
                            crate::engine::llm_providers::proxy::call_llm_api_with_session(
                                &api_key, &mut session, all_elements_prompt, 4000, use_pro_model,
                            ).await?;

                        log_token_usage(input_tokens, output_tokens, "ALL_ELEMENTS");

                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }

                        response_text
                    },
                    "openai-codex" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) =
                            crate::engine::llm_providers::openai_codex::call_llm_api_with_session(
                                &api_key, &mut session, all_elements_prompt, 1000
                            ).await?;
                        log_token_usage(input_tokens, output_tokens, "ALL_ELEMENTS");
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        response_text
                    },
                    "claude" | "claude-subscription" | _ => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                // Clone the session so we can modify it
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };

                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }

                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) =
                            crate::engine::llm_providers::claude::call_llm_api_with_session(
                                &api_key, &mut session, all_elements_prompt, 1000
                            ).await?;

                        // Log token usage for ALL_ELEMENTS request
                        log_token_usage(input_tokens, output_tokens, "ALL_ELEMENTS");

                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }

                        response_text
                    }
                };
                
                // Parse the new action from the response
                let new_action = parse_agent_action(&response)?;

                info!("After seeing all elements, LLM decided: {:?}", new_action.action_type);

                // Handle MemorySave specially since execute_action doesn't process it
                if new_action.action_type == ActionType::MemorySave {
                    if let Some(mem_text) = new_action.parameters.as_ref().and_then(|p| p.get("memory")) {
                        // Mirror into the structured memory_manager — the
                        // prompt reads from there in agent mode.
                        let _ = memory_manager::save_to_memory(None, mem_text);

                        // Save to MEMORY_STORE
                        let mut mem = MEMORY_STORE.lock().unwrap();
                        if !mem.is_empty() {
                            *mem = format!("{}\n---\n{}", mem_text, mem);
                        } else {
                            *mem = mem_text.to_string();
                        }
                        info!("ALL_ELEMENTS -> MEMORY_SAVE: Added {} chars to memory (total {} chars)", mem_text.len(), mem.len());

                        // Record in recent actions
                        {
                            let mut state_opt = APP_STATE.lock().unwrap();
                            if let Some(mut state) = state_opt.clone() {
                                let truncated_memory = if mem_text.len() > 100 {
                                    format!("{}...", truncate_str(mem_text, 100))
                                } else {
                                    mem_text.to_string()
                                };
                                let enhanced_action = EnhancedAction {
                                    command: format!("MEMORY_SAVE:{}", truncated_memory),
                                    action_type: "MemorySave".to_string(),
                                    justification: new_action.reasoning.clone(),
                                    screen_context: summarize_current_screen(&state),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                };
                                state.recent_actions.push(enhanced_action);
                                *state_opt = Some(state.clone());
                                last_app_state = Some(state);
                            }
                        }
                    }
                    continue;
                }

                // Now execute the actual action (non-MemorySave)
                match execute_action(app_handle, &new_action).await {
                    Ok(result_msg) => {
                        // Record the actual action (not the ALL_ELEMENTS request)
                        // result_msg contains the confirmation from the system
                        {
                            let mut state_opt = APP_STATE.lock().unwrap();
                            if let Some(mut state) = state_opt.clone() {
                                // Extract command representation
                                let command = match new_action.action_type {
                                    ActionType::LaunchApp => format!("LAUNCH:{}", new_action.app_name.as_deref().unwrap_or("")),
                                    ActionType::ClickElement => {
                                        // Try to find the element from the cache to get its details
                                        if let Some(target) = &new_action.target_element {
                                            #[cfg(target_os = "macos")]
                                            {
                                                let cache = ELEMENT_CACHE.lock().unwrap();
                                                if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| e.path == *target) {
                                                    let element_type = element.role.replace("AX", "");
                                                    let element_label = if !element.title.is_empty() {
                                                        &element.title
                                                    } else if !element.description.is_empty() {
                                                        &element.description
                                                    } else if !element.value.is_empty() {
                                                        &element.value
                                                    } else {
                                                        "unlabeled"
                                                    };
                                                    let truncated_label = truncate_element_label(element_label, 30);
                                                    format!("CLICK:{} {}:{}", index + 1, element_type, truncated_label)
                                                } else {
                                                    format!("CLICK:{}", target)
                                                }
                                            }
                                            #[cfg(not(target_os = "macos"))]
                                            {
                                                format!("CLICK:{}", target)
                                            }
                                        } else {
                                            "CLICK:?".to_string()
                                        }
                                    },
                                    ActionType::TypeText => {
                                        let text = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("text"))
                                            .map(|t| t.as_str())
                                            .unwrap_or("");
                                        // Try to find the element from the cache to get its details
                                        if let Some(target) = &new_action.target_element {
                                            format!("TYPE:{}:{}", target, text)
                                        } else {
                                            format!("TYPE:?:{}", text)
                                        }
                                    },
                                    ActionType::PressKey => {
                                        let key = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("key"))
                                            .map(|k| k.as_str())
                                            .unwrap_or("");
                                        format!("PRESS:{}", key)
                                    },
                                    ActionType::WaitTime => {
                                        let time = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("wait_time"))
                                            .map(|t| t.as_str())
                                            .unwrap_or("1");
                                        format!("WAIT:{}", time)
                                    },
                                    ActionType::Complete => "COMPLETE".to_string(),
                                    ActionType::RequestTakeover => "REQUEST_TAKEOVER".to_string(),
                                    ActionType::AskClarification => "ASK_CLARIFICATION".to_string(),
                                    ActionType::AllElements => "ALL_ELEMENTS".to_string(), // Should not reach here
                                    ActionType::FullText => "FULL_TEXT".to_string(), // Should not reach here
                                    ActionType::MemorySave => "MEMORY_SAVE".to_string(),
                                    ActionType::ExcelType => {
                                        let cell_data = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("cell_data"))
                                            .map(|d| d.as_str())
                                            .unwrap_or("");
                                        let cell_count = cell_data.split(":::").count();
                                        format!("EXCEL_TYPE:{} cells", cell_count)
                                    },
                                    ActionType::ExcelCommand => {
                                        let cmd = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("command"))
                                            .map(|c| c.as_str())
                                            .unwrap_or("UNKNOWN");
                                        format!("{}", cmd)
                                    },
                                    ActionType::WordCommand => {
                                        let cmd = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("command"))
                                            .map(|c| c.as_str())
                                            .unwrap_or("UNKNOWN");
                                        format!("{}", cmd)
                                    },
                                    ActionType::PowerPointCommand => {
                                        let cmd = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("command"))
                                            .map(|c| c.as_str())
                                            .unwrap_or("UNKNOWN");
                                        format!("{}", cmd)
                                    },
                                    ActionType::Stuck => "STUCK".to_string(),
                                    ActionType::ParseError => "PARSE_ERROR".to_string(), // Should not reach here
                                    ActionType::TerminalRun => {
                                        let cmd = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("command"))
                                            .map(|c| c.as_str())
                                            .unwrap_or("");
                                        let truncated = if cmd.len() > 50 { truncate_str(cmd, 50) } else { cmd };
                                        format!("TERMINAL_RUN:{}", truncated)
                                    },
                                    ActionType::TerminalBackground => {
                                        let cmd = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("command"))
                                            .map(|c| c.as_str())
                                            .unwrap_or("");
                                        let truncated = if cmd.len() > 50 { truncate_str(cmd, 50) } else { cmd };
                                        format!("TERMINAL_BACKGROUND:{}", truncated)
                                    },
                                    ActionType::TerminalCheck => {
                                        let pid = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("process_id"))
                                            .map(|p| p.as_str())
                                            .unwrap_or("");
                                        format!("TERMINAL_CHECK:{}", pid)
                                    },
                                    ActionType::TerminalKill => {
                                        let pid = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("process_id"))
                                            .map(|p| p.as_str())
                                            .unwrap_or("");
                                        format!("TERMINAL_KILL:{}", pid)
                                    },
                                    ActionType::TerminalRead => {
                                        let pid = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("process_id"))
                                            .map(|p| p.as_str())
                                            .unwrap_or("");
                                        format!("TERMINAL_READ:{}", pid)
                                    },
                                    ActionType::GoogleSearch => {
                                        let query = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("query"))
                                            .map(|q| q.as_str())
                                            .unwrap_or("");
                                        let truncated = if query.len() > 60 { truncate_str(query, 60) } else { query };
                                        format!("GOOGLE_SEARCH:{}", truncated)
                                    },
                                    ActionType::FetchPages => {
                                        let urls = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("urls"))
                                            .map(|u| u.as_str())
                                            .unwrap_or("");
                                        format!("FETCH_PAGES:{}", urls)
                                    },
                                    ActionType::WriteFile => {
                                        let path = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("path"))
                                            .map(|p| p.as_str())
                                            .unwrap_or("");
                                        format!("WRITE_FILE:{}", path)
                                    },
                                    ActionType::BrowserConsole => "BROWSER_CONSOLE".to_string(),
                                    ActionType::NavigateURL => {
                                        let url = new_action.parameters.as_ref()
                                            .and_then(|p| p.get("url"))
                                            .map(|u| u.as_str())
                                            .unwrap_or("");
                                        // Try to find the element from the cache to get its details
                                        if let Some(target) = &new_action.target_element {
                                            #[cfg(target_os = "macos")]
                                            {
                                                let cache = ELEMENT_CACHE.lock().unwrap();
                                                if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| e.path == *target) {
                                                    let element_type = element.role.replace("AX", "");
                                                    let truncated_label = truncate_element_label(&element.title, 20);
                                                    format!("URL:{} {}:{}:{}", index + 1, element_type, truncated_label, url)
                                                } else {
                                                    format!("URL:{}:{}", target, url)
                                                }
                                            }
                                            #[cfg(not(target_os = "macos"))]
                                            {
                                                let cache = ELEMENT_CACHE.lock().unwrap();
                                                if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| {
                                                    e.get("element_path").and_then(|v| v.as_str()).unwrap_or("") == *target ||
                                                    e.get("element_ref").and_then(|v| v.as_str()).unwrap_or("") == *target
                                                }) {
                                                    let element_type = element.get("element_type").and_then(|v| v.as_str()).unwrap_or("");
                                                    let element_name = element.get("element_name").and_then(|v| v.as_str()).unwrap_or("");
                                                    let truncated_label = truncate_element_label(element_name, 20);
                                                    format!("URL:{} {}:{}:{}", index + 1, element_type, truncated_label, url)
                                                } else {
                                                    format!("URL:{}:{}", target, url)
                                                }
                                            }
                                        } else {
                                            format!("URL:?:{}", url)
                                        }
                                    }
                                };
                                
                                // Include system result message in justification for LLM feedback
                                let justification_with_result = if result_msg.is_empty() || result_msg == "OK" {
                                    new_action.reasoning.clone()
                                } else {
                                    format!("{} | ✓ {}", new_action.reasoning.clone(), result_msg)
                                };
                                
                                let enhanced_action = EnhancedAction {
                                    command,
                                    action_type: format!("{:?}", new_action.action_type),
                                    justification: justification_with_result,
                                    screen_context: summarize_current_screen(&state),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                };
                                
                                state.recent_actions.push(enhanced_action);
                                *state_opt = Some(state);
                            }
                        }
                        
                        // Check if completion
                        if new_action.action_type == ActionType::Complete {
                            info!("Automation marked as completed after ALL_ELEMENTS review");
                            update_execution_progress(app_handle, 100.0, Some("Automation execution completed".to_string()))?;
                            
                            // Update execution run status to completed
                            finalize_execution_run(app_handle, "completed", None);
                            
                            // Get the accumulated memory/clipboard contents
                            let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

                            // Emit automation_stopped event with clipboard data
                            let _ = app_handle.emit("automation_stopped", serde_json::json!({
                                "clipboard": if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
                            }));
                            
                            return Ok(());
                        }
                        
                        // Smart wait after action
                        log_and_file("INFO", &format!("Starting smart_wait_after_action for {:?} (ALL_ELEMENTS)", new_action.action_type));
                        if let Some(new_state) = smart_wait_after_action(&new_action, &app_state, || Box::pin(update_app_state())).await? {
                            info!("Got fresh app state from smart wait after ALL_ELEMENTS - {} elements", new_state.accessible_elements.len());
                            log_and_file("INFO", &format!("smart_wait completed with {} elements (ALL_ELEMENTS)", new_state.accessible_elements.len()));
                            last_app_state = Some(new_state);
                        } else {
                            log_and_file("INFO", "smart_wait completed without updating state (ALL_ELEMENTS)");
                        }
                    },
                    Err(e) => {
                        error!("Failed to execute action after ALL_ELEMENTS: {}", e);
                        update_execution_progress(app_handle, 0.0, Some(format!("Error: {}", e)))?;
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            } else {
                return Err("Cannot request ALL_ELEMENTS without an active session".to_string());
            }

            // Continue to next iteration (ALL_ELEMENTS request is now recorded above)
            continue;
        }
        
        // Check if LLM requested FULL_TEXT
        if action.action_type == ActionType::FullText {
            info!("LLM requested full text content");

            // Fetch fresh full text via clipboard extraction (Windows) or relaxed scan (macOS)
            #[cfg(target_os = "windows")]
            let updated_app_state = {
                use crate::window_details_collector::windows::windows_nvda_bridge;
                let pid = app_state.current_pid.as_deref().unwrap_or("");
                info!("FULL_TEXT: Calling get_full_text_async for pid {}", pid);
                let (_elements, full_text) = windows_nvda_bridge::get_full_text_async(pid).await;
                info!("FULL_TEXT: Got {} chars from clipboard extraction", full_text.len());
                
                let mut state = app_state.clone();
                if !full_text.is_empty() {
                    state.full_text_content = Some(full_text);
                }
                state
            };
            
            // macOS: Try clipboard extraction first, fallback to AX tree scan
            #[cfg(target_os = "macos")]
            let updated_app_state = {
                use crate::window_details_collector::macos::macos_action_detector_engine::{get_full_text_content_by_pid, get_full_text_via_clipboard};
                let pid = app_state.current_pid.as_deref().unwrap_or("");

                let full_text = match get_full_text_via_clipboard(pid) {
                    Ok(text) => {
                        info!("FULL_TEXT: Got {} chars from clipboard extraction", text.len());
                        text
                    }
                    Err(e) => {
                        info!("FULL_TEXT: Clipboard extraction failed ({}), falling back to AX scan", e);
                        let ax_text = get_full_text_content_by_pid(pid);
                        info!("FULL_TEXT: Got {} chars from AX scan fallback", ax_text.len());
                        ax_text
                    }
                };

                let mut state = app_state.clone();
                if !full_text.is_empty() {
                    state.full_text_content = Some(full_text);
                }
                state
            };

            // Record the FULL_TEXT request so LLM knows it already asked for this
            {
                let mut state_opt = APP_STATE.lock().unwrap();
                if let Some(mut state) = state_opt.clone() {
                    let enhanced_action = EnhancedAction {
                        command: "FULL_TEXT".to_string(),
                        action_type: "FullText".to_string(),
                        justification: "Requested full text content of the page".to_string(),
                        screen_context: summarize_current_screen(&state),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    state.recent_actions.push(enhanced_action);
                    // Also update the cached state with fresh full_text_content
                    state.full_text_content = updated_app_state.full_text_content.clone();
                    *state_opt = Some(state);
                }
            }

            // Create and send the full text prompt using the UPDATED state with fresh clipboard content
            let full_text_prompt = create_full_text_prompt(&updated_app_state);

            // Log what we're actually sending to the LLM
            info!("FULL_TEXT: Sending prompt to LLM ({} chars total)", full_text_prompt.len());
            log_and_file("INFO", &format!("FULL_TEXT prompt content:\n{}", full_text_prompt));
            
            // Send to LLM using the same session
            let has_session = LLM_SESSION.lock().unwrap().is_some();
            
            if has_session {
                // Get API choice and key (same logic as ALL_ELEMENTS)
                let api_choice = app_handle
                    .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
                    .setting_value;

                // Get use_pro_model setting (only relevant for proxy)
                let use_pro_model = app_handle
                    .db(|db| match get_setting(db, "use_pro_model") {
                        Ok(setting) => setting.setting_value == "true",
                        Err(_) => false,
                    });

                let api_key = match api_choice.as_str() {
                    "openai" => app_handle
                        .db(|db| get_setting(db, "api_key_open_ai").expect("Failed to get OpenAI API key"))
                        .setting_value,
                    "grok" => app_handle
                        .db(|db| get_setting(db, "api_key_grok").expect("Failed to get Grok API key"))
                        .setting_value,
                    "deepseek" => app_handle
                        .db(|db| get_setting(db, "api_key_deepseek").expect("Failed to get DeepSeek API key"))
                        .setting_value,
                    "gemini" => app_handle
                        .db(|db| get_setting(db, "api_key_gemini").expect("Failed to get Gemini API key"))
                        .setting_value,
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
                    "claude-subscription" => app_handle
                        .db(|db| get_setting(db, "api_key_claude_oauth").map(|s| s.setting_value).unwrap_or_default()),
                    "openai-codex" => crate::auth::openai_codex_oauth::load(&app_handle)
                        .map(|c| c.access)
                        .unwrap_or_default(),
                    "claude" | _ => app_handle
                        .db(|db| get_setting(db, "api_key_claude").expect("Failed to get Claude API key"))
                        .setting_value,
                };
                
                // Send the full text prompt
                let response = match api_choice.as_str() {
                    "openai" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) = 
                            crate::engine::llm_providers::openai::call_llm_api_with_session(
                                &api_key, &mut session, full_text_prompt, 1000
                            ).await?;
                        
                        // Log token usage for FULL_TEXT request
                        log_token_usage(input_tokens, output_tokens, "FULL_TEXT");
                        
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        
                        response_text
                    },
                    "grok" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) = 
                            crate::engine::llm_providers::grok::call_llm_api_with_session(
                                &api_key, &mut session, full_text_prompt, 500
                            ).await?;
                        
                        // Log token usage for FULL_TEXT request
                        log_token_usage(input_tokens, output_tokens, "FULL_TEXT");
                        
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        
                        response_text
                    },
                    "deepseek" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) = 
                            crate::engine::llm_providers::deepseek::call_llm_api_with_session(
                                &api_key, &mut session, full_text_prompt, 500
                            ).await?;
                        
                        log_token_usage(input_tokens, output_tokens, "FULL_TEXT");
                        
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        
                        response_text
                    },
                    "gemini" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                // Clone the session so we can modify it
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) = 
                            crate::engine::llm_providers::gemini::call_llm_api_with_session(
                                &api_key, &mut session, full_text_prompt, 4000
                            ).await?;
                        
                        // Log token usage for FULL_TEXT request
                        log_token_usage(input_tokens, output_tokens, "FULL_TEXT");
                        
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        
                        response_text
                    },
                    "proxy" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };

                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }

                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) =
                            crate::engine::llm_providers::proxy::call_llm_api_with_session(
                                &api_key, &mut session, full_text_prompt, 4000, use_pro_model,
                            ).await?;

                        log_token_usage(input_tokens, output_tokens, "FULL_TEXT");

                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }

                        response_text
                    },
                    "openai-codex" => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };
                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }
                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) =
                            crate::engine::llm_providers::openai_codex::call_llm_api_with_session(
                                &api_key, &mut session, full_text_prompt, 1000
                            ).await?;
                        log_token_usage(input_tokens, output_tokens, "FULL_TEXT");
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }
                        response_text
                    },
                    "claude" | "claude-subscription" | _ => {
                        let (session_data, session_exists) = {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                (Some(session.clone()), true)
                            } else {
                                (None, false)
                            }
                        };

                        if !session_exists {
                            return Err("No LLM session available".to_string());
                        }

                        let mut session = session_data.unwrap();
                        let (response_text, input_tokens, output_tokens) =
                            crate::engine::llm_providers::claude::call_llm_api_with_session(
                                &api_key, &mut session, full_text_prompt, 1000
                            ).await?;

                        // Log token usage for FULL_TEXT request
                        log_token_usage(input_tokens, output_tokens, "FULL_TEXT");

                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            *session_guard = Some(session);
                        }

                        response_text
                    }
                };
                
                // Log the raw LLM response for FULL_TEXT
                info!("FULL_TEXT: Raw LLM response: {}", response);
                log_and_file("DEBUG", &format!("FULL_TEXT LLM response:\n{}", response));

                // Parse the new action from the response
                let new_action = parse_agent_action(&response)?;

                info!("After seeing full text, LLM decided: {:?}", new_action.action_type);

                // Handle MemorySave specially since execute_action doesn't process it
                if new_action.action_type == ActionType::MemorySave {
                    if let Some(mem_text) = new_action.parameters.as_ref().and_then(|p| p.get("memory")) {
                        // Mirror into structured memory_manager so prompts see it.
                        let _ = memory_manager::save_to_memory(None, mem_text);

                        // Save to MEMORY_STORE
                        let mut mem = MEMORY_STORE.lock().unwrap();
                        if !mem.is_empty() {
                            *mem = format!("{}\n---\n{}", mem_text, mem);
                        } else {
                            *mem = mem_text.to_string();
                        }
                        info!("FULL_TEXT -> MEMORY_SAVE: Added {} chars to memory (total {} chars)", mem_text.len(), mem.len());

                        // Record in recent actions
                        {
                            let mut state_opt = APP_STATE.lock().unwrap();
                            if let Some(mut state) = state_opt.clone() {
                                let truncated_memory = if mem_text.len() > 100 {
                                    format!("{}...", truncate_str(mem_text, 100))
                                } else {
                                    mem_text.to_string()
                                };
                                let enhanced_action = EnhancedAction {
                                    command: format!("MEMORY_SAVE:{}", truncated_memory),
                                    action_type: "MemorySave".to_string(),
                                    justification: new_action.reasoning.clone(),
                                    screen_context: summarize_current_screen(&state),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                };
                                state.recent_actions.push(enhanced_action);
                                *state_opt = Some(state.clone());
                                last_app_state = Some(state);
                            }
                        }
                    }
                    continue;
                }

                // Now execute the actual action (non-MemorySave)
                match execute_action(app_handle, &new_action).await {
                    Ok(result_msg) => {
                        // Record the actual action (not the FULL_TEXT request)
                        {
                            let mut state_opt = APP_STATE.lock().unwrap();
                            if let Some(mut state) = state_opt.clone() {
                                let command = format!("{:?}", new_action.action_type);
                                
                                // Include system result message in justification for LLM feedback
                                let justification_with_result = if result_msg.is_empty() || result_msg == "OK" {
                                    new_action.reasoning.clone()
                                } else {
                                    format!("{} | ✓ {}", new_action.reasoning.clone(), result_msg)
                                };
                                
                                let enhanced_action = EnhancedAction {
                                    command,
                                    action_type: format!("{:?}", new_action.action_type),
                                    justification: justification_with_result,
                                    screen_context: summarize_current_screen(&state),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                };
                                
                                state.recent_actions.push(enhanced_action);
                                *state_opt = Some(state);
                            }
                        }
                        
                        // Check if completion
                        if new_action.action_type == ActionType::Complete {
                            info!("Automation marked as completed after FULL_TEXT review");
                            update_execution_progress(app_handle, 100.0, Some("Automation execution completed".to_string()))?;
                            
                            // Update execution run status to completed
                            finalize_execution_run(app_handle, "completed", None);
                            
                            // Get the accumulated memory/clipboard contents
                            let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

                            // Emit automation_stopped event with clipboard data
                            let _ = app_handle.emit("automation_stopped", serde_json::json!({
                                "clipboard": if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
                            }));
                            
                            return Ok(());
                        }
                        
                        // Smart wait after action
                        if let Some(new_state) = smart_wait_after_action(&new_action, &app_state, || Box::pin(update_app_state())).await? {
                            info!("Got fresh app state from smart wait after FULL_TEXT - {} elements", new_state.accessible_elements.len());
                            last_app_state = Some(new_state);
                        }
                    },
                    Err(e) => {
                        error!("Failed to execute action after FULL_TEXT: {}", e);
                        update_execution_progress(app_handle, 0.0, Some(format!("Error: {}", e)))?;
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            } else {
                return Err("Cannot request FULL_TEXT without an active session".to_string());
            }

            // Continue to next iteration (FULL_TEXT request is now recorded above)
            continue;
        }
        
        // DUPLICATE-CLICK SUPPRESSION: Avoid clicking same element thrice in a row without state change
        if action.action_type == ActionType::ClickElement {
            let len = app_state.recent_actions.len();
            if len >= 2 {
                // Get the LAST 2 actions (most recent), not the first 2
                let last_actions = &app_state.recent_actions[len - 2..];
                let prospective_command = {
                    if let Some(target) = &action.target_element {
                        // Try to map to index if possible for comparison; fall back to target path
                        format!("CLICK:{}", target)
                    } else {
                        "CLICK".to_string()
                    }
                };

                let duplicate = last_actions.iter().all(|a| a.command.starts_with("CLICK") && a.command.contains(&prospective_command));
                if duplicate {
                    warn!("Suppressing duplicate click {:?}", prospective_command);
                    // insert small wait and skip execution
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    continue;
                }
            }
        }
        
        // Handle MemorySave actions separately (they don't interact with UI)
        if action.action_type == ActionType::MemorySave {
            if let Some(mem_text) = action.parameters.as_ref().and_then(|p| p.get("memory")) {
                // Mirror into the structured memory_manager so the next prompt's
                // CURRENT MEMORY block (which reads memory_manager) sees this entry.
                // Without this, MEMORY_SAVE writes only land in the raw String
                // store and the LLM never sees what it just saved.
                let _ = memory_manager::save_to_memory(None, mem_text);

                let mut mem = MEMORY_STORE.lock().unwrap();

                // Prepend new memory (newest at top for easier matching)
                if !mem.is_empty() {
                    *mem = format!("{}\n---\n{}", mem_text, mem);
                } else {
                    *mem = mem_text.to_string();
                }

                // Cap total memory to 50000 chars (trim from end = oldest entries)
                if mem.len() > 50000 {
                    // Find a separator near the end to trim at cleanly
                    let target_len = 45000;
                    if let Some(sep_pos) = mem[target_len..].find("\n---\n") {
                        // Trim everything after this separator (oldest entries)
                        mem.truncate(target_len + sep_pos);
                        info!("Memory exceeded 50000 chars, removed oldest entries, now {} chars", mem.len());
                    } else {
                        // If no separator found, just keep the first 45000 chars
                        let mut char_boundary = target_len;
                        while char_boundary < mem.len() && !mem.is_char_boundary(char_boundary) {
                            char_boundary += 1;
                        }
                        mem.truncate(char_boundary);
                        info!("Memory exceeded 50000 chars, truncated to first 45000 chars");
                    }
                }
                
                info!("Added to MEMORY_STORE (total {} chars)", mem.len());

                // Add the MEMORY_SAVE action to recent actions and get updated state
                let updated_state = {
                    let mut state_opt = APP_STATE.lock().unwrap();
                    if let Some(mut state) = state_opt.clone() {
                        // Truncate the memory text for the action history (show first 100 chars)
                        let truncated_memory = if mem_text.len() > 100 {
                            format!("{}...", truncate_str(mem_text, 100))
                        } else {
                            mem_text.to_string()
                        };

                        let enhanced_action = EnhancedAction {
                            command: format!("MEMORY_SAVE:{}", truncated_memory),
                            action_type: "MemorySave".to_string(),
                            justification: action.reasoning.clone(),
                            screen_context: summarize_current_screen(&state),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        };

                        state.recent_actions.push(enhanced_action);
                        *state_opt = Some(state.clone());
                        Some(state)
                    } else {
                        None
                    }
                };
                
                // Use updated state so next iteration sees the MEMORY_SAVE action
                last_app_state = updated_state.or(Some(app_state));
            } else {
                // No memory text, preserve original app state
                last_app_state = Some(app_state);
            }
            // Skip UI execution and continue loop
            continue;
        }
        
        // Handle ParseError action type - send feedback to LLM and continue
        if action.action_type == ActionType::ParseError {
            if let Some(params) = &action.parameters {
                let error_msg = params.get("error").unwrap_or(&"Unknown parse error".to_string()).clone();
                let original_response = params.get("response").unwrap_or(&"".to_string()).clone();
                let error_count = params.get("error_count").unwrap_or(&"1".to_string()).clone();

                info!("Handling parse error #{}: {}", error_count, error_msg);
                log_and_file("ERROR", &format!("Parse error #{}: {}", error_count, error_msg));

                // Add error feedback to LLM session
                if let Some(session) = LLM_SESSION.lock().unwrap().as_mut() {
                    let error_feedback = format!(
                        "ERROR: Your last response could not be parsed. {}\n\n\
                        IMPORTANT: You MUST follow the exact command format:\n\
                        - TYPE:<element_number>:<text> (e.g., TYPE:5:hello world)\n\
                        - CLICK:<element_number> (e.g., CLICK:3)\n\
                        - Use ONLY the element numbers from the list\n\
                        - Do NOT include element descriptions in commands\n\n\
                        Your response that failed: {}\n\n\
                        Please provide a valid command now.",
                        error_msg,
                        original_response
                    );
                    session.add_user_message(error_feedback);
                    log_and_file("INFO", "Added parse error feedback to LLM session");
                }

                // Continue to next iteration to get corrected response
                continue;
            }
        }

        // Final check before executing the action - in case stop was requested during processing
        let should_stop_before_execution = {
            *SHOULD_STOP_EXECUTION.lock().unwrap()
        };

        if should_stop_before_execution {
            info!("Stopping automation execution before action - action will not be executed");

            // Use simple fixed progress value
            update_execution_progress(app_handle, 0.0, Some("Execution stopped as requested".to_string()))?;

            // Update execution run status to stopped
            finalize_execution_run(app_handle, "stopped", None);

            // Get the accumulated memory/clipboard contents
            let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

            // Emit automation_stopped event with clipboard data
            let _ = app_handle.emit("automation_stopped", serde_json::json!({
                "clipboard": if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
            }));

            return Ok(());
        }

        match execute_action(app_handle, &action).await {
            Ok(result_msg) => {
                // Record the action in APP_STATE with enhanced information
                // result_msg contains the confirmation from the system (e.g., "Clicked", "Text inserted")
                {
                    let mut state_opt = APP_STATE.lock().unwrap();
                    if let Some(mut state) = state_opt.clone() {
                        // Extract command representation
                        let command = match action.action_type {
                            ActionType::LaunchApp => format!("LAUNCH:{}", action.app_name.as_deref().unwrap_or("")),
                            ActionType::ClickElement => {
                                // Try to find the element from the cache to get its details
                                if let Some(target) = &action.target_element {
                                    #[cfg(target_os = "macos")]
                                    {
                                        let cache = ELEMENT_CACHE.lock().unwrap();
                                        if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| e.path == *target) {
                                            let element_type = element.role.replace("AX", "");
                                            let element_label = if !element.title.is_empty() {
                                                &element.title
                                            } else if !element.description.is_empty() {
                                                &element.description
                                            } else if !element.value.is_empty() {
                                                &element.value
                                            } else {
                                                "unlabeled"
                                            };
                                            let truncated_label = truncate_element_label(element_label, 30);
                                            format!("CLICK:{} {}:{}", index + 1, element_type, truncated_label)
                                        } else {
                                            format!("CLICK:{}", target)
                                        }
                                    }
                                    #[cfg(not(target_os = "macos"))]
                                    {
                                        format!("CLICK:{}", target)
                                    }
                                } else {
                                    "CLICK:?".to_string()
                                }
                            },
                            ActionType::TypeText => {
                                let text = action.parameters.as_ref()
                                    .and_then(|p| p.get("text"))
                                    .map(|t| t.as_str())
                                    .unwrap_or("");
                                
                                // Check if this is multi-element typing
                                if let Some(target) = &action.target_element {
                                    if target.contains("|||") {
                                        // Multi-element typing: show summary
                                        let target_elements: Vec<&str> = target.split("|||").map(|s| s.trim()).collect();
                                        let texts: Vec<&str> = text.split("|||").map(|s| s.trim()).collect();
                                        
                                        if target_elements.len() == texts.len() {
                                            let mut type_summaries = Vec::new();
                                            
                                            #[cfg(target_os = "macos")]
                                            {
                                                let cache = ELEMENT_CACHE.lock().unwrap();
                                                for (element_path, _text_to_type) in target_elements.iter().zip(texts.iter()) {
                                                    if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| e.path == *element_path) {
                                                        let element_type = element.role.replace("AX", "");
                                                        let element_label = if !element.title.is_empty() {
                                                            &element.title
                                                        } else if !element.description.is_empty() {
                                                            &element.description
                                                        } else {
                                                            "unlabeled"
                                                        };
                                                        let truncated_label = truncate_element_label(element_label, 20);
                                                        type_summaries.push(format!("{}({}:{})", index + 1, element_type, truncated_label));
                                                    } else {
                                                        type_summaries.push(format!("{}(?)", element_path));
                                                    }
                                                }
                                            }
                                            #[cfg(not(target_os = "macos"))]
                                            {
                                                let cache = ELEMENT_CACHE.lock().unwrap();
                                                for (element_path, _text_to_type) in target_elements.iter().zip(texts.iter()) {
                                                    if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| {
                                                        e.get("element_path").and_then(|v| v.as_str()).unwrap_or("") == *element_path ||
                                                        e.get("element_ref").and_then(|v| v.as_str()).unwrap_or("") == *element_path
                                                    }) {
                                                        let element_type = element.get("element_type").and_then(|v| v.as_str()).unwrap_or("");
                                                        let element_name = element.get("element_name").and_then(|v| v.as_str()).unwrap_or("");
                                                        let element_desc = element.get("element_description").and_then(|v| v.as_str()).unwrap_or("");
                                                        let element_label = if !element_name.is_empty() {
                                                            element_name
                                                        } else if !element_desc.is_empty() {
                                                            element_desc
                                                        } else {
                                                            "unlabeled"
                                                        };
                                                        let truncated_label = truncate_element_label(element_label, 20);
                                                        type_summaries.push(format!("{}({}:{})", index + 1, element_type, truncated_label));
                                                    } else {
                                                        type_summaries.push(format!("{}(?)", element_path));
                                                    }
                                                }
                                            }
                                            
                                            format!("TYPE_MULTI:{} elements [{}]", target_elements.len(), type_summaries.join(", "))
                                        } else {
                                            format!("TYPE_MULTI:{}:{}", target_elements.len(), text)
                                        }
                                    } else {
                                        // Single element typing (existing logic)
                                        #[cfg(target_os = "macos")]
                                        {
                                            let cache = ELEMENT_CACHE.lock().unwrap();
                                            if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| e.path == *target) {
                                                let element_type = element.role.replace("AX", "");
                                                let element_label = if !element.title.is_empty() {
                                                    &element.title
                                                } else if !element.description.is_empty() {
                                                    &element.description
                                                } else {
                                                    "unlabeled"
                                                };
                                                let truncated_label = truncate_element_label(element_label, 30);
                                                format!("TYPE:{} {}:{}:{}", index + 1, element_type, truncated_label, text)
                                            } else {
                                                format!("TYPE:{}:{}", target, text)
                                            }
                                        }
                                        #[cfg(not(target_os = "macos"))]
                                        {
                                            let cache = ELEMENT_CACHE.lock().unwrap();
                                            if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| {
                                                e.get("element_path").and_then(|v| v.as_str()).unwrap_or("") == *target ||
                                                e.get("element_ref").and_then(|v| v.as_str()).unwrap_or("") == *target
                                            }) {
                                                let element_type = element.get("element_type").and_then(|v| v.as_str()).unwrap_or("");
                                                let element_name = element.get("element_name").and_then(|v| v.as_str()).unwrap_or("");
                                                let element_desc = element.get("element_description").and_then(|v| v.as_str()).unwrap_or("");
                                                let element_label = if !element_name.is_empty() {
                                                    element_name
                                                } else if !element_desc.is_empty() {
                                                    element_desc
                                                } else {
                                                    "unlabeled"
                                                };
                                                let truncated_label = truncate_element_label(element_label, 30);
                                                format!("TYPE:{} {}:{}:{}", index + 1, element_type, truncated_label, text)
                                            } else {
                                                format!("TYPE:{}:{}", target, text)
                                            }
                                        }
                                    }
                                } else {
                                    format!("TYPE:?:{}", text)
                                }
                            },
                            ActionType::PressKey => {
                                let key = action.parameters.as_ref()
                                    .and_then(|p| p.get("key"))
                                    .map(|k| k.as_str())
                                    .unwrap_or("");
                                format!("PRESS:{}", key)
                            },
                            ActionType::WaitTime => {
                                let time = action.parameters.as_ref()
                                    .and_then(|p| p.get("wait_time"))
                                    .map(|t| t.as_str())
                                    .unwrap_or("1");
                                format!("WAIT:{}", time)
                            },
                            ActionType::Complete => "COMPLETE".to_string(),
                            ActionType::RequestTakeover => "REQUEST_TAKEOVER".to_string(),
                            ActionType::AskClarification => "ASK_CLARIFICATION".to_string(),
                            ActionType::AllElements => "ALL_ELEMENTS".to_string(),
                            ActionType::FullText => "FULL_TEXT".to_string(),
                            ActionType::MemorySave => "MEMORY_SAVE".to_string(),
                            ActionType::ExcelType => {
                                let cell_data = action.parameters.as_ref()
                                    .and_then(|p| p.get("cell_data"))
                                    .map(|d| d.as_str())
                                    .unwrap_or("");
                                let cell_count = cell_data.split(":::").count();
                                format!("EXCEL_TYPE:{} cells", cell_count)
                            },
                            ActionType::ExcelCommand => {
                                let cmd = action.parameters.as_ref()
                                    .and_then(|p| p.get("command"))
                                    .map(|c| c.as_str())
                                    .unwrap_or("UNKNOWN");
                                format!("{}", cmd)
                            },
                            ActionType::WordCommand => {
                                let cmd = action.parameters.as_ref()
                                    .and_then(|p| p.get("command"))
                                    .map(|c| c.as_str())
                                    .unwrap_or("UNKNOWN");
                                format!("{}", cmd)
                            },
                            ActionType::PowerPointCommand => {
                                let cmd = action.parameters.as_ref()
                                    .and_then(|p| p.get("command"))
                                    .map(|c| c.as_str())
                                    .unwrap_or("UNKNOWN");
                                format!("{}", cmd)
                            },
                            ActionType::Stuck => "STUCK".to_string(),
                            ActionType::ParseError => "PARSE_ERROR".to_string(), // Should not reach here
                            ActionType::TerminalRun => {
                                let cmd = action.parameters.as_ref()
                                    .and_then(|p| p.get("command"))
                                    .map(|c| c.as_str())
                                    .unwrap_or("");
                                let truncated = if cmd.len() > 50 { truncate_str(cmd, 50) } else { cmd };
                                format!("TERMINAL_RUN:{}", truncated)
                            },
                            ActionType::TerminalBackground => {
                                let cmd = action.parameters.as_ref()
                                    .and_then(|p| p.get("command"))
                                    .map(|c| c.as_str())
                                    .unwrap_or("");
                                let truncated = if cmd.len() > 50 { truncate_str(cmd, 50) } else { cmd };
                                format!("TERMINAL_BACKGROUND:{}", truncated)
                            },
                            ActionType::TerminalCheck => {
                                let pid = action.parameters.as_ref()
                                    .and_then(|p| p.get("process_id"))
                                    .map(|p| p.as_str())
                                    .unwrap_or("");
                                format!("TERMINAL_CHECK:{}", pid)
                            },
                            ActionType::TerminalKill => {
                                let pid = action.parameters.as_ref()
                                    .and_then(|p| p.get("process_id"))
                                    .map(|p| p.as_str())
                                    .unwrap_or("");
                                format!("TERMINAL_KILL:{}", pid)
                            },
                            ActionType::TerminalRead => {
                                let pid = action.parameters.as_ref()
                                    .and_then(|p| p.get("process_id"))
                                    .map(|p| p.as_str())
                                    .unwrap_or("");
                                format!("TERMINAL_READ:{}", pid)
                            },
                            ActionType::GoogleSearch => {
                                let query = action.parameters.as_ref()
                                    .and_then(|p| p.get("query"))
                                    .map(|q| q.as_str())
                                    .unwrap_or("");
                                let truncated = if query.len() > 60 { truncate_str(query, 60) } else { query };
                                format!("GOOGLE_SEARCH:{}", truncated)
                            },
                            ActionType::FetchPages => {
                                let urls = action.parameters.as_ref()
                                    .and_then(|p| p.get("urls"))
                                    .map(|u| u.as_str())
                                    .unwrap_or("");
                                format!("FETCH_PAGES:{}", urls)
                            },
                            ActionType::WriteFile => {
                                let path = action.parameters.as_ref()
                                    .and_then(|p| p.get("path"))
                                    .map(|p| p.as_str())
                                    .unwrap_or("");
                                format!("WRITE_FILE:{}", path)
                            },
                            ActionType::BrowserConsole => "BROWSER_CONSOLE".to_string(),
                            ActionType::NavigateURL => {
                                let url = action.parameters.as_ref()
                                    .and_then(|p| p.get("url"))
                                    .map(|u| u.as_str())
                                    .unwrap_or("");
                                // Try to find the element from the cache to get its details
                                if let Some(target) = &action.target_element {
                                    #[cfg(target_os = "macos")]
                                    {
                                        let cache = ELEMENT_CACHE.lock().unwrap();
                                        if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| e.path == *target) {
                                            let element_type = element.role.replace("AX", "");
                                            let truncated_label = truncate_element_label(&element.title, 20);
                                            format!("URL:{} {}:{}:{}", index + 1, element_type, truncated_label, url)
                                        } else {
                                            format!("URL:{}:{}", target, url)
                                        }
                                    }
                                    #[cfg(not(target_os = "macos"))]
                                    {
                                        let cache = ELEMENT_CACHE.lock().unwrap();
                                        if let Some((index, element)) = cache.iter().enumerate().find(|(_, e)| {
                                            e.get("element_path").and_then(|v| v.as_str()).unwrap_or("") == *target ||
                                            e.get("element_ref").and_then(|v| v.as_str()).unwrap_or("") == *target
                                        }) {
                                            let element_type = element.get("element_type").and_then(|v| v.as_str()).unwrap_or("");
                                            let element_name = element.get("element_name").and_then(|v| v.as_str()).unwrap_or("");
                                            let truncated_label = truncate_element_label(element_name, 20);
                                            format!("URL:{} {}:{}:{}", index + 1, element_type, truncated_label, url)
                                        } else {
                                            format!("URL:{}:{}", target, url)
                                        }
                                    }
                                } else {
                                    format!("URL:?:{}", url)
                                }
                            }
                        };
                        
                        // Include system result message in justification for LLM feedback
                        let justification_with_result = if result_msg.is_empty() || result_msg == "OK" {
                            action.reasoning.clone()
                        } else {
                            format!("{} | ✓ {}", action.reasoning.clone(), result_msg)
                        };
                        
                        let enhanced_action = EnhancedAction {
                            command,
                            action_type: format!("{:?}", action.action_type),
                            justification: justification_with_result,
                            screen_context: summarize_current_screen(&state),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        };
                        
                        state.recent_actions.push(enhanced_action);
                        *state_opt = Some(state);
                    }
                }
                
                // Check if the automation was completed by the LLM
                if action.action_type == ActionType::Complete {
                    info!("Automation marked as completed by LLM's decision");
                    update_execution_progress(app_handle, 100.0, Some("Automation execution completed".to_string()))?;
                    
                    // Update execution run status to completed
                    finalize_execution_run(app_handle, "completed", None);
                    
                    // Get the accumulated memory/clipboard contents
                    let clipboard_contents = MEMORY_STORE.lock().unwrap().clone();

                    // Emit automation_stopped event with clipboard data
                    let _ = app_handle.emit("automation_stopped", serde_json::json!({
                        "clipboard": if clipboard_contents.is_empty() { None } else { Some(clipboard_contents) }
                    }));
                    
                    return Ok(());
                }
                
                // IMPORTANT: SMART WAIT based on action type and context
                // This ensures appropriate wait times for different scenarios
                // Smart wait returns the final app state if it was updated
                log_and_file("INFO", &format!("Starting smart_wait_after_action for {:?}", action.action_type));
                if let Some(new_state) = smart_wait_after_action(&action, &app_state, || Box::pin(update_app_state())).await? {
                    info!("Got fresh app state from smart wait - {} elements", new_state.accessible_elements.len());
                    log_and_file("INFO", &format!("smart_wait completed with {} elements", new_state.accessible_elements.len()));
                    last_app_state = Some(new_state);
                } else {
                    log_and_file("INFO", "smart_wait completed without updating state");
                }
                
            },
            Err(e) => {
                error!("Failed to execute action: {}", e);

                // IMPORTANT: Record failed actions so LLM knows they were attempted and doesn't loop
                // This applies to PRESS, CLICK, TYPE, ExcelCommand, WordCommand, and other actions that can fail
                {
                    let mut state_opt = APP_STATE.lock().unwrap();
                    if let Some(mut state) = state_opt.clone() {
                        let (command, action_type_str) = match action.action_type {
                            ActionType::PressKey => {
                                let key = action.parameters.as_ref()
                                    .and_then(|p| p.get("key"))
                                    .map(|k| k.as_str())
                                    .unwrap_or("");
                                (format!("PRESS:{} (FAILED)", key), "PressKey".to_string())
                            },
                            ActionType::ClickElement => {
                                let target = action.target_element.as_deref().unwrap_or("?");
                                (format!("CLICK:{} (FAILED)", target), "ClickElement".to_string())
                            },
                            ActionType::TypeText => {
                                let text = action.parameters.as_ref()
                                    .and_then(|p| p.get("text"))
                                    .map(|t| t.as_str())
                                    .unwrap_or("");
                                let truncated = if text.len() > 20 { truncate_str(text, 20) } else { text };
                                (format!("TYPE:'{}...' (FAILED)", truncated), "TypeText".to_string())
                            },
                            ActionType::ExcelCommand => {
                                let cmd = action.parameters.as_ref()
                                    .and_then(|p| p.get("command"))
                                    .map(|c| c.as_str())
                                    .unwrap_or("UNKNOWN");
                                (format!("{} (FAILED)", cmd), "ExcelCommand".to_string())
                            },
                            ActionType::ExcelType => {
                                let cell_data = action.parameters.as_ref()
                                    .and_then(|p| p.get("cell_data"))
                                    .map(|d| d.as_str())
                                    .unwrap_or("");
                                let cell_count = cell_data.split(":::").count();
                                (format!("EXCEL_TYPE:{} cells (FAILED)", cell_count), "ExcelType".to_string())
                            },
                            ActionType::WordCommand => {
                                let cmd = action.parameters.as_ref()
                                    .and_then(|p| p.get("command"))
                                    .map(|c| c.as_str())
                                    .unwrap_or("UNKNOWN");
                                (format!("{} (FAILED)", cmd), "WordCommand".to_string())
                            },
                            ActionType::LaunchApp => {
                                let app = action.app_name.as_deref().unwrap_or("?");
                                (format!("LAUNCH:{} (FAILED)", app), "LaunchApp".to_string())
                            },
                            _ => {
                                (format!("{:?} (FAILED)", action.action_type), format!("{:?}", action.action_type))
                            }
                        };

                        log_and_file("WARN", &format!("Recorded failed action in history: {}", command));

                        let enhanced_action = EnhancedAction {
                            command,
                            action_type: action_type_str,
                            justification: format!("{} | ERROR: {}", action.reasoning.clone(), e),
                            screen_context: summarize_current_screen(&state),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        };

                        state.recent_actions.push(enhanced_action);
                        *state_opt = Some(state);
                    }
                }

                update_execution_progress(app_handle, 0.0, Some(format!("Error: {}", e)))?;

                // Shorter delay to allow user to see the error
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        
        // Update the global state with the updated execution state
        {
            let mut state = AUTOMATION_STATE.lock().unwrap();
            *state = Some(execution_state.clone());
            
            // Log state update with key info
            info!(
                "Updated global state for automation {}: Status: {}", 
                execution_state.automation_id,
                execution_state.status
            );
        }
    }
}

/// Update the current application state by querying accessibility APIs
async fn update_app_state() -> Result<AppState, String> {
    let update_start = std::time::Instant::now();
    info!("🔄 UPDATE_APP_STATE: Starting app state update");
    
    // Get and clone the current app state, then drop the lock
    let current_state = {
        let guard = APP_STATE.lock().unwrap();
        guard.clone()
    };
    
    let mut app_state = current_state.unwrap_or(AppState {
        current_app: None,
        current_pid: None,
        accessible_elements: Vec::new(),
        text_content: None,
        full_text_content: None,
        word_document_info: None,
        excel_sheet_info: None,
        recent_actions: Vec::new(),
        element_tree: None,
        content_map: Vec::new(),
        action_history_summary: None,
    });
    
    // Get the current active window information
    let active_window = crate::monitoring::active_windows::get_active_window();
    
    // Update app info if the active window has changed
    if app_state.current_app.as_deref() != Some(&active_window.app_name) {
        info!("Active application changed from {:?} to {}", app_state.current_app, active_window.app_name);
        
        // Only wait if there was a REAL app switch, not initial detection
        if app_state.current_app.is_some() {
            // Give the accessibility tree time to stabilize after app switch
            info!("Waiting for accessibility tree to stabilize after app switch...");
            tokio::time::sleep(Duration::from_millis(APP_SWITCH_DELAY_MS)).await;
        }
        
        app_state.current_app = Some(active_window.app_name.clone());
        app_state.current_pid = Some(active_window.process_id.to_string());
    }
    
    // IMPORTANT: Always update the accessible elements, even if the active app hasn't changed
    // This ensures we have fresh UI elements after every action
    if app_state.current_app.is_some() && app_state.current_pid.is_some() {
        let pid = app_state.current_pid.as_ref().unwrap();
        let app_name = app_state.current_app.as_ref().unwrap();
        
        // Check if this is a web browser
        let _is_web_browser = app_name.contains("Chrome") || app_name.contains("Safari") || 
                             app_name.contains("Firefox") || app_name.contains("Edge");
        
        // Get elements based on platform
        #[cfg(target_os = "windows")]
        let (actionable_elements, text_content_string, full_text_content_string) = {

            use crate::window_details_collector::windows::windows_nvda_bridge;
            info!("🔄 UPDATE_APP_STATE: Calling NVDA bridge for elements from pid {}", pid);
            log_and_file("INFO", &format!("Calling NVDA API /api/elements for pid {}", pid));
            let nvda_start = std::time::Instant::now();
            let elements = windows_nvda_bridge::get_actionable_elements_by_pid_async(pid).await;
            let nvda_elapsed = nvda_start.elapsed();
            info!("🔄 UPDATE_APP_STATE: NVDA bridge returned {} elements in {:.2?}", elements.0.len(), nvda_elapsed);
            log_and_file("INFO", &format!("NVDA API response: {} elements retrieved in {:.2?}", elements.0.len(), nvda_elapsed));
            // Convert to JSON format for Windows
            let converted_elements: Vec<serde_json::Value> = 
                elements.0.into_iter().map(|e| {
                    serde_json::json!({
                        "element_ref": e.element_ref,
                        "element_type": e.element_type,
                        "element_name": e.element_name,
                        "element_value": e.element_value,
                        "element_description": e.element_description,
                        "element_path": e.element_path,
                        "is_focused": e.is_focused,
                    })
                }).collect();
            info!("NVDA returned {} elements for {}", converted_elements.len(), app_name);
            
            // For Windows, use same text for both truncated and full (for now)
            (converted_elements, elements.1.clone(), elements.1)
        };
        
        #[cfg(target_os = "macos")]
        let (mut actionable_elements, mut text_content_string, mut full_text_content_string) = 
            crate::window_details_collector::macos::macos_action_detector_engine::get_actionable_elements_by_pid(pid);
        
        // Only do refresh logic for macOS
        #[cfg(target_os = "macos")]
        {
            // Check if element count is suspiciously low  
            let is_web_browser = app_name.contains("Chrome") || app_name.contains("Safari") || 
                                app_name.contains("Firefox") || app_name.contains("Edge");
            let needs_refresh = if is_web_browser {
                actionable_elements.len() < MIN_ELEMENTS_WEB_BROWSER
            } else {
                actionable_elements.len() < MIN_ELEMENTS_DESKTOP_APP
            };
            
            if needs_refresh {
                let refresh_reason = format!("low element count ({} elements)", actionable_elements.len());
                warn!("Element refresh triggered by: {} for {}", refresh_reason, app_name);
                log_and_file("WARN", &format!("Element refresh triggered by: {}", refresh_reason));
                
                // Update metrics
                {
                    let mut metrics = REFRESH_METRICS.lock().unwrap();
                    metrics.low_count_refreshes += 1;
                }
                
                // Force accessibility refresh for macOS only
                if let Err(e) = force_accessibility_refresh(app_name, pid).await {
                    warn!("Failed to refresh accessibility tree: {}", e);
                }
                
                // Get elements again after refresh for macOS
                let (new_elements, new_text, new_full_text) = 
                    crate::window_details_collector::macos::macos_action_detector_engine::get_actionable_elements_by_pid(pid);
                
                if new_elements.len() > actionable_elements.len() {
                    info!("Accessibility refresh successful: {} -> {} elements", 
                          actionable_elements.len(), new_elements.len());
                    log_and_file("INFO", &format!("Accessibility refresh improved element count: {} -> {}", 
                          actionable_elements.len(), new_elements.len()));
                    actionable_elements = new_elements;
                    text_content_string = new_text;
                    full_text_content_string = new_full_text;
                    
                    // Track successful refresh
                    {
                        let mut metrics = REFRESH_METRICS.lock().unwrap();
                        metrics.successful_refreshes += 1;
                    }
                } else {
                    warn!("Accessibility refresh did not improve element count: {} elements", new_elements.len());
                    
                    // Track failed refresh
                    {
                        let mut metrics = REFRESH_METRICS.lock().unwrap();
                        metrics.failed_refreshes += 1;
                    }
                }
            }
        }
        
        // Build element tree text from the elements we already have (no need for second API call)
        #[cfg(target_os = "windows")]
        let (_, element_tree) = {
            log_and_file("INFO", "Building element tree from cached elements (no additional API call)");
            let mut tree = String::new();
            for elem in &actionable_elements {
                if let (Some(elem_type), Some(elem_name)) = 
                    (elem.get("element_type").and_then(|v| v.as_str()),
                     elem.get("element_name").and_then(|v| v.as_str())) {
                    let elem_value = elem.get("element_value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    tree.push_str(&format!("[{}] {} - {}\n", elem_type, elem_name, elem_value));
                }
            }
            log_and_file("INFO", &format!("Element tree built: {} chars from {} elements", tree.len(), actionable_elements.len()));
            ("".to_string(), tree)
        };
        
        #[cfg(target_os = "macos")]
        let (_, element_tree) = 
            crate::window_details_collector::window_details_collector::get_element_tree_by_window_app_name(pid);
        
        // DETAILED LOGGING: Show element processing details
        log_and_file("INFO", "===== APP STATE UPDATE =====");
        log_and_file("INFO", &format!("PID: {}, App: {}", pid, app_state.current_app.as_deref().unwrap_or("Unknown")));
        log_and_file("INFO", &format!("Raw actionable elements found: {}", actionable_elements.len()));
        log_and_file("INFO", &format!("Element tree size: {} characters", element_tree.len()));
        
        // Debug print the first 100 characters of the element tree
        let element_tree_preview = if element_tree.len() > 100 {
            let mut char_boundary = 100;
            while !element_tree.is_char_boundary(char_boundary) && char_boundary > 0 {
                char_boundary -= 1;
            }
            format!("{}...", &element_tree[..char_boundary])
        } else {
            element_tree.clone()
        };
        info!("Element tree preview (100 chars): {}", element_tree_preview);
        
        // Limit element tree for LLM to 1000 characters to control token usage
        let element_tree_for_llm = if element_tree.len() > 1000 {
            let mut char_boundary = 1000;
            while !element_tree.is_char_boundary(char_boundary) && char_boundary > 0 {
                char_boundary -= 1;
            }
            element_tree[..char_boundary].to_string()
        } else {
            element_tree
        };
        
        // Store the truncated element tree for LLM context
        app_state.element_tree = Some(element_tree_for_llm);
        
        // Store both truncated and full text content
        app_state.text_content = if text_content_string.is_empty() {
            None
        } else {
            Some(text_content_string)
        };
        
        app_state.full_text_content = if full_text_content_string.is_empty() {
            None
        } else {
            Some(full_text_content_string)
        };

        // Auto-fetch Word document info (cursor + structure) when Microsoft Word is active
        #[cfg(target_os = "macos")]
        {
            let app_lower = app_name.to_lowercase();
            if app_lower.contains("microsoft word") || app_lower.contains("word") {
                let word_start = std::time::Instant::now();
                match crate::engine::app_commands::word_macos::execute_word_command("WORD_GET_DOCUMENT_INFO", &[]).await {
                    Ok(doc_info) => {
                        info!("Auto-fetched Word document info ({} chars) in {:.2?}", doc_info.len(), word_start.elapsed());
                        app_state.word_document_info = Some(doc_info);
                    }
                    Err(e) => {
                        warn!("Failed to auto-fetch Word document info: {}", e);
                        app_state.word_document_info = None;
                    }
                }
            } else {
                app_state.word_document_info = None;
            }
        }

        // Extract screen state information before consuming actionable_elements
        let element_count = actionable_elements.len();

        let key_elements = extract_key_elements(&actionable_elements);
        let content_indicators = extract_content_indicators(&actionable_elements);
        let navigation_available = extract_navigation_options(&actionable_elements);
        
        // Convert elements to a simplified representation for the LLM
        let mut element_descriptions = Vec::new();
        
        // IMPORTANT: Also store the original elements in a global cache for direct access by index
        let mut element_cache = Vec::new();

        // Track AXTextArea elements to limit their value length after 30
        let mut textarea_count = 0;

        // For macOS, track and remove duplicates
        #[cfg(target_os = "macos")]
        let mut seen_elements = HashSet::new();
        #[cfg(target_os = "macos")]
        let mut duplicate_count = 0;

        // Track the current group context to insert headers when it changes
        let mut current_group_context = String::new();

        // Create a mapping from display numbers to cache indices
        let mut display_mapping: Vec<usize> = Vec::new();

        // Track previous element's description to avoid repeating text
        let mut previous_element_description = String::new();

        // Track last heading to avoid consecutive duplicates
        let mut last_heading = String::new();

        for element in actionable_elements.iter() {
            // Check if this is a heading marker element
            #[cfg(target_os = "macos")]
            {
                if element.role == "HEADING_MARKER" {
                    // Add the heading directly without numbering
                    let heading_text = if !element.title.is_empty() {
                        &element.title
                    } else if !element.description.is_empty() {
                        &element.description
                    } else if !element.value.is_empty() {
                        &element.value
                    } else {
                        "Unknown Heading"
                    };

                    // Skip if this heading is the same as the last one
                    if heading_text != last_heading {
                        element_descriptions.push(format!("─── HEADING: {} ───", heading_text));
                        last_heading = heading_text.to_string();
                    }

                    // Don't add to element_cache as it's not clickable
                    // Don't increment cache_index
                    continue;
                }
            }

            // macOS-specific duplicate detection - TEMPORARILY DISABLED
            #[cfg(target_os = "macos")]
            {
                // Create a duplicate check key based on Type + Value + Description + Title
                let duplicate_key = format!("{}|{}|{}|{}",
                    element.role, element.value, element.description, element.title);

                if seen_elements.contains(&duplicate_key) {
                    duplicate_count += 1;
                    continue;
                }
                seen_elements.insert(duplicate_key);
            }

            // Check if we have a new group context and add a header
            #[cfg(target_os = "macos")]
            {
                if !element.group_context.is_empty() && element.group_context != current_group_context {
                    // Skip if this heading is the same as the last one
                    if element.group_context != last_heading {
                        // Add a heading element (not clickable, just for context)
                        element_descriptions.push(format!("─── HEADING: {} ───", element.group_context));
                        last_heading = element.group_context.clone();
                    }
                    // Note: We don't add this to element_cache as it's not a real clickable element
                    current_group_context = element.group_context.clone();
                }
            }
            
            // Build element description
            let mut parts = Vec::new();
            

            #[cfg(target_os = "macos")]
            {
                // Special handling for marker elements with simplified format
                if element.role == "STATIC_TEXT_MARKER" {
                    // For static text, use simplified format: T:Text | <content>
                    let text_content = if !element.value.is_empty() {
                        &element.value
                    } else if !element.title.is_empty() {
                        &element.title
                    } else {
                        &element.description
                    };

                    if !text_content.is_empty() {
                        // Skip if this text is the same as the previous element's description
                        // This handles cases like: "AXLink | D:Newton Apartments" followed by "Text | Newton Apartments"
                        if text_content == &previous_element_description {
                            continue;
                        }

                        // Truncate to reasonable length
                        let text = if text_content.len() > 100 {
                            let truncated: String = text_content.chars().take(100).collect();
                            format!("{}...", truncated)
                        } else {
                            text_content.clone()
                        };
                        parts.push("T:Text".to_string());
                        parts.push(text);
                    }
                } else if element.role == "HEADING_MARKER" {
                    // For headings, use simplified format: T:Heading | <content>
                    let text_content = if !element.value.is_empty() {
                        &element.value
                    } else if !element.title.is_empty() {
                        &element.title
                    } else {
                        &element.description
                    };

                    if !text_content.is_empty() {
                        // Truncate to reasonable length
                        let text = if text_content.len() > 100 {
                            let truncated: String = text_content.chars().take(100).collect();
                            format!("{}...", truncated)
                        } else {
                            text_content.clone()
                        };
                        parts.push("T:Heading".to_string());
                        parts.push(text);
                    }
                } else {
                    // Regular elements keep their role
                    parts.push(format!("T:{}", element.role));
                    // Check if this is an AXTextArea element
                    let is_textarea = element.role == "AXTextArea";
                    if is_textarea {
                        textarea_count += 1;
                    }

                    if !element.value.is_empty() {
                        // For AXTextArea elements after the first 30, limit value to 50 chars
                        let max_value_length = if is_textarea && textarea_count > 30 {
                            50
                        } else {
                            200
                        };

                        // Truncate value to appropriate length
                        let value = if element.value.len() > max_value_length {
                            let mut char_boundary = max_value_length;
                            while !element.value.is_char_boundary(char_boundary) && char_boundary > 0 {
                                char_boundary -= 1;
                            }
                            format!("{}...", &element.value[..char_boundary])
                        } else {
                            element.value.clone()
                        };
                        parts.push(format!("V:{}", value));
                    }

                    if !element.title.is_empty() {
                        // Truncate title to reasonable length (reduced from 200)
                        let title = if element.title.len() > 100 {
                            // Use char-aware truncation to avoid panicking on multi-byte UTF-8 characters
                            let truncated: String = element.title.chars().take(100).collect();
                            format!("{}...", truncated)
                        } else {
                            element.title.clone()
                        };
                        parts.push(format!("L:{}", title));
                    }

                    if !element.description.is_empty() {
                        // Truncate description to reasonable length
                        let description = if element.description.len() > 100 {
                            // Use char-aware truncation to avoid panicking on multi-byte UTF-8 characters
                            let truncated: String = element.description.chars().take(100).collect();
                            format!("{}...", truncated)
                        } else {
                            element.description.clone()
                        };
                        parts.push(format!("D:{}", description));
                    }
                }

                // Add sibling context if available (nearby static text that provides context)
                // Skip if context is the same as description or title (redundant)
                if !element.sibling_context.is_empty() {
                    let is_redundant = element.sibling_context == element.description
                        || element.sibling_context == element.title;

                    if !is_redundant {
                        let context = if element.sibling_context.len() > 100 {
                            let truncated: String = element.sibling_context.chars().take(100).collect();
                            format!("{}...", truncated)
                        } else {
                            element.sibling_context.clone()
                        };
                        parts.push(format!("C:{}", context));
                    }
                }
            }
            
            #[cfg(not(target_os = "macos"))]
            {
                parts.push(format!("T:{}", element.get("element_type").and_then(|v| v.as_str()).unwrap_or("")));
                
                if let Some(value_str) = element.get("element_value").and_then(|v| v.as_str()) {
                    if !value_str.is_empty() {
                        let value = if value_str.len() > 200 {
                            // Use char-aware truncation to avoid panicking on multi-byte UTF-8 characters
                            let truncated: String = value_str.chars().take(200).collect();
                            format!("{}...", truncated)
                        } else {
                            value_str.to_string()
                        };
                        parts.push(format!("V:{}", value));
                    }
                }
                
                if let Some(title_str) = element.get("element_name").and_then(|v| v.as_str()) {
                    if !title_str.is_empty() {
                        let title = if title_str.len() > 100 {
                            // Use char-aware truncation to avoid panicking on multi-byte UTF-8 characters
                            let truncated: String = title_str.chars().take(100).collect();
                            format!("{}...", truncated)
                        } else {
                            title_str.to_string()
                        };
                        parts.push(format!("N:{}", title));
                    }
                }
                
                if let Some(desc_str) = element.get("element_description").and_then(|v| v.as_str()) {
                    if !desc_str.is_empty() {
                        let description = if desc_str.len() > 200 {
                            // Use char-aware truncation to avoid panicking on multi-byte UTF-8 characters
                            let truncated: String = desc_str.chars().take(200).collect();
                            format!("{}...", truncated)
                        } else {
                            desc_str.to_string()
                        };
                        parts.push(format!("D:{}", description));
                    }
                }
            }
            
            let description = parts.join(" | ");

            // Store both the description for the LLM and the actual element details
            element_descriptions.push(description.clone());

            // Track previous element's description for deduplication
            // Store the raw description text (without role prefix) for comparison
            #[cfg(target_os = "macos")]
            {
                if !element.description.is_empty() {
                    previous_element_description = element.description.clone();
                } else if !element.title.is_empty() {
                    previous_element_description = element.title.clone();
                } else if !element.value.is_empty() {
                    previous_element_description = element.value.clone();
                } else {
                    previous_element_description.clear();
                }
            }

            // Track the mapping: this element will be at index element_cache.len()
            display_mapping.push(element_cache.len());
            element_cache.push(element.clone());
        }

        app_state.accessible_elements = element_descriptions;

        // Store the display to cache mapping
        #[cfg(target_os = "macos")]
        {
            let mut map = DISPLAY_TO_CACHE_MAP.lock().unwrap();
            *map = display_mapping;
        }

        // Log AXTextArea optimization if applicable
        if textarea_count > 30 {
            log_and_file("INFO", &format!("🔧 AXTextArea optimization active: {} textarea elements found", textarea_count));
            log_and_file("INFO", &format!("   → First 30 textareas: values truncated to 200 chars"));
            log_and_file("INFO", &format!("   → Remaining {} textareas: values truncated to 50 chars", textarea_count - 30));
        }

        // Summary logging
        let cache_len = element_cache.len();
        log_and_file("INFO", &format!("Processed {} actionable elements for LLM context", app_state.accessible_elements.len()));
        
        // Log duplicate removal for macOS
        #[cfg(target_os = "macos")]
        if duplicate_count > 0 {
            log_and_file("INFO", &format!("Removed {} duplicate elements (same Type+Value+Description+Title)", duplicate_count));
        }
        
        log_and_file("INFO", &format!("Extracted {} chars of text content from non-actionable elements", 
              app_state.text_content.as_ref().map_or(0, |t| t.len())));
        log_and_file("INFO", &format!("Cached {} raw actionable elements for index-based access", cache_len));
        
        // Store the element cache in a global variable for retrieval by index
        update_element_cache(element_cache);
        
        // Create and store screen state
        let screen_state = ScreenState {
            app_name: app_state.current_app.clone().unwrap_or_default(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            element_count,
            key_elements,
            content_indicators,
            navigation_available,
        };
        
        // Add to content map
        app_state.content_map.push(screen_state.clone());
        
        // Log screen state summary
        info!("Screen state captured: {} elements, indicators: {:?}, navigation: {:?}",
            screen_state.element_count,
            screen_state.content_indicators,
            screen_state.navigation_available
        );
    }
    
    // Update the global state (get and set, then drop the lock)
    {
        let mut state = APP_STATE.lock().unwrap();
        *state = Some(app_state.clone());
    }
    
    let update_elapsed = update_start.elapsed();
    info!("🔄 UPDATE_APP_STATE: Completed in {:.2?} - {} elements found", update_elapsed, app_state.accessible_elements.len());
    
    Ok(app_state)
}

/// Force the browser to refresh its accessibility tree
#[cfg(target_os = "macos")]
async fn force_accessibility_refresh(app_name: &str, pid: &str) -> Result<(), String> {
    info!("Forcing accessibility tree refresh for {}", app_name);
    
    // Ensure app is in foreground (this also focuses it)
    let _ = crate::chrome_automation::launch_app(app_name);
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Perform soft reload to refresh the accessibility tree
    info!("Performing soft reload to refresh accessibility tree");
    let _ = crate::window_details_collector::macos::macos_acting_engine::press_key_in_app(
        app_name, "cmd+r"
    ).await;
    
    // Give the page some time to reload and JS to run
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    let (after_reload_elements, _, _) = crate::window_details_collector::macos::macos_action_detector_engine::get_actionable_elements_by_pid(pid);
    info!("Post-reload element count: {} elements", after_reload_elements.len());
    
    Ok(())
}

/// Helper to update the element cache
#[cfg(target_os = "macos")]
fn update_element_cache(elements: Vec<crate::window_details_collector::macos::macos_action_detector_engine::ActionableElement>) {
    let mut cache = ELEMENT_CACHE.lock().unwrap();
    *cache = elements;
}

#[cfg(target_os = "windows")]
fn update_element_cache(elements: Vec<serde_json::Value>) {
    let mut cache = ELEMENT_CACHE.lock().unwrap();
    *cache = elements;
}

/// Helper to get an element by index
#[cfg(target_os = "macos")]
fn get_element_by_index(index: usize) -> Option<crate::window_details_collector::macos::macos_action_detector_engine::ActionableElement> {
    // First check the display mapping to get the actual cache index
    let mapping = DISPLAY_TO_CACHE_MAP.lock().unwrap();
    let cache = ELEMENT_CACHE.lock().unwrap();

    // The index from LLM is 1-based display number
    if index > 0 && index <= mapping.len() {
        // Get the actual cache index from the mapping
        let cache_index = mapping[index - 1];

        if cache_index < cache.len() {
            Some(cache[cache_index].clone())
        } else {
            warn!("Mapping error: display index {} maps to cache index {}, but cache has only {} elements",
                index, cache_index, cache.len());
            None
        }
    } else {
        warn!("Invalid element index {} - display has {} clickable elements", index, mapping.len());
        None
    }
}

#[cfg(target_os = "windows")]
fn get_element_by_index(index: usize) -> Option<serde_json::Value> {
    let cache = ELEMENT_CACHE.lock().unwrap();
    let cache_size = cache.len();
    
    if index > 0 && index <= cache_size {
        Some(cache[index - 1].clone())
    } else {
        warn!("Invalid element index {} - cache has {} elements", index, cache_size);
        None
    }
}

/// Extract key elements from the element list (buttons, links, important text)
#[cfg(target_os = "macos")]
fn extract_key_elements(elements: &[crate::window_details_collector::macos::macos_action_detector_engine::ActionableElement]) -> Vec<String> {
    let mut key_elements = Vec::new();
    
    for element in elements.iter().take(10) {
        // Focus on interactive elements and important text
        if element.role == "AXButton" || element.role == "AXLink" || 
           element.role == "AXMenuItem" || element.role == "AXTextField" {
            let mut desc = format!("{}", element.role);
            if !element.title.is_empty() {
                desc.push_str(&format!(": {}", element.title));
            } else if !element.description.is_empty() {
                desc.push_str(&format!(": {}", element.description));
            }
            key_elements.push(desc);
        }
    }
    
    key_elements
}

/// Extract content indicators (pagination, counts, etc.)
#[cfg(target_os = "macos")]
fn extract_content_indicators(elements: &[crate::window_details_collector::macos::macos_action_detector_engine::ActionableElement]) -> Vec<String> {
    let mut indicators = Vec::new();
    
    for element in elements {
        let text = format!("{} {} {}", element.title, element.value, element.description);
        
        // Look for pagination patterns
        if text.contains("Page ") && text.contains(" of ") {
            indicators.push(text.clone());
        }
        // Look for item counts
        else if text.contains("Showing ") && (text.contains(" of ") || text.contains(" - ")) {
            indicators.push(text.clone());
        }
        // Look for numbered items
        else if text.contains("1 of ") || text.contains("1-") {
            indicators.push(text.clone());
        }
        // Look for result counts
        else if text.contains(" results") || text.contains(" items") || text.contains(" contacts") {
            indicators.push(text.clone());
        }
    }
    
    indicators
}

/// Extract navigation options (Next, Previous, Load more, etc.)
#[cfg(target_os = "macos")]
fn extract_navigation_options(elements: &[crate::window_details_collector::macos::macos_action_detector_engine::ActionableElement]) -> Vec<String> {
    let mut nav_options = Vec::new();
    
    for element in elements {
        if element.role == "AXButton" || element.role == "AXLink" {
            let text = format!("{} {}", element.title, element.description).to_lowercase();
            
            if text.contains("next") || text.contains("previous") || 
               text.contains("load more") || text.contains("show more") ||
               text.contains("view more") || text.contains("see more") ||
               text.contains("older") || text.contains("newer") ||
               text.contains(">>") || text.contains("<<") {
                nav_options.push(format!("{}: {}", element.role, element.title));
            }
        }
    }
    
    // Check for scroll indicators
    for element in elements {
        if element.role == "AXScrollBar" {
            nav_options.push("Scrollable content detected".to_string());
            break;
        }
    }
    
    nav_options
}

#[cfg(target_os = "windows")]
fn extract_key_elements(elements: &[serde_json::Value]) -> Vec<String> {
    let mut key_elements = Vec::new();
    for element in elements.iter().take(10) {
        if let Some(name) = element.get("name").and_then(|v| v.as_str()) {
            key_elements.push(name.to_string());
        }
    }
    key_elements
}

#[cfg(target_os = "windows")]
fn extract_content_indicators(elements: &[serde_json::Value]) -> Vec<String> {
    let mut indicators = Vec::new();
    for element in elements {
        if let Some(name) = element.get("name").and_then(|v| v.as_str()) {
            let lower_name = name.to_lowercase();
            if lower_name.contains("page") || lower_name.contains("result") ||
               lower_name.contains("item") || lower_name.contains("showing") {
                indicators.push(name.to_string());
            }
        }
    }
    indicators
}

#[cfg(target_os = "windows")]
fn extract_navigation_options(elements: &[serde_json::Value]) -> Vec<String> {
    let mut nav_options = Vec::new();
    for element in elements {
        if let Some(name) = element.get("name").and_then(|v| v.as_str()) {
            let lower_name = name.to_lowercase();
            if lower_name.contains("next") || lower_name.contains("previous") ||
               lower_name.contains("load more") || lower_name.contains("show more") {
                nav_options.push(name.to_string());
            }
        }
    }
    nav_options
}

/// Create a screen summary for action context
fn summarize_current_screen(app_state: &AppState) -> String {
    let app_name = app_state.current_app.as_deref().unwrap_or("Unknown");
    let element_count = app_state.accessible_elements.len();
    
    // Get the most recent screen state if available
    if let Some(last_screen) = app_state.content_map.last() {
        format!(
            "{} - {} elements, nav: {:?}, indicators: {:?}",
            app_name,
            element_count,
            last_screen.navigation_available.join(", "),
            last_screen.content_indicators.join(", ")
        )
    } else {
        format!("{} - {} elements", app_name, element_count)
    }
}

/// Decide the next action based on the current state
async fn decide_next_action(
    app_handle: &AppHandle,
    execution_state: &ExecutionState,
    app_state: &AppState,
    last_action_result: Option<&str>,
) -> Result<AgentAction, String> {
    // DETAILED LOGGING: Show decision context
    log_and_file("INFO", "===== AUTOMATION DECISION POINT =====");
    log_and_file("INFO", &format!("Automation: {} (ID: {})", execution_state.name, execution_state.automation_id));
    log_and_file("INFO", &format!("Current app: {}", app_state.current_app.as_deref().unwrap_or("None")));
    log_and_file("INFO", &format!("Available elements: {}", app_state.accessible_elements.len()));
    log_and_file("INFO", &format!("Recent actions: {}", app_state.recent_actions.len()));
    log_and_file("INFO", &format!("Element tree size: {} chars", app_state.element_tree.as_ref().map_or(0, |t| t.len())));
    
    // DEBUGGING: Show what recent actions the LLM will see
    // DEBUGGING: Show what recent actions the LLM will see
    info!("===== RECENT ACTIONS SENT TO LLM =====");
    if app_state.recent_actions.is_empty() {
        info!("No recent actions to send to LLM");
    } else {
        info!("LLM will see these {} recent actions:", app_state.recent_actions.len());
        for (i, action) in app_state.recent_actions.iter().enumerate() {
            info!("  {}. {} - {} ({})", 
                i + 1, 
                action.command, 
                action.justification,
                action.screen_context
            );
        }
    }
    info!("==========================================");
    
    // Get API choice setting
    let api_choice = app_handle
        .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
        .setting_value;

    // Get use_pro_model setting (only relevant for proxy)
    let use_pro_model = app_handle
        .db(|db| match get_setting(db, "use_pro_model") {
            Ok(setting) => setting.setting_value == "true",
            Err(_) => false,
        });

    // Get the appropriate API key based on the choice
    let (api_key, provider_name) = match api_choice.as_str() {
        "openai" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_open_ai").expect("Failed to get OpenAI API key"))
                .setting_value;
            if key.is_empty() {
                return Err("OpenAI API key is not configured".to_string());
            }
            (key, "OpenAI")
        },
        "grok" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_grok").expect("Failed to get Grok API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Grok API key is not configured".to_string());
            }
            (key, "Grok")
        },
        "deepseek" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_deepseek").expect("Failed to get DeepSeek API key"))
                .setting_value;
            if key.is_empty() {
                return Err("DeepSeek API key is not configured".to_string());
            }
            (key, "DeepSeek")
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
                
            (jwt_token, "Proxy")
        },
        "gemini" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_gemini").expect("Failed to get Gemini API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Gemini API key is not configured".to_string());
            }
            (key, "Gemini")
        },
        "claude-subscription" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_claude_oauth").map(|s| s.setting_value).unwrap_or_default());
            if key.is_empty() {
                return Err("Claude OAuth token is not configured — sign in via Settings.".to_string());
            }
            (key, "Claude (subscription)")
        },
        "openai-codex" => {
            let key = crate::auth::openai_codex_oauth::load(&app_handle)
                .map(|c| c.access)
                .unwrap_or_default();
            if key.is_empty() {
                return Err("ChatGPT (subscription) not signed in — sign in via Settings.".to_string());
            }
            (key, "ChatGPT (subscription)")
        },
        "claude" | _ => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_claude").expect("Failed to get Claude API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Claude API key is not configured".to_string());
            }
            (key, "Claude")
        }
    };

    info!("Using {} provider for LLM interaction", provider_name);
    
    // Check if we have an active session
    let has_session = LLM_SESSION.lock().unwrap().is_some();
    
    if has_session {
        // Use session-based approach with incremental updates
        info!("Using session-based LLM interaction");
        
        // Retrieve automation once to access the generalized script for context anchoring
        let _automation = app_handle
            .db(|db| automation_repository::get_automation_by_id(db, execution_state.automation_id))
            .map_err(|e| format!("Failed to get automation: {}", e))?
            .ok_or_else(|| format!("Automation with ID {} not found", execution_state.automation_id))?;

        // Create incremental prompt - no longer needs script or description
        let incremental_prompt = create_incremental_prompt(app_state, last_action_result);

        // Log the incremental update size
        info!("Incremental update size: {} characters", incremental_prompt.len());

        // Check if app-specific commands are available for logging
        let has_app_commands = app_state.current_app.as_ref()
            .map(|app| crate::engine::app_commands::has_app_specific_commands(app))
            .unwrap_or(false);

        // DETAILED LOGGING: Show what's being sent to the LLM
        log_and_file("INFO", &format!("===== LLM DECISION REQUEST ({}) =====", provider_name));
        log_and_file("INFO", &format!("Session-based approach: {}", has_session));
        log_and_file("INFO", &format!("App-specific commands available: {}", has_app_commands));
        log_and_file("INFO", &format!("Incremental prompt ({} chars):\n{}", incremental_prompt.len(), incremental_prompt));
        log_and_file("INFO", "=== END LLM REQUEST ===");
        
        // Send incremental update to the appropriate LLM provider
        let response = match api_choice.as_str() {
            "openai" => {
                // Clone the session data to avoid holding the lock across await
                let (session_data, session_exists) = {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    if let Some(session) = session_guard.as_mut() {
                        // Clone the session so we can modify it
                        (Some(session.clone()), true)
                    } else {
                        (None, false)
                    }
                };
                
                if !session_exists {
                    return Err("No LLM session available".to_string());
                }
                
                let mut session = session_data.unwrap();
                let (response_text, input_tokens, output_tokens) =
                    crate::engine::llm_providers::openai::call_llm_api_with_session(
                        &api_key, &mut session, incremental_prompt, 2000
                    ).await?;

                // Log token usage for this step
                log_token_usage(input_tokens, output_tokens, "DECISION_STEP");

                // Update the global session with the modified data
                {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    *session_guard = Some(session);
                }

                response_text
            },
            "grok" => {
                // Clone the session data to avoid holding the lock across await
                let (session_data, session_exists) = {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    if let Some(session) = session_guard.as_mut() {
                        // Clone the session so we can modify it
                        (Some(session.clone()), true)
                    } else {
                        (None, false)
                    }
                };
                
                if !session_exists {
                    return Err("No LLM session available".to_string());
                }
                
                let mut session = session_data.unwrap();
                let (response_text, input_tokens, output_tokens) =
                    crate::engine::llm_providers::grok::call_llm_api_with_session(
                        &api_key, &mut session, incremental_prompt, 2000
                    ).await?;

                // Log token usage for this step
                log_token_usage(input_tokens, output_tokens, "DECISION_STEP");

                // Update the global session with the modified data
                {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    *session_guard = Some(session);
                }

                response_text
            },
            "deepseek" => {
                let (session_data, session_exists) = {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    if let Some(session) = session_guard.as_mut() {
                        (Some(session.clone()), true)
                    } else {
                        (None, false)
                    }
                };
                
                if !session_exists {
                    return Err("No LLM session available".to_string());
                }
                
                let mut session = session_data.unwrap();
                let (response_text, input_tokens, output_tokens) =
                    crate::engine::llm_providers::deepseek::call_llm_api_with_session(
                        &api_key, &mut session, incremental_prompt, 2000
                    ).await?;

                log_token_usage(input_tokens, output_tokens, "DECISION_STEP");

                {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    *session_guard = Some(session);
                }

                response_text
            },
            "proxy" => {
                // Clone the session data to avoid holding the lock across await
                let (session_data, session_exists) = {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    if let Some(session) = session_guard.as_mut() {
                        // Clone the session so we can modify it
                        (Some(session.clone()), true)
                    } else {
                        (None, false)
                    }
                };
                
                if !session_exists {
                    return Err("No LLM session available".to_string());
                }
                
                let mut session = session_data.unwrap();
                let (response_text, input_tokens, output_tokens) =
                    crate::engine::llm_providers::proxy::call_llm_api_with_session(
                        &api_key, &mut session, incremental_prompt, 4000, use_pro_model,
                    ).await?;

                log_token_usage(input_tokens, output_tokens, "DECISION_STEP");
                
                {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    *session_guard = Some(session);
                }
                
                response_text
            },
            "gemini" => {
                // Clone the session data to avoid holding the lock across await
                let (session_data, session_exists) = {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    if let Some(session) = session_guard.as_mut() {
                        // Clone the session so we can modify it
                        (Some(session.clone()), true)
                    } else {
                        (None, false)
                    }
                };
                
                if !session_exists {
                    return Err("No LLM session available".to_string());
                }
                
                let mut session = session_data.unwrap();
                let (response_text, input_tokens, output_tokens) = 
                    crate::engine::llm_providers::gemini::call_llm_api_with_session(
                        &api_key, &mut session, incremental_prompt, 4000
                    ).await?;
                
                // Log token usage for this step
                log_token_usage(input_tokens, output_tokens, "DECISION_STEP");
                
                // Update the global session with the modified data
                {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    *session_guard = Some(session);
                }
                
                response_text
            },
            "openai-codex" => {
                let (session_data, session_exists) = {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    if let Some(session) = session_guard.as_mut() {
                        (Some(session.clone()), true)
                    } else {
                        (None, false)
                    }
                };
                if !session_exists {
                    return Err("No LLM session available".to_string());
                }
                let mut session = session_data.unwrap();
                let (response_text, input_tokens, output_tokens) =
                    crate::engine::llm_providers::openai_codex::call_llm_api_with_session(
                        &api_key, &mut session, incremental_prompt, 2000
                    ).await?;
                log_token_usage(input_tokens, output_tokens, "DECISION_STEP");
                {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    *session_guard = Some(session);
                }
                response_text
            },
            "claude" | "claude-subscription" | _ => {
                // Clone the session data to avoid holding the lock across await
                let (session_data, session_exists) = {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    if let Some(session) = session_guard.as_mut() {
                        // Clone the session so we can modify it
                        (Some(session.clone()), true)
                    } else {
                        (None, false)
                    }
                };

                if !session_exists {
                    return Err("No LLM session available".to_string());
                }

                let mut session = session_data.unwrap();
                let (response_text, input_tokens, output_tokens) =
                    crate::engine::llm_providers::claude::call_llm_api_with_session(
                        &api_key, &mut session, incremental_prompt, 2000
                    ).await?;

                // Log token usage for this step
                log_token_usage(input_tokens, output_tokens, "DECISION_STEP");

                // Update the global session with the modified data
                {
                    let mut session_guard = LLM_SESSION.lock().unwrap();
                    *session_guard = Some(session);
                }

                response_text
            }
        };
        
        // Log the LLM response
        log_and_file("INFO", &format!("===== LLM DECISION RESPONSE ({}) =====", provider_name));
        log_and_file("INFO", &format!("Response ({} chars): {}", response.len(), response));
        log_and_file("INFO", "=== END LLM RESPONSE ===");
        
        // Check for empty response before parsing
        if response.trim().is_empty() {
            warn!("LLM returned empty response - likely hit context limit or rate limit");
            log_and_file("WARN", "LLM returned empty response - waiting before retry");
            
            // Return a wait action to allow recovery
            return Ok(AgentAction {
                action_type: ActionType::WaitTime,
                app_name: None,
                target_element: None,
                parameters: Some(HashMap::from([("wait_time".to_string(), "3".to_string())])),
                reasoning: "LLM returned empty response - waiting before retry".to_string(),
            });
        }

        // CRITICAL: Check for PLAN command BEFORE parsing!
        // PLAN is a special command that returns control to the orchestrator
        // and should not go through regular action parsing
        if crate::engine::agent_mode_engine::is_plan_command(&response) {
            log_and_file("INFO", "Detected PLAN command - returning to orchestrator");
            return Ok(AgentAction {
                action_type: ActionType::WaitTime, // Dummy action type - won't be executed
                app_name: None,
                target_element: None,
                parameters: None,
                reasoning: response.clone(), // Store the full PLAN command
            });
        }

        // Parse the agent action from the LLM response
        match parse_agent_action(&response) {
            Ok(action) => {
                // Log the parsed action with reasoning
                log_and_file("INFO", &format!(
                    "Parsed action for automation {}: {:?} | Reasoning: {}",
                    execution_state.automation_id,
                    action.action_type,
                    action.reasoning
                ));

                // Accept COMPLETE actions without additional verification
                // The LLM already has instructions in its system prompt to be thorough
                if action.action_type == ActionType::Complete {
                    info!("LLM decided task is complete");
                }

                // Reset parse error counter on successful parse
                if let Some(session) = LLM_SESSION.lock().unwrap().as_mut() {
                    session.consecutive_parse_errors = 0;
                }

                return Ok(action);
            },
            Err(parse_error) => {
                // Log the parse error
                error!("Failed to parse LLM response: {}", parse_error);
                log_and_file("ERROR", &format!("Parse error: {}", parse_error));

                // Track consecutive parse errors
                let error_count = if let Some(session) = LLM_SESSION.lock().unwrap().as_mut() {
                    session.consecutive_parse_errors += 1;
                    session.consecutive_parse_errors
                } else {
                    1
                };

                // If we've had 3 consecutive parse errors, terminate
                if error_count >= 3 {
                    error!("Too many consecutive parse errors ({}), terminating automation", error_count);
                    return Err(format!("Automation terminated after {} consecutive parse errors. Last error: {}", error_count, parse_error));
                }

                // Return a ParseError action that will be handled in the main loop
                // This will cause the LLM to receive feedback about the error
                return Ok(AgentAction {
                    action_type: ActionType::ParseError,
                    app_name: None,
                    target_element: None,
                    parameters: Some(HashMap::from([
                        ("error".to_string(), parse_error.clone()),
                        ("response".to_string(), response),
                        ("error_count".to_string(), error_count.to_string()),
                    ])),
                    reasoning: format!("Parse error #{}: {}", error_count, parse_error),
                });
            }
        }
    }
    
    // Fallback to stateless approach if no session
    info!("Using stateless LLM interaction (fallback)");
    
    // First get the full automation to access the generalized_script
    let automation = app_handle
        .db(|db| automation_repository::get_automation_by_id(db, execution_state.automation_id))
        .map_err(|e| format!("Failed to get automation: {}", e))?
        .ok_or_else(|| format!("Automation with ID {} not found", execution_state.automation_id))?;
    
    // IMPORTANT: Log the generalized script and element tree to help with debugging
    info!("===== DECISION MAKING CONTEXT =====");
    info!("Automation: {} (ID: {})", automation.name, automation.id);
    
    // Log the generalized script (pretty print the JSON if possible)
    info!("Generalized Script:");
    match serde_json::from_str::<serde_json::Value>(&automation.generalized_script) {
        Ok(json_value) => {
            if let Ok(pretty) = serde_json::to_string_pretty(&json_value) {
                let script_lines: Vec<&str> = pretty.lines().collect();
                for (i, line) in script_lines.iter().enumerate() {
                    if i < 30 {  // Limit to first 30 lines to avoid excessive logging
                        info!("  {}", line);
                    } else if i == 30 {
                        info!("  ... [additional script lines omitted]");
                        break;
                    }
                }
            } else {
                info!("  {}", automation.generalized_script);
            }
        },
        Err(_) => {
            info!("  {}", automation.generalized_script);
        }
    }
    
    // Log the current UI elements (with numbers for reference)
    info!("Current UI Elements ({}): ", app_state.accessible_elements.len());
    for (i, element) in app_state.accessible_elements.iter().enumerate().take(50) {
        info!("  {}. {}", i + 1, element);
    }
    if app_state.accessible_elements.len() > 50 {
        info!("  ... [additional {} elements omitted]", app_state.accessible_elements.len() - 50);
    }
    
    // Log recent actions
    info!("Recent Actions:");
    for action in &app_state.recent_actions {
        info!("  {}: {} - {}", action.timestamp, action.command, action.justification);
    }
    info!("===================================");
    
    // Create the prompt for the LLM to decide the next action
    let prompt = create_action_decision_prompt(app_handle, execution_state, app_state, &automation);
    
    // Get system prompt with context from the automation
    // Use synthetic prompt for automations without a generalized script
    let trimmed_script = automation.generalized_script.trim();

    // Check if script is effectively empty (various formats)
    let is_empty_script = automation.generalized_script.is_empty() ||
                         trimmed_script.is_empty() ||
                         trimmed_script == "[]" ||
                         trimmed_script == "{}" ||
                         trimmed_script == "{ }" ||
                         trimmed_script == "[ ]" ||
                         trimmed_script == "null" ||
                         // Check if it's just an empty JSON object/array with possible whitespace
                         (trimmed_script.starts_with('{') && trimmed_script.ends_with('}') && trimmed_script.len() <= 10) ||
                         (trimmed_script.starts_with('[') && trimmed_script.ends_with(']') && trimmed_script.len() <= 10) ||
                         // Check for script with only empty SCRIPT action
                         trimmed_script.contains(r#""script":"{}"#) ||
                         trimmed_script.contains(r#""script":"[]"#) ||
                         trimmed_script.contains(r#""script":""#);

    // Get app-specific commands based on current app
    let app_specific_commands = app_state.current_app.as_ref()
        .and_then(|app| crate::engine::app_commands::get_app_specific_commands(app));

    let system_prompt = if is_empty_script {
        automation_agent_synthetic_prompt::get_synthetic_system_prompt_with_context(
            Some(&execution_state.objective),
            automation.nl_description.as_deref(),
            Some(&automation.name),
            None,  // No additional instructions in stateless mode
            app_specific_commands.as_deref(),
        )
    } else {
        automation_agent_prompt::get_system_prompt_with_context(
            Some(&execution_state.objective),
            Some(&automation.generalized_script),
            automation.nl_description.as_deref(),
            Some(&automation.name),
            None,  // No additional instructions in stateless mode
            app_specific_commands.as_deref(),
        )
    };
    
    // DETAILED LOGGING: Show what's being sent to the LLM (stateless approach)
    log_and_file("INFO", &format!("===== LLM DECISION REQUEST ({}) =====", provider_name));
    log_and_file("INFO", "Session-based approach: false (stateless fallback)");
    log_and_file("INFO", &format!("System prompt ({} chars):\n{}", system_prompt.len(), system_prompt));
    log_and_file("INFO", &format!("User prompt ({} chars):\n{}", prompt.len(), prompt));
    log_and_file("INFO", "=== END LLM REQUEST ===");
    
    // Send request to the appropriate provider
    let response = match api_choice.as_str() {
        "openai" => {
            let (response_text, input_tokens, output_tokens) = 
                crate::engine::llm_providers::openai::call_llm_api(
                    &api_key, prompt, &system_prompt, 2000
                ).await?;
            
            // Log token usage for stateless decision
            log_token_usage(input_tokens, output_tokens, "STATELESS_DECISION");
            response_text
        },
        "grok" => {
            let (response_text, input_tokens, output_tokens) = 
                crate::engine::llm_providers::grok::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000
                ).await?;
            
            // Log token usage for stateless decision
            log_token_usage(input_tokens, output_tokens, "STATELESS_DECISION");
            response_text
        },
        "deepseek" => {
            let (response_text, input_tokens, output_tokens) = 
                crate::engine::llm_providers::deepseek::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000
                ).await?;
            
            log_token_usage(input_tokens, output_tokens, "STATELESS_DECISION");
            response_text
        },
        "proxy" => {
            let (response_text, input_tokens, output_tokens) = 
                crate::engine::llm_providers::proxy::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000,
                ).await?;
            
            log_token_usage(input_tokens, output_tokens, "STATELESS_DECISION");
            response_text
        },
        "gemini" => {
            let (response_text, input_tokens, output_tokens) = 
                crate::engine::llm_providers::gemini::call_llm_api(
                    &api_key, prompt, &system_prompt, 2000
                ).await?;
            
            // Log token usage for stateless decision
            log_token_usage(input_tokens, output_tokens, "STATELESS_DECISION");
            response_text
        },
        "openai-codex" => {
            let (response_text, input_tokens, output_tokens) =
                crate::engine::llm_providers::openai_codex::call_llm_api(
                    &api_key, prompt, &system_prompt, 2000
                ).await?;
            log_token_usage(input_tokens, output_tokens, "STATELESS_DECISION");
            response_text
        },
        "claude" | "claude-subscription" | _ => {
            let (response_text, input_tokens, output_tokens) =
                crate::engine::llm_providers::claude::call_llm_api(
                    &api_key, prompt, &system_prompt, 2000
                ).await?;

            // Log token usage for stateless decision
            log_token_usage(input_tokens, output_tokens, "STATELESS_DECISION");
            response_text
        }
    };
    
    // Log the LLM response
    log_and_file("INFO", &format!("===== LLM DECISION RESPONSE ({}) =====", provider_name));
    log_and_file("INFO", &format!("Response ({} chars): {}", response.len(), response));
    log_and_file("INFO", "=== END LLM RESPONSE ===");
    
    // Parse the agent action from the LLM response
    let action = parse_agent_action(&response)?;
    
    // Log the parsed action with reasoning
    log_and_file("INFO", &format!(
        "Parsed action for automation {}: {:?} | Reasoning: {}", 
        execution_state.automation_id,
        action.action_type,
        action.reasoning
    ));
    
    // Accept COMPLETE actions without additional verification
    // The LLM already has instructions in its system prompt to be thorough
    if action.action_type == ActionType::Complete {
        info!("LLM decided task is complete (stateless mode)");
    }
    
    Ok(action)
}

// Removed create_completion_verification_prompt function - no longer needed

/// Execute a single action
/// Returns Ok(message) with a confirmation/result message on success
async fn execute_action(
    app_handle: &AppHandle,
    action: &AgentAction
) -> Result<String, String> {
    // Execute the action based on type
    match action.action_type {
        ActionType::LaunchApp => {
            if let Some(app_name) = &action.app_name {
                // Use platform-specific acting engine to launch the app
                #[cfg(target_os = "macos")]
                let result = crate::window_details_collector::macos::macos_acting_engine::launch_app_and_wait(app_name).await;
                
                #[cfg(target_os = "windows")]
                let result = crate::window_details_collector::windows::windows_action_engine::launch_app_and_wait(app_name).await;
                
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                let result = crate::chrome_automation::launch_app(app_name);
                
                if result.success {
                    // Don't update app state here - smart_wait_after_action handles it
                    Ok(format!("Launched {}", app_name))
                } else {
                    Err(result.message)
                }
            } else {
                Err("App name is required for LaunchApp action".to_string())
            }
        },
        ActionType::ClickElement => {
            if let (Some(app_name), Some(target_element)) = (&action.app_name, &action.target_element) {
                info!("Attempting to click element: {} in app: {}", target_element, app_name);
                
                // Only launch/focus app on macOS - Windows NVDA handles focus
                #[cfg(target_os = "macos")]
                {
                    let app_result = crate::chrome_automation::launch_app(app_name);
                    if !app_result.success {
                        return Err(format!("Failed to ensure app is running: {}", app_result.message));
                    }
                }
                
                // Use platform-specific acting engine
                #[cfg(target_os = "windows")]
                let result = {
                    use crate::window_details_collector::windows::windows_action_engine;
                    // On Windows, target_element should be the element ID
                    // Use async version for consistency with element retrieval
                    windows_action_engine::click_element_by_id_async("", target_element).await
                };
                
                #[cfg(target_os = "macos")]
                let result = crate::window_details_collector::macos::macos_acting_engine::find_and_click_element(
                    app_name, target_element
                ).await;
                
                if result.success {
                    Ok("Clicked".to_string())
                } else {
                    Err(result.message)
                }
            } else {
                Err("App name and target element are required for ClickElement action".to_string())
            }
        },
        ActionType::TypeText => {
            if let (Some(app_name), Some(target_element), Some(params)) = (&action.app_name, &action.target_element, &action.parameters) {
                if let Some(text) = params.get("text") {
                    // Check if this is multi-element typing (||| delimiter)
                    if target_element.contains("|||") {
                        // Multi-element typing: parse |||-separated elements and texts
                        let target_elements: Vec<&str> = target_element.split("|||").map(|s| s.trim()).collect();
                        let texts: Vec<&str> = text.split("|||").map(|s| s.trim()).collect();
                        
                        if target_elements.len() != texts.len() {
                            return Err(format!("Mismatch between number of elements ({}) and texts ({})", target_elements.len(), texts.len()));
                        }
                        
                        info!("Multi-element typing in app {}: {} element:text pairs", app_name, target_elements.len());
                        
                        // Only launch/focus app on macOS - Windows NVDA handles focus
                        #[cfg(target_os = "macos")]
                        {
                            let app_result = crate::chrome_automation::launch_app(app_name);
                            if !app_result.success {
                                return Err(format!("Failed to ensure app is running: {}", app_result.message));
                            }
                        }
                        
                        // Type into each element sequentially
                        for (i, (element, text_to_type)) in target_elements.iter().zip(texts.iter()).enumerate() {
                            info!("Typing step {}/{}: '{}' in element {}", i + 1, target_elements.len(), text_to_type, element);
                            
                            // Use platform-specific acting engine
                            #[cfg(target_os = "windows")]
                            let result = {
                                use crate::window_details_collector::windows::windows_action_engine;
                                windows_action_engine::type_text_in_element_by_id_async("", element, text_to_type).await
                            };
                            
                            #[cfg(target_os = "macos")]
                            let result = crate::window_details_collector::macos::macos_acting_engine::type_text_in_app(
                                app_name, text_to_type, Some(element)
                            ).await;
                            
                            if result.success {
                                // AUTO-ENTER FEATURE: Automatically press Enter after typing text
                                // This simulates user pressing Enter after typing in a field
                                if AUTO_ENTER_ENABLED {
                                    info!("Auto-pressing Enter after typing text in element {}", element);
                                    
                                    
                                    // Press Enter automatically
                                    #[cfg(target_os = "macos")]
                                    let enter_result = crate::window_details_collector::macos::macos_acting_engine::press_key_in_app(
                                        app_name, "enter"
                                    ).await;
                                    
                                    #[cfg(not(target_os = "macos"))]
                                    let enter_result = crate::chrome_automation::AppResult {
                                        success: true,
                                        message: "Auto-enter not implemented for Windows".to_string()
                                    };
                                    
                                    if enter_result.success {
                                        info!("Successfully auto-pressed Enter after typing in element {}", element);
                                        
                                        // Record the auto-enter action
                                        {
                                            let mut state_opt = APP_STATE.lock().unwrap();
                                            if let Some(mut state) = state_opt.clone() {
                                                let enhanced_action = EnhancedAction {
                                                    command: "PRESS:enter".to_string(),
                                                    action_type: "PressKey".to_string(),
                                                    justification: format!("Auto-enter after typing in {}", element),
                                                    screen_context: summarize_current_screen(&state),
                                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                                };
                                                state.recent_actions.push(enhanced_action);
                                                *state_opt = Some(state);
                                            }
                                        }
                                    } else {
                                        warn!("Failed to auto-press Enter in element {}: {}", element, enter_result.message);
                                    }
                                } else {
                                    info!("Auto-enter disabled, skipping automatic Enter press for element {}", element);
                                }
                                
                                // Short delay between typing operations to avoid overwhelming the UI
                                if i < target_elements.len() - 1 {
                                    tokio::time::sleep(Duration::from_millis(150)).await;
                                }
                            } else {
                                return Err(format!("Failed to type '{}' in element {}: {}", text_to_type, element, result.message));
                            }
                        }
                        
                        info!("Completed multi-element typing: {} elements processed", target_elements.len());
                        Ok(format!("Typed in {} elements", target_elements.len()))
                    } else {
                        // Single element typing (existing logic)
                        info!("Single-element typing in app {}: {} in element {}", app_name, text, target_element);
                        
                        // Log what we're about to send to NVDA for comparison with direct implementation
                        info!("🔍 DEBUG: Sending to NVDA - element_path: '{}', text: '{}'", target_element, text);
                        
                        // Use platform-specific acting engine
                        #[cfg(target_os = "windows")]
                        let result = {
                            use crate::window_details_collector::windows::windows_action_engine;
                            // Prefer async HTTP client to match element retrieval flow
                            let type_result = windows_action_engine::type_text_in_element_by_id_async("", target_element, text).await;
                            info!("📡 Type command result: success={}, message={}", type_result.success, type_result.message);
                            type_result
                        };
                        
                        #[cfg(target_os = "macos")]
                        let result = crate::window_details_collector::macos::macos_acting_engine::type_text_in_app(
                            app_name, text, Some(target_element.as_str())
                        ).await;
                        
                        if result.success {
                            // AUTO-ENTER FEATURE: Automatically press Enter after typing text
                            // This simulates user pressing Enter after typing in a field
                            if AUTO_ENTER_ENABLED {
                                info!("Auto-pressing Enter after typing text");
                                
                                // Smart wait based on content type instead of fixed delays
                                // Only treat as URL if it's actually a navigation URL, not an email
                                let is_url = (text.contains("http") || text.contains("www")) && !text.contains("@");
                                
                                if is_url {
                                    info!("Detected URL/navigation text, waiting before Enter");
                                    
                                    // Platform-specific wait strategy
                                    #[cfg(target_os = "windows")]
                                    {
                                        // On Windows with NVDA, just use a fixed delay - no need to poll
                                        info!("Windows/NVDA: Fixed wait for URL input (500ms)");
                                        tokio::time::sleep(Duration::from_millis(500)).await;
                                    }
                                    
                                    #[cfg(target_os = "macos")]
                                    {
                                        // On macOS, wait for DOM stability as the browser updates
                                        info!("macOS: Waiting for DOM stability before Enter");
                                        let config = WaitConfig {
                                            max_wait: Duration::from_secs(2),
                                            check_interval: Duration::from_millis(100),
                                        };
                                        let _ = wait_for_conditions(&[
                                            WaitCondition::DomStable { threshold_ms: 200 },
                                            WaitCondition::MinimumWait { duration: Duration::from_millis(300) },
                                        ], config, || Box::pin(update_app_state())).await?;
                                    }
                                } else {
                                    // For regular text, just ensure it's registered
                                    tokio::time::sleep(Duration::from_millis(200)).await;
                                }
                                
                                // Press Enter automatically
                                #[cfg(target_os = "macos")]
                                let enter_result = crate::window_details_collector::macos::macos_acting_engine::press_key_in_app(
                                    app_name, "enter"
                                ).await;
                                
                                #[cfg(not(target_os = "macos"))]
                                let enter_result = crate::chrome_automation::AppResult {
                                    success: true,
                                    message: "Auto-enter not implemented for Windows".to_string()
                                };
                                
                                if enter_result.success {
                                    info!("Successfully auto-pressed Enter after typing");
                                    
                                    // Record the auto-enter action
                                    {
                                        let mut state_opt = APP_STATE.lock().unwrap();
                                        if let Some(mut state) = state_opt.clone() {
                                            let enhanced_action = EnhancedAction {
                                                command: "PRESS:enter".to_string(),
                                                action_type: "PressKey".to_string(),
                                                justification: format!("Auto-enter after typing in {}", target_element),
                                                screen_context: summarize_current_screen(&state),
                                                timestamp: chrono::Utc::now().to_rfc3339(),
                                            };
                                            state.recent_actions.push(enhanced_action);
                                            *state_opt = Some(state);
                                        }
                                    }
                                } else {
                                    warn!("Failed to auto-press Enter: {}", enter_result.message);
                                }
                            } else {
                                info!("Auto-enter disabled, skipping automatic Enter press");
                            }
                            
                            Ok("Text typed".to_string())
                        } else {
                            Err(result.message)
                        }
                    }
                } else {
                    Err("Text parameter is required for TypeText action".to_string())
                }
            } else {
                Err("App name, target element, and parameters are required for TypeText action".to_string())
            }
        },
        ActionType::PressKey => {
            if let (Some(app_name), Some(params)) = (&action.app_name, &action.parameters) {
                if let Some(key) = params.get("key") {
                    info!("🔑 PRESS_KEY action starting: key='{}' in app '{}'", key, app_name);

                    // On Windows, don't launch the app - it should already be active
                    // On macOS, we still ensure the app is active
                    #[cfg(target_os = "macos")]
                    {
                        let app_result = crate::chrome_automation::launch_app(app_name);
                        if !app_result.success {
                            return Err(format!("Failed to ensure app is running: {}", app_result.message));
                        }
                    }

                    // Use the acting engine to press a key
                    #[cfg(target_os = "macos")]
                    let result = crate::window_details_collector::macos::macos_acting_engine::press_key_in_app(
                        app_name, key
                    ).await;

                    #[cfg(target_os = "windows")]
                    let result = {
                        use crate::window_details_collector::windows::windows_action_engine;
                        // Clone the key string to move it into the blocking task
                        let key_clone = key.to_string();
                        info!("🔑 Calling windows_action_engine::press_key with key '{}'", key_clone);
                        // Use tokio::task::spawn_blocking for the synchronous press_key function
                        tokio::task::spawn_blocking(move || {
                            windows_action_engine::press_key(&key_clone)
                        }).await
                        .unwrap_or_else(|e| crate::window_details_collector::windows::ActionResult::failure(&format!("Failed to spawn blocking task: {}", e)))
                    };

                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    let result = crate::chrome_automation::AppResult {
                        success: false,
                        message: "PressKey not implemented for this platform".to_string()
                    };

                    info!("🔑 PRESS_KEY result: success={}, message='{}'", result.success, result.message);

                    if result.success {
                        info!("🔑 PRESS_KEY succeeded for '{}'", key);
                        Ok(format!("Pressed {}", key))
                    } else {
                        error!("🔑 PRESS_KEY FAILED for '{}': {}", key, result.message);
                        Err(result.message)
                    }
                } else {
                    Err("Key parameter is required for PressKey action".to_string())
                }
            } else {
                Err("App name and parameters are required for PressKey action".to_string())
            }
        },
        ActionType::WaitTime => {
            if let Some(params) = &action.parameters {
                if let Some(wait_time) = params.get("wait_time") {
                    let seconds = wait_time.parse::<u64>().unwrap_or(1);
                    info!("Waiting for {} seconds", seconds);
                    tokio::time::sleep(Duration::from_secs(seconds)).await;
                    
                    Ok(format!("Waited {}s", seconds))
                } else {
                    // Default to 1 second if not specified
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    
                    Ok("Waited 1s".to_string())
                }
            } else {
                // Default to 1 second if not specified
                tokio::time::sleep(Duration::from_secs(1)).await;
                
                Ok("Waited 1s".to_string())
            }
        },
        ActionType::Complete => {
            info!("Marking automation as completed");

            // Get the current execution state and app state
            let (execution_state_opt, app_state_opt) = {
                let exec_guard = AUTOMATION_STATE.lock().unwrap();
                let app_guard = APP_STATE.lock().unwrap();
                (exec_guard.clone(), app_guard.clone())
            };

            // Generate completion message using LLM and capture data for extraction
            let (completion_message, extraction_context) = if let (Some(ref execution_state), Some(ref app_state)) = (&execution_state_opt, &app_state_opt) {
                // Get the automation to retrieve the objective
                let automation = app_handle
                    .db(|db| automation_repository::get_automation_by_id(db, execution_state.automation_id))
                    .map_err(|e| format!("Failed to get automation: {}", e))?
                    .ok_or_else(|| format!("Automation with ID {} not found", execution_state.automation_id))?;

                // Use objective override (continuation prompt) if set, otherwise use original objective
                let effective_objective = CURRENT_OBJECTIVE_OVERRIDE.lock().unwrap()
                    .clone()
                    .unwrap_or_else(|| automation.objective.clone());
                
                info!("Using objective for completion message: {}", &effective_objective);

                // Format recent actions (most recent first)
                let recent_actions = if app_state.recent_actions.is_empty() {
                    "No actions performed".to_string()
                } else {
                    app_state.recent_actions.iter().rev()
                        .enumerate()
                        .take(10) // Last 10 actions
                        .map(|(idx, act)| format!("{}. {} - {}", idx + 1, act.command, act.justification))
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                // Get memory content
                let memory = MEMORY_STORE.lock().unwrap().clone();

                // Create completion prompt using effective objective (continuation prompt if set)
                let completion_prompt = automation_completion_prompt::get_completion_prompt(
                    &effective_objective,
                    &memory,
                    &recent_actions,
                );

                info!("Generating completion message with LLM...");
                log_and_file("INFO", "Generating completion message");

                // Get API settings
                let api_choice = app_handle
                    .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
                    .setting_value;

                // Get the appropriate API key
                let api_key = match api_choice.as_str() {
                    "openai" => {
                        app_handle
                            .db(|db| get_setting(db, "api_key_open_ai").expect("Failed to get OpenAI API key"))
                            .setting_value
                    },
                    "grok" => {
                        app_handle
                            .db(|db| get_setting(db, "api_key_grok").expect("Failed to get Grok API key"))
                            .setting_value
                    },
                    "deepseek" => {
                        app_handle
                            .db(|db| get_setting(db, "api_key_deepseek").expect("Failed to get DeepSeek API key"))
                            .setting_value
                    },
                    "proxy" => {
                        let user_id = app_handle
                            .db(|db| match get_setting(db, "user_id") {
                                Ok(setting) => Some(setting.setting_value),
                                Err(_) => None,
                            })
                            .unwrap_or_default();

                        app_handle
                            .db(|db| crate::repository::user_auth_repository::get_valid_auth_token(db, &user_id))
                            .ok()
                            .flatten()
                            .unwrap_or_default()
                    },
                    "gemini" => {
                        app_handle
                            .db(|db| get_setting(db, "api_key_gemini").expect("Failed to get Gemini API key"))
                            .setting_value
                    },
                    "claude-subscription" => {
                        app_handle
                            .db(|db| get_setting(db, "api_key_claude_oauth").map(|s| s.setting_value).unwrap_or_default())
                    },
                    "openai-codex" => {
                        crate::auth::openai_codex_oauth::load(&app_handle)
                            .map(|c| c.access)
                            .unwrap_or_default()
                    },
                    "claude" | _ => {
                        app_handle
                            .db(|db| get_setting(db, "api_key_claude").expect("Failed to get Claude API key"))
                            .setting_value
                    }
                };

                // Make a simple one-shot LLM call (no session needed for completion)
                let completion_result = match api_choice.as_str() {
                    "openai" => {
                        crate::engine::llm_providers::openai::call_llm_api(
                            &api_key,
                            completion_prompt.clone(),
                            "",
                            1000,
                        ).await
                    },
                    "grok" => {
                        crate::engine::llm_providers::grok::call_llm_api(
                            &api_key,
                            completion_prompt.clone(),
                            "",
                            1000,
                        ).await
                    },
                    "deepseek" => {
                        crate::engine::llm_providers::deepseek::call_llm_api(
                            &api_key,
                            completion_prompt.clone(),
                            "",
                            1000,
                        ).await
                    },
                    "proxy" => {
                        crate::engine::llm_providers::proxy::call_llm_api(
                            &api_key,
                            completion_prompt.clone(),
                            "",
                            1000,
                        ).await
                    },
                    "gemini" => {
                        crate::engine::llm_providers::gemini::call_llm_api(
                            &api_key,
                            completion_prompt.clone(),
                            "",
                            1000,
                        ).await
                    },
                    "openai-codex" => {
                        crate::engine::llm_providers::openai_codex::call_llm_api(
                            &api_key,
                            completion_prompt.clone(),
                            "",
                            1000,
                        ).await
                    },
                    "claude" | "claude-subscription" | _ => {
                        crate::engine::llm_providers::claude::call_llm_api(
                            &api_key,
                            completion_prompt.clone(),
                            "",
                            1000,
                        ).await
                    }
                };

                let msg = match completion_result {
                    Ok((message, input_tokens, output_tokens)) => {
                        info!("Completion message generated: {} chars", message.len());
                        log_and_file("INFO", &format!("Completion message: {}", message));
                        log_token_usage(input_tokens, output_tokens, "COMPLETION");
                        message.trim().to_string()
                    },
                    Err(e) => {
                        warn!("Failed to generate completion message: {}", e);
                        // Fallback message
                        "Automation completed successfully.".to_string()
                    }
                };
                
                // Package context for data extraction (use effective objective for consistency)
                let ctx = Some((
                    execution_state.automation_id,
                    effective_objective.clone(),
                    memory.clone(),
                    recent_actions.clone(),
                    api_choice.clone(),
                    api_key.clone(),
                ));
                
                (msg, ctx)
            } else {
                ("Automation completed successfully.".to_string(), None)
            };

            // Mark the automation as completed and emit the completion message
            if let Some(mut execution_state) = execution_state_opt {
                execution_state.status = "Completed".to_string();
                execution_state.milestones.push("Automation completed".to_string());

                // Update the global state with milestone
                {
                    let mut state = AUTOMATION_STATE.lock().unwrap();
                    *state = Some(execution_state.clone());
                }

                // Save completion message to database
                let run_id_val = *CURRENT_EXECUTION_RUN_ID.lock().unwrap();
                if let Some(run_id) = run_id_val {
                    if let Err(e) = app_handle.db(|db| {
                        crate::repository::automation_execution_repository::update_execution_run_completion_message(
                            db,
                            run_id,
                            &completion_message,
                        )
                    }) {
                        warn!("Failed to save completion message to database: {}", e);
                    } else {
                        info!("Saved completion message to database for run {}", run_id);
                    }
                    
                    // Extract and store structured data from memory (if enabled in settings)
                    let store_task_data = app_handle
                        .db(|db| match get_setting(db, "store_task_data") {
                            Ok(setting) => setting.setting_value != "false", // Default to true
                            Err(_) => true, // Default to true if setting doesn't exist
                        });
                    
                    if store_task_data {
                        if let Some((automation_id, objective, memory, recent_actions, api_choice, api_key)) = extraction_context.clone() {
                            match extract_and_store_task_data(
                                app_handle,
                                automation_id,
                                run_id,
                                &objective,
                                &memory,
                                &recent_actions,
                                &api_choice,
                                &api_key,
                            ).await {
                                Ok(ids) => {
                                    if !ids.is_empty() {
                                        info!("Successfully extracted and stored {} data records", ids.len());
                                    }
                                },
                                Err(e) => {
                                    warn!("Data extraction failed (non-fatal): {}", e);
                                }
                            }
                        }
                    } else {
                        info!("Data extraction skipped (disabled in settings)");
                    }
                }

                // Emit completion message to frontend
                app_handle.emit("automation_completion_message", serde_json::json!({
                    "message": completion_message,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                })).map_err(|e| format!("Failed to emit completion message: {}", e))?;
            }

            Ok("Automation completed".to_string())
        },
        ActionType::Stuck => {
            // LLM signaled it's stuck and needs replanning
            info!("🚨 LLM signaled STUCK - triggering replan");
            log_and_file("INFO", &format!("STUCK command received: {}", action.reasoning));
            
            // Get recent actions for context
            let recent_action_strings: Vec<String> = {
                let state_guard = APP_STATE.lock().unwrap();
                if let Some(ref state) = *state_guard {
                    state.recent_actions.iter()
                        .rev()
                        .take(15)
                        .map(|a| format!("{}: {}", a.action_type, a.command))
                        .collect()
                } else {
                    Vec::new()
                }
            };
            
            // Get current automation data
            let execution_state = AUTOMATION_STATE.lock().unwrap().clone();
            
            if let Some(ref exec_state) = execution_state {
                let automation = app_handle
                    .db(|db| automation_repository::get_automation_by_id(db, exec_state.automation_id))
                    .map_err(|e| format!("Failed to get automation: {}", e))?
                    .ok_or_else(|| format!("Automation with ID {} not found", exec_state.automation_id))?;
                
                let current_step = STEP_NUMBER.load(std::sync::atomic::Ordering::SeqCst);
                
                match trigger_replan(app_handle, &automation, current_step, &recent_action_strings, &action.reasoning).await {
                    Ok(new_plan) => {
                        info!("✅ Replan successful, reinitializing LLM session with new plan");
                        
                        // Reinitialize LLM session with new plan
                        let system_prompt = automation_agent_synthetic_prompt::get_synthetic_system_prompt_with_context(
                            Some(&automation.objective),
                            Some(&new_plan),
                            Some(&automation.name),
                            None,
                            None,
                        );
                        
                        // Get current session ID or create new one
                        let session_id = {
                            let session_guard = LLM_SESSION.lock().unwrap();
                            session_guard.as_ref().map(|s| s.session_id.clone()).unwrap_or_else(|| "replan".to_string())
                        };
                        
                        let new_session = LLMSession::new(
                            session_id,
                            system_prompt,
                            format!("Previous approach wasn't working. New plan:\n{}", new_plan),
                        );
                        *LLM_SESSION.lock().unwrap() = Some(new_session);
                        
                        info!("LLM session reinitialized with replanned strategy");
                        log_and_file("INFO", "Replan successful - continuing with new approach");
                        
                        Ok(format!("Replanned successfully. New approach: {}", new_plan.lines().next().unwrap_or("(new plan)")))
                    },
                    Err(e) => {
                        warn!("Replan failed: {}. Automation will stop.", e);
                        log_and_file("WARN", &format!("Replan failed: {}", e));
                        Err(format!("Unable to replan: {}. Consider trying a different approach or providing more details.", e))
                    }
                }
            } else {
                Err("No execution state available for replan".to_string())
            }
        },
        ActionType::RequestTakeover => {
            if let Some(params) = &action.parameters {
                let instructions = params.get("instructions").unwrap_or(&"Please complete the required action".to_string()).clone();
                
                info!("Requesting user takeover: {}", instructions);
                log_and_file("INFO", &format!("Requesting user takeover: {}", instructions));
                
                // Reset takeover complete flag
                *TAKEOVER_COMPLETE.lock().unwrap() = false;
                
                // Bring Heelix window to foreground
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    info!("Brought Heelix window to foreground for user takeover");
                }
                
                // Emit event to frontend
                app_handle.emit("user_takeover_required", serde_json::json!({
                    "instructions": instructions,
                    "reasoning": action.reasoning.clone(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                })).map_err(|e| format!("Failed to emit takeover request: {}", e))?;

                // Pause usage timer while waiting for user
                *USAGE_PAUSED.lock().unwrap() = true;
                *PAUSE_START_TIME.lock().unwrap() = Some(Instant::now());
                info!("Usage timer paused for user takeover");

                // Wait for user to complete the takeover
                info!("Waiting for user to complete manual action...");

                let start_time = Instant::now();
                let timeout = Duration::from_secs(300); // 5 minute timeout
                
                loop {
                    // Check if takeover is complete
                    if *TAKEOVER_COMPLETE.lock().unwrap() {
                        info!("User completed takeover action");
                        log_and_file("INFO", "User completed takeover action");

                        // Resume usage timer
                        if let Some(pause_start) = *PAUSE_START_TIME.lock().unwrap() {
                            let pause_duration = pause_start.elapsed();
                            *TOTAL_PAUSED_DURATION.lock().unwrap() += pause_duration;
                            info!("Usage timer resumed. Paused for {} seconds", pause_duration.as_secs());
                        }
                        *USAGE_PAUSED.lock().unwrap() = false;
                        *PAUSE_START_TIME.lock().unwrap() = None;

                        // Reset the flag
                        *TAKEOVER_COMPLETE.lock().unwrap() = false;

                        // Give UI time to update after manual action
                        tokio::time::sleep(Duration::from_secs(2)).await;

                        return Ok("User completed takeover".to_string());
                    }
                    
                    // Check for timeout
                    if start_time.elapsed() > timeout {
                        warn!("User takeover timed out after 5 minutes");

                        // Resume usage timer on timeout
                        if let Some(pause_start) = *PAUSE_START_TIME.lock().unwrap() {
                            let pause_duration = pause_start.elapsed();
                            *TOTAL_PAUSED_DURATION.lock().unwrap() += pause_duration;
                        }
                        *USAGE_PAUSED.lock().unwrap() = false;
                        *PAUSE_START_TIME.lock().unwrap() = None;

                        return Err("User takeover timed out".to_string());
                    }

                    // Check if automation was stopped
                    if *SHOULD_STOP_EXECUTION.lock().unwrap() {
                        info!("Takeover cancelled - automation stopped");

                        // Resume usage timer on cancellation
                        if let Some(pause_start) = *PAUSE_START_TIME.lock().unwrap() {
                            let pause_duration = pause_start.elapsed();
                            *TOTAL_PAUSED_DURATION.lock().unwrap() += pause_duration;
                        }
                        *USAGE_PAUSED.lock().unwrap() = false;
                        *PAUSE_START_TIME.lock().unwrap() = None;

                        return Err("Takeover cancelled - automation stopped".to_string());
                    }
                    
                    // Sleep briefly before checking again
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            } else {
                Err("Instructions required for user takeover".to_string())
            }
        },
        ActionType::AskClarification => {
            if let Some(params) = &action.parameters {
                let question = params.get("question").unwrap_or(&"Please provide clarification".to_string()).clone();
                
                info!("Requesting clarification: {}", question);
                log_and_file("INFO", &format!("Requesting clarification: {}", question));
                
                // Reset clarification response
                *CLARIFICATION_RESPONSE.lock().unwrap() = None;
                
                // Bring Heelix window to foreground
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    info!("Brought Heelix window to foreground for clarification request");
                }
                
                // Emit event to frontend
                app_handle.emit("clarification_required", serde_json::json!({
                    "question": question,
                    "reasoning": action.reasoning.clone(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                })).map_err(|e| format!("Failed to emit clarification request: {}", e))?;

                // Pause usage timer while waiting for user
                *USAGE_PAUSED.lock().unwrap() = true;
                *PAUSE_START_TIME.lock().unwrap() = Some(Instant::now());
                info!("Usage timer paused for clarification request");

                // Wait for user to provide clarification
                info!("Waiting for user to provide clarification...");

                let start_time = Instant::now();
                let timeout = Duration::from_secs(120); // 2 minute timeout
                
                loop {
                    // Check if clarification response is received
                    let response = {
                        let guard = CLARIFICATION_RESPONSE.lock().unwrap();
                        guard.clone()
                    };
                    
                    if let Some(user_response) = response {
                        info!("User provided clarification: {}", user_response);
                        log_and_file("INFO", &format!("User clarification received: {}", user_response));

                        // Resume usage timer
                        if let Some(pause_start) = *PAUSE_START_TIME.lock().unwrap() {
                            let pause_duration = pause_start.elapsed();
                            *TOTAL_PAUSED_DURATION.lock().unwrap() += pause_duration;
                            info!("Usage timer resumed. Paused for {} seconds", pause_duration.as_secs());
                        }
                        *USAGE_PAUSED.lock().unwrap() = false;
                        *PAUSE_START_TIME.lock().unwrap() = None;

                        // Add the clarification to the LLM session if available
                        {
                            let mut session_guard = LLM_SESSION.lock().unwrap();
                            if let Some(session) = session_guard.as_mut() {
                                // Add the clarification as a user message
                                let clarification_message = format!(
                                    "## User Clarification\nQuestion: {}\nAnswer: {}",
                                    question, user_response
                                );
                                session.add_user_message(clarification_message);
                                info!("Added clarification to LLM session");
                            }
                        }

                        // Clear the response
                        *CLARIFICATION_RESPONSE.lock().unwrap() = None;

                        // Give a moment for the UI to update
                        tokio::time::sleep(Duration::from_millis(500)).await;

                        return Ok("User provided clarification".to_string());
                    }
                    
                    // Check if automation was stopped
                    if *SHOULD_STOP_EXECUTION.lock().unwrap() {
                        info!("Clarification request cancelled - automation stopped");

                        // Resume usage timer on cancellation
                        if let Some(pause_start) = *PAUSE_START_TIME.lock().unwrap() {
                            let pause_duration = pause_start.elapsed();
                            *TOTAL_PAUSED_DURATION.lock().unwrap() += pause_duration;
                        }
                        *USAGE_PAUSED.lock().unwrap() = false;
                        *PAUSE_START_TIME.lock().unwrap() = None;

                        return Err("Clarification request cancelled - automation stopped".to_string());
                    }

                    // Check for timeout
                    if start_time.elapsed() > timeout {
                        warn!("Clarification request timed out after 2 minutes");

                        // Resume usage timer on timeout
                        if let Some(pause_start) = *PAUSE_START_TIME.lock().unwrap() {
                            let pause_duration = pause_start.elapsed();
                            *TOTAL_PAUSED_DURATION.lock().unwrap() += pause_duration;
                        }
                        *USAGE_PAUSED.lock().unwrap() = false;
                        *PAUSE_START_TIME.lock().unwrap() = None;

                        return Err("Clarification request timed out".to_string());
                    }
                    
                    // Sleep briefly before checking again
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            } else {
                Err("Clarification question is required for AskClarification action".to_string())
            }
        },
        ActionType::AllElements => {
            // This should be handled in the reactive loop, not here
            warn!("AllElements action reached execute_action - this should have been handled in the loop");
            Ok("AllElements handled".to_string())
        },
        ActionType::FullText => {
            // This should be handled in the reactive loop, not here
            warn!("FullText action reached execute_action - this should have been handled in the loop");
            Ok("FullText handled".to_string())
        },
        ActionType::MemorySave => {
            // This should be handled in the reactive loop, not here
            warn!("MemorySave action reached execute_action - this should have been handled in the loop");
            Ok("Memory saved".to_string())
        },
        ActionType::ExcelType => {
            if let Some(params) = &action.parameters {
                if let Some(cell_data) = params.get("cell_data") {
                    info!("Entering data into Excel cells: {}", cell_data);
                    
                    // Parse the cell data
                    let mut cells = Vec::new();
                    let mut values = Vec::new();
                    
                    for pair in cell_data.split(":::") {
                        let mut parts = pair.splitn(2, ':');
                        if let (Some(cell), Some(value)) = (parts.next(), parts.next()) {
                            cells.push(cell.to_string());
                            values.push(value.to_string());
                        }
                    }
                    
                    if cells.is_empty() {
                        return Err("No cell data to enter".to_string());
                    }
                    
                    // Platform-specific implementation
                    #[cfg(target_os = "macos")]
                    {
                        // Create AppleScript to set multiple cells at once
                        let mut script = String::from("tell application \"Microsoft Excel\"\n");
                        script.push_str("    activate\n");
                        
                        // Add each cell assignment
                        for (cell, value) in cells.iter().zip(values.iter()) {
                            // Check if this is a formula (starts with =)
                            if value.starts_with("=") {
                                // For formulas, set the formula property directly
                                script.push_str(&format!("    set formula of range \"{}\" to \"{}\"\n", cell, value));
                            } else {
                                // For regular values, escape quotes
                                let escaped_value = value.replace("\"", "\\\"");
                                script.push_str(&format!("    set value of range \"{}\" to \"{}\"\n", cell, escaped_value));
                            }
                        }
                        
                        script.push_str("end tell");
                        
                        info!("Executing AppleScript for Excel data entry");
                        
                        // Execute the AppleScript
                        match crate::window_details_collector::macos::macos_acting_engine::execute_applescript(&script).await {
                            Ok(_output) => {
                                info!("Successfully entered data into {} Excel cells", cells.len());
                                Ok(format!("Entered data into {} cells", cells.len()))
                            },
                            Err(e) => {
                                error!("Failed to execute AppleScript for Excel: {}", e);
                                Err(format!("Failed to enter Excel data: {}", e))
                            }
                        }
                    }
                    
                    #[cfg(target_os = "windows")]
                    {
                        // Use NVDA bridge to set Excel cells
                        use crate::window_details_collector::windows::windows_nvda_bridge;
                        
                        // The cell_data is already in the correct format from the action parameters
                        info!("Using NVDA bridge to set {} Excel cells", cells.len());
                        
                        match windows_nvda_bridge::set_excel_cells_async(&cell_data).await {
                            Ok(()) => {
                                info!("Successfully set {} Excel cells via NVDA", cells.len());
                                Ok(format!("Set {} cells via NVDA", cells.len()))
                            },
                            Err(e) => {
                                error!("Failed to set Excel cells via NVDA: {}", e);
                                Err(format!("Failed to set Excel cells: {}", e))
                            }
                        }
                    }
                    
                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    {
                        Err("Excel automation not supported on this platform".to_string())
                    }
                } else {
                    Err("Cell data is required for ExcelType action".to_string())
                }
            } else {
                Err("Parameters are required for ExcelType action".to_string())
            }
        },
        ActionType::NavigateURL => {
            if let (Some(app_name), Some(target_element), Some(params)) = (&action.app_name, &action.target_element, &action.parameters) {
                if let Some(url) = params.get("url") {
                    info!("Navigating to URL: {} in {} using element {}", url, app_name, target_element);
                    
                    // Use the same typing logic as the Type action for consistency
                    #[cfg(target_os = "windows")]
                    let result = {
                        use crate::window_details_collector::windows::windows_action_engine;
                        windows_action_engine::type_text_in_element_by_id_async("", target_element, url).await
                    };
                    
                    #[cfg(target_os = "macos")]
                    let result = crate::window_details_collector::macos::macos_acting_engine::type_text_in_app(
                        app_name, url, Some(target_element)
                    ).await;
                    
                    if !result.success {
                        return Err(format!("Failed to type URL: {}", result.message));
                    }
                    
                    // Press Enter to navigate
                    #[cfg(target_os = "windows")]
                    let enter_result = {
                        use crate::window_details_collector::windows::windows_action_engine;
                        use crate::window_details_collector::windows::ActionResult;
                        // Use tokio::task::spawn_blocking for the synchronous press_key function
                        tokio::task::spawn_blocking(|| {
                            windows_action_engine::press_key("enter")
                        }).await
                        .unwrap_or_else(|e| ActionResult::failure(&format!("Failed to spawn blocking task: {}", e)))
                    };
                    
                    #[cfg(target_os = "macos")]
                    let enter_result = crate::window_details_collector::macos::macos_acting_engine::press_key_in_app(
                        app_name, "enter"
                    ).await;
                    
                    if !enter_result.success {
                        return Err(format!("Failed to press Enter: {}", enter_result.message));
                    }
                    
                    info!("Successfully initiated navigation to {}", url);
                    Ok(format!("Navigating to {}", url))
                } else {
                    Err("URL parameter is required for NavigateURL action".to_string())
                }
            } else {
                Err("App name, target element, and parameters are required for NavigateURL action".to_string())
            }
        },
        ActionType::ExcelCommand => {
            // Execute Excel-specific commands (platform-specific)
            #[cfg(target_os = "macos")]
            {
                if let Some(params) = &action.parameters {
                    let command = params.get("command").map(|s| s.as_str()).unwrap_or("UNKNOWN");
                    info!("Executing Excel AppleScript command: {}", command);
                    log_and_file("INFO", &format!("Executing Excel command: {}", command));

                    // Execute via the app_commands module (all param handling is internal)
                    match crate::engine::app_commands::excel_macos::execute_excel_command_from_params(params).await {
                        Ok(result) => {
                            info!("Excel command succeeded: {}", result);
                            log_and_file("INFO", &format!("Excel command result: {}", result));

                            // For GET commands, store the result in memory
                            if crate::engine::app_commands::excel_macos::is_data_retrieval_command(command) {
                                let mut mem = MEMORY_STORE.lock().unwrap();
                                if !mem.is_empty() {
                                    mem.push_str("\n---\n");
                                }
                                mem.push_str(&format!("Excel data: {}", result));
                                info!("Stored Excel data in memory");
                            }

                            Ok(result)  // Return the confirmation message
                        },
                        Err(e) => {
                            error!("Excel command failed: {}", e);
                            Err(e)
                        }
                    }
                } else {
                    Err("Parameters required for ExcelCommand action".to_string())
                }
            }

            #[cfg(target_os = "windows")]
            {
                if let Some(params) = &action.parameters {
                    let command = params.get("command").map(|s| s.as_str()).unwrap_or("UNKNOWN");
                    info!("Executing Excel COM command via NVDA: {}", command);
                    log_and_file("INFO", &format!("Executing Excel command via NVDA: {}", command));

                    // Execute via the app_commands module using NVDA bridge
                    match crate::engine::app_commands::excel_windows::execute_excel_command_from_params(params).await {
                        Ok(result) => {
                            info!("Excel command succeeded: {}", result);
                            log_and_file("INFO", &format!("Excel command result: {}", result));

                            // For GET commands, store the result in memory
                            if crate::engine::app_commands::excel_windows::is_data_retrieval_command(command) {
                                let mut mem = MEMORY_STORE.lock().unwrap();
                                if !mem.is_empty() {
                                    mem.push_str("\n---\n");
                                }
                                mem.push_str(&format!("Excel data: {}", result));
                                info!("Stored Excel data in memory");
                            }

                            Ok(result)  // Return the confirmation message
                        },
                        Err(e) => {
                            error!("Excel command failed: {}", e);
                            Err(e)
                        }
                    }
                } else {
                    Err("Parameters required for ExcelCommand action".to_string())
                }
            }

            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                warn!("ExcelCommand is only supported on macOS and Windows");
                Err("Excel commands are only available on macOS and Windows".to_string())
            }
        },
        ActionType::WordCommand => {
            // Execute Word-specific commands (platform-specific)
            #[cfg(target_os = "macos")]
            {
                if let Some(params) = &action.parameters {
                    let command = params.get("command").map(|s| s.as_str()).unwrap_or("UNKNOWN");
                    info!("Executing Word AppleScript command: {}", command);
                    log_and_file("INFO", &format!("Executing Word command: {}", command));

                    // Execute via the app_commands module (all param handling is internal)
                    match crate::engine::app_commands::word_macos::execute_word_command_from_params(params).await {
                        Ok(result) => {
                            info!("Word command succeeded: {}", result);
                            log_and_file("INFO", &format!("Word command result: {}", result));

                            // For GET commands, store the result in memory
                            if crate::engine::app_commands::word_macos::is_data_retrieval_command(command) {
                                let mut mem = MEMORY_STORE.lock().unwrap();
                                if !mem.is_empty() {
                                    mem.push_str("\n---\n");
                                }
                                mem.push_str(&format!("Word data: {}", result));
                                info!("Stored Word data in memory");
                            }

                            Ok(result)  // Return the confirmation message
                        },
                        Err(e) => {
                            error!("Word command failed: {}", e);
                            Err(e)
                        }
                    }
                } else {
                    Err("Parameters required for WordCommand action".to_string())
                }
            }

            #[cfg(target_os = "windows")]
            {
                if let Some(params) = &action.parameters {
                    let command = params.get("command").map(|s| s.as_str()).unwrap_or("UNKNOWN");
                    info!("Executing Word COM command via NVDA: {}", command);
                    log_and_file("INFO", &format!("Executing Word command via NVDA: {}", command));

                    // Execute via the app_commands module using NVDA bridge
                    match crate::engine::app_commands::word_windows::execute_word_command_from_params(params).await {
                        Ok(result) => {
                            info!("Word command succeeded: {}", result);
                            log_and_file("INFO", &format!("Word command result: {}", result));

                            // For GET commands, store the result in memory
                            if crate::engine::app_commands::word_windows::is_data_retrieval_command(command) {
                                let mut mem = MEMORY_STORE.lock().unwrap();
                                if !mem.is_empty() {
                                    mem.push_str("\n---\n");
                                }
                                mem.push_str(&format!("Word data: {}", result));
                                info!("Stored Word data in memory");
                            }

                            Ok(result)  // Return the confirmation message
                        },
                        Err(e) => {
                            error!("Word command failed: {}", e);
                            Err(e)
                        }
                    }
                } else {
                    Err("Parameters required for WordCommand action".to_string())
                }
            }

            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                warn!("WordCommand is only supported on macOS and Windows");
                Err("Word commands are only available on macOS and Windows".to_string())
            }
        },
        ActionType::PowerPointCommand => {
            // Execute PowerPoint-specific commands (Windows only via COM)
            #[cfg(target_os = "windows")]
            {
                if let Some(params) = &action.parameters {
                    let command = params.get("command").map(|s| s.as_str()).unwrap_or("UNKNOWN");
                    info!("Executing PowerPoint COM command via NVDA: {}", command);
                    log_and_file("INFO", &format!("Executing PowerPoint command via NVDA: {}", command));

                    // Execute via the app_commands module using NVDA bridge
                    match crate::engine::app_commands::powerpoint_windows::execute_powerpoint_command_from_params(params).await {
                        Ok(result) => {
                            info!("PowerPoint command succeeded: {}", result);
                            log_and_file("INFO", &format!("PowerPoint command result: {}", result));

                            // For GET commands, store the result in memory
                            if crate::engine::app_commands::powerpoint_windows::is_data_retrieval_command(command) {
                                let mut mem = MEMORY_STORE.lock().unwrap();
                                if !mem.is_empty() {
                                    mem.push_str("\n---\n");
                                }
                                mem.push_str(&format!("PowerPoint data: {}", result));
                                info!("Stored PowerPoint data in memory");
                            }

                            Ok(result)  // Return the confirmation message
                        },
                        Err(e) => {
                            error!("PowerPoint command failed: {}", e);
                            Err(e)
                        }
                    }
                } else {
                    Err("Parameters required for PowerPointCommand action".to_string())
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                warn!("PowerPointCommand is only supported on Windows");
                Err("PowerPoint commands are only available on Windows".to_string())
            }
        },
        ActionType::ParseError => {
            // This should never be executed - ParseError is handled in the main loop
            warn!("ParseError action reached execute_action - this should be handled in the main loop");
            Ok("ParseError handled".to_string())
        },
        // Terminal execution actions
        ActionType::TerminalRun => {
            if let Some(params) = &action.parameters {
                let command = params.get("command").map(|s| s.as_str()).unwrap_or("");
                let cwd = params.get("cwd").map(|s| s.as_str());
                
                if command.is_empty() {
                    return Err("Command is required for TerminalRun action".to_string());
                }
                
                info!("Executing terminal command: {}", command);
                log_and_file("INFO", &format!("Terminal command: {}", command));
                
                // Use the terminal executor with permission checking
                let options = crate::engine::terminal::executor::ExecutionOptions {
                    cwd: cwd.map(|s| s.to_string()),
                    env: None,
                    timeout_secs: Some(900), // 15 minute timeout (large repos, builds, AI coding)
                    background: false,
                    reason: Some(action.reasoning.clone()),
                    skip_approval: false,
                };
                
                // Spawn the execution in a task
                let app_handle_clone = app_handle.clone();
                let command_clone = command.to_string();
                
                let result = {
                    let execution_future = crate::engine::terminal::executor::execute(&command_clone, options);
                    
                    // Poll for pending approval requests and emit UI events
                    tokio::select! {
                        result = execution_future => result,
                        // Continuously check for approval requests while waiting
                        _ = async {
                            loop {
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                if let Some(request) = crate::engine::terminal::executor::get_pending_approval_request().await {
                                    info!("Emitting terminal approval request to UI");
                                    let _ = app_handle_clone.emit("terminal-approval-request", serde_json::json!({
                                        "request_id": request.request_id,
                                        "command": request.command,
                                        "executable": request.executable,
                                        "resolved_path": request.resolved_path,
                                        "working_dir": request.working_dir,
                                        "timestamp": request.timestamp,
                                        "reason": request.reason
                                    }));
                                    // After emitting, wait until it's resolved (don't emit again)
                                    while crate::engine::terminal::executor::get_pending_approval_request().await.is_some() {
                                        tokio::time::sleep(Duration::from_millis(100)).await;
                                    }
                                }
                            }
                        } => unreachable!()
                    }
                };
                
                if result.denied {
                    warn!("Terminal command denied: {}", result.error.as_ref().unwrap_or(&"Unknown".to_string()));
                    Err(format!("Command denied: {}", result.error.unwrap_or_else(|| "Permission denied".to_string())))
                } else if result.success {
                    // Store output in memory for the agent to use
                    // For Claude CLI commands, prefer the clean parsed content over raw JSONL stdout
                    let raw_output = if let Some(ref clean) = result.claude_content {
                        clean.clone()
                    } else if !result.stdout.is_empty() {
                        result.stdout.clone()
                    } else if !result.stderr.is_empty() {
                        result.stderr.clone()
                    } else {
                        "Command completed with no output".to_string()
                    };
                    
                    // Clean the output: strip ANSI escapes and progress lines
                    // (progress is great for the UI but wastes memory budget for the agent)
                    let output = if result.claude_content.is_some() {
                        raw_output // Claude CLI content is already clean
                    } else {
                        clean_terminal_output_for_memory(&raw_output)
                    };
                    
                    // Truncate output if too long (generous limit to preserve test results, build errors, AI responses)
                    let truncated_output = if output.len() > 10000 {
                        format!("{}...\n[Output truncated, {} total chars]", truncate_str(&output, 10000), output.len())
                    } else {
                        output.clone()
                    };
                    
                    // Store in memory for agent context
                    {
                        let mut mem = MEMORY_STORE.lock().unwrap();
                        if !mem.is_empty() {
                            mem.push_str("\n---\n");
                        }
                        mem.push_str(&format!("Terminal output ({}): {}", command, truncated_output));
                    }
                    
                    info!("Terminal command completed: exit_code={:?}", result.exit_code);
                    Ok(format!("Command completed (exit {}): {}", 
                        result.exit_code.unwrap_or(0),
                        if truncated_output.len() > 100 { 
                            format!("{}...", truncate_str(&truncated_output, 100)) 
                        } else { 
                            truncated_output 
                        }
                    ))
                } else {
                    let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
                    warn!("Terminal command failed: {}", error_msg);
                    Err(format!("Command failed: {}", error_msg))
                }
            } else {
                Err("Parameters required for TerminalRun action".to_string())
            }
        },
        ActionType::TerminalBackground => {
            if let Some(params) = &action.parameters {
                let command = params.get("command").map(|s| s.as_str()).unwrap_or("");
                let cwd = params.get("cwd").map(|s| s.as_str());
                
                if command.is_empty() {
                    return Err("Command is required for TerminalBackground action".to_string());
                }
                
                info!("Starting background process: {}", command);
                log_and_file("INFO", &format!("Background process: {}", command));
                
                let options = crate::engine::terminal::executor::ExecutionOptions {
                    cwd: cwd.map(|s| s.to_string()),
                    env: None,
                    timeout_secs: None, // No timeout for background processes
                    background: true,
                    reason: Some(action.reasoning.clone()),
                    skip_approval: false,
                };
                
                // Same approval polling pattern for background processes
                let app_handle_clone = app_handle.clone();
                let command_clone = command.to_string();
                
                let result = {
                    let execution_future = crate::engine::terminal::executor::execute(&command_clone, options);
                    
                    tokio::select! {
                        result = execution_future => result,
                        _ = async {
                            loop {
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                if let Some(request) = crate::engine::terminal::executor::get_pending_approval_request().await {
                                    info!("Emitting terminal approval request to UI (background)");
                                    let _ = app_handle_clone.emit("terminal-approval-request", serde_json::json!({
                                        "request_id": request.request_id,
                                        "command": request.command,
                                        "executable": request.executable,
                                        "resolved_path": request.resolved_path,
                                        "working_dir": request.working_dir,
                                        "timestamp": request.timestamp,
                                        "reason": request.reason
                                    }));
                                    while crate::engine::terminal::executor::get_pending_approval_request().await.is_some() {
                                        tokio::time::sleep(Duration::from_millis(100)).await;
                                    }
                                }
                            }
                        } => unreachable!()
                    }
                };
                
                if result.denied {
                    Err(format!("Command denied: {}", result.error.unwrap_or_else(|| "Permission denied".to_string())))
                } else if result.running {
                    // Store process ID in memory for later reference
                    {
                        let mut mem = MEMORY_STORE.lock().unwrap();
                        if !mem.is_empty() {
                            mem.push_str("\n---\n");
                        }
                        mem.push_str(&format!("Background process started: {} (ID: {})", command, result.process_id));
                    }
                    
                    info!("Background process started: {}", result.process_id);
                    Ok(format!("Background process started with ID: {}", result.process_id))
                } else {
                    Err(format!("Failed to start background process: {}", result.error.unwrap_or_else(|| "Unknown error".to_string())))
                }
            } else {
                Err("Parameters required for TerminalBackground action".to_string())
            }
        },
        ActionType::TerminalCheck => {
            if let Some(params) = &action.parameters {
                let process_id = params.get("process_id").map(|s| s.as_str()).unwrap_or("");
                
                if process_id.is_empty() {
                    return Err("Process ID is required for TerminalCheck action".to_string());
                }
                
                info!("Checking process status: {}", process_id);
                
                let is_running = crate::engine::terminal::executor::is_process_running(process_id).await;
                
                if is_running {
                    // Get current output
                    if let Some((stdout, stderr)) = crate::engine::terminal::executor::get_process_output(process_id).await {
                        let output = if !stdout.is_empty() { stdout } else { stderr };
                        let truncated = if output.len() > 500 {
                            format!("{}...", &output[output.len()-500..])
                        } else {
                            output
                        };
                        Ok(format!("Process {} is running. Recent output: {}", process_id, truncated))
                    } else {
                        Ok(format!("Process {} is running", process_id))
                    }
                } else {
                    // Process completed - get final output
                    if let Some((stdout, stderr)) = crate::engine::terminal::executor::get_process_output(process_id).await {
                        let output = if !stdout.is_empty() { stdout } else { stderr };
                        Ok(format!("Process {} completed. Output: {}", process_id, output))
                    } else {
                        Ok(format!("Process {} has completed", process_id))
                    }
                }
            } else {
                Err("Parameters required for TerminalCheck action".to_string())
            }
        },
        ActionType::TerminalKill => {
            if let Some(params) = &action.parameters {
                let process_id = params.get("process_id").map(|s| s.as_str()).unwrap_or("");
                
                if process_id.is_empty() {
                    return Err("Process ID is required for TerminalKill action".to_string());
                }
                
                info!("Killing process: {}", process_id);
                log_and_file("INFO", &format!("Killing process: {}", process_id));
                
                match crate::engine::terminal::executor::kill_process(process_id).await {
                    Ok(()) => {
                        info!("Process {} killed successfully", process_id);
                        Ok(format!("Process {} killed", process_id))
                    },
                    Err(e) => {
                        warn!("Failed to kill process {}: {}", process_id, e);
                        Err(format!("Failed to kill process: {}", e))
                    }
                }
            } else {
                Err("Parameters required for TerminalKill action".to_string())
            }
        },
        ActionType::TerminalRead => {
            if let Some(params) = &action.parameters {
                let process_id = params.get("process_id").map(|s| s.as_str()).unwrap_or("");
                
                if process_id.is_empty() {
                    return Err("Process ID is required for TerminalRead action".to_string());
                }
                
                info!("Reading output from process: {}", process_id);
                
                if let Some((stdout, stderr)) = crate::engine::terminal::executor::get_process_output(process_id).await {
                    let output = if !stdout.is_empty() {
                        stdout
                    } else if !stderr.is_empty() {
                        format!("(stderr) {}", stderr)
                    } else {
                        "No output available".to_string()
                    };
                    
                    // Store in memory
                    {
                        let mut mem = MEMORY_STORE.lock().unwrap();
                        if !mem.is_empty() {
                            mem.push_str("\n---\n");
                        }
                        mem.push_str(&format!("Process {} output: {}", process_id, output));
                    }
                    
                    Ok(format!("Process output: {}", output))
                } else {
                    Err(format!("Process {} not found", process_id))
                }
            } else {
                Err("Parameters required for TerminalRead action".to_string())
            }
        },
        // Browser console: fetch Chrome JS errors via CDP (--remote-debugging-port=9222)
        ActionType::BrowserConsole => {
            info!("Fetching browser console errors via CDP");
            match crate::engine::browser_console::get_console_errors().await {
                Some(errors) => {
                    let _ = crate::engine::browser_console::clear_console_errors().await;
                    Ok(errors)
                },
                None => {
                    Ok("No browser console errors found (Chrome DevTools may not be available — launch Chrome with --remote-debugging-port=9222 to enable).".to_string())
                }
            }
        },
        // Direct file write — bypasses shell escaping
        ActionType::WriteFile => {
            if let Some(params) = &action.parameters {
                let path = params.get("path").map(|s| s.as_str()).unwrap_or("");
                let content = params.get("content").map(|s| s.as_str()).unwrap_or("");

                if path.is_empty() {
                    return Err("File path is required for WriteFile action".to_string());
                }

                info!("Writing file: {} ({} bytes)", path, content.len());
                log_and_file("INFO", &format!("WriteFile: {} ({} bytes)", path, content.len()));

                let file_path = std::path::Path::new(path);

                if let Some(parent) = file_path.parent() {
                    if !parent.exists() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            return Err(format!("Failed to create directories for {}: {}", path, e));
                        }
                    }
                }

                match std::fs::write(file_path, content) {
                    Ok(_) => {
                        let msg = format!("File written successfully: {} ({} bytes)", path, content.len());
                        info!("{}", msg);
                        Ok(msg)
                    },
                    Err(e) => {
                        let msg = format!("Failed to write file {}: {}", path, e);
                        warn!("{}", msg);
                        Err(msg)
                    }
                }
            } else {
                Err("Parameters required for WriteFile action".to_string())
            }
        },
        // Parallel HTTP fetch: grab page content from URLs without using the browser
        ActionType::FetchPages => {
            if let Some(params) = &action.parameters {
                let urls_str = params.get("urls").map(|s| s.as_str()).unwrap_or("");

                if urls_str.is_empty() {
                    return Err("FETCH_PAGES requires at least one URL".to_string());
                }

                let urls: Vec<String> = urls_str
                    .split(',')
                    .map(|u| u.trim().to_string())
                    .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
                    .take(8)
                    .collect();

                if urls.is_empty() {
                    return Err("FETCH_PAGES: no valid HTTP URLs provided".to_string());
                }

                // Reject URLs already attempted in this run. Re-fetching is the
                // primary failure mode behind FETCH_PAGES loops, so we enforce
                // "one fetch per URL per run" server-side rather than relying on
                // the prompt.
                let run_id = *CURRENT_EXECUTION_RUN_ID.lock().unwrap();
                let (fresh_urls, duplicate_urls): (Vec<String>, Vec<(String, bool)>) = {
                    let mut ledger = FETCHED_URLS.lock().unwrap();
                    let entry = match (run_id, ledger.as_mut()) {
                        (Some(rid), Some(e)) if e.0 == rid => Some(e),
                        (Some(rid), _) => {
                            *ledger = Some((rid, std::collections::HashSet::new(), std::collections::HashSet::new()));
                            ledger.as_mut()
                        }
                        _ => None,
                    };
                    match entry {
                        Some(e) => {
                            let mut fresh = Vec::new();
                            let mut dup = Vec::new();
                            for u in urls.into_iter() {
                                if e.1.contains(&u) {
                                    let succeeded = e.2.contains(&u);
                                    dup.push((u, succeeded));
                                } else {
                                    fresh.push(u);
                                }
                            }
                            (fresh, dup)
                        }
                        None => (urls, Vec::new()),
                    }
                };

                if fresh_urls.is_empty() {
                    let mut msg = String::from("FETCH_PAGES rejected: every URL was already fetched this run.\n");
                    for (u, ok) in &duplicate_urls {
                        msg.push_str(&format!("  - {} ({})\n", u, if *ok { "succeeded earlier" } else { "failed earlier" }));
                    }
                    msg.push_str("\nDo NOT re-fetch. If a URL succeeded earlier, the content is in your RECENT ACTIONS / MEMORY — re-read it. If it failed, the URL is bad — try GOOGLE_SEARCH for a different source.");
                    return Err(msg);
                }

                let mut prefix = String::new();
                if !duplicate_urls.is_empty() {
                    prefix.push_str(&format!(
                        "Skipped {} duplicate URL(s) (already fetched this run — do NOT retry):\n",
                        duplicate_urls.len()
                    ));
                    for (u, ok) in &duplicate_urls {
                        prefix.push_str(&format!("  - {} ({})\n", u, if *ok { "succeeded earlier" } else { "failed earlier" }));
                    }
                    prefix.push('\n');
                }

                info!("FETCH_PAGES: fetching {} new URL(s) in parallel ({} skipped as duplicates)", fresh_urls.len(), duplicate_urls.len());
                log_and_file("INFO", &format!("FETCH_PAGES: {} URL(s): {}", fresh_urls.len(), fresh_urls.join(", ")));

                let (result, succeeded_urls) = crate::engine::web_fetcher::fetch_and_extract_pages(fresh_urls.clone()).await;

                // Mark every attempted URL as fetched (failures count too — a 404
                // doesn't become a 200 on retry).
                if let Some(rid) = run_id {
                    let mut ledger = FETCHED_URLS.lock().unwrap();
                    let entry = ledger.get_or_insert_with(|| (rid, std::collections::HashSet::new(), std::collections::HashSet::new()));
                    for u in &fresh_urls {
                        entry.1.insert(u.clone());
                    }
                    for u in &succeeded_urls {
                        entry.2.insert(u.clone());
                    }
                }

                Ok(format!("{}{}", prefix, result))
            } else {
                Err("Parameters required for FetchPages action".to_string())
            }
        },
        // Google Search acceleration: launch Chrome, navigate to search URL, wait for results
        ActionType::GoogleSearch => {
            if let Some(params) = &action.parameters {
                let query = params.get("query").map(|s| s.as_str()).unwrap_or("");

                if query.is_empty() {
                    return Err("Query is required for GoogleSearch action".to_string());
                }

                let encoded_query = urlencoding::encode(query);
                let search_url = format!("https://www.google.com/search?q={}", encoded_query);

                info!("Google Search acceleration: query='{}', url='{}'", query, search_url);
                log_and_file("INFO", &format!("Google Search: {}", query));

                // Step 1: Launch/focus Google Chrome
                #[cfg(target_os = "macos")]
                {
                    let launch_result = crate::window_details_collector::macos::macos_acting_engine::launch_app_and_wait("Google Chrome").await;
                    if !launch_result.success {
                        return Err(format!("Failed to launch Chrome: {}", launch_result.message));
                    }
                }

                #[cfg(target_os = "windows")]
                {
                    let launch_result = crate::window_details_collector::windows::windows_action_engine::launch_app_and_wait("Google Chrome").await;
                    if !launch_result.success {
                        return Err(format!("Failed to launch Chrome: {}", launch_result.message));
                    }
                }

                // Step 2: Focus address bar with Cmd+L (macOS) or Ctrl+L (Windows)
                tokio::time::sleep(Duration::from_millis(500)).await;

                #[cfg(target_os = "macos")]
                {
                    let key_result = crate::window_details_collector::macos::macos_acting_engine::press_key_in_app(
                        "Google Chrome", "cmd+l"
                    ).await;
                    if !key_result.success {
                        warn!("Failed to focus address bar with Cmd+L: {}", key_result.message);
                        let _ = crate::window_details_collector::macos::macos_acting_engine::press_key_in_app(
                            "Google Chrome", "cmd+t"
                        ).await;
                    }
                }

                #[cfg(target_os = "windows")]
                {
                    use crate::window_details_collector::windows::windows_action_engine;
                    let key_result = tokio::task::spawn_blocking(|| {
                        windows_action_engine::press_key("ctrl+l")
                    }).await.unwrap_or_else(|e| {
                        use crate::window_details_collector::windows::ActionResult;
                        ActionResult::failure(&format!("Failed to spawn blocking task: {}", e))
                    });
                    if !key_result.success {
                        warn!("Failed to focus address bar with Ctrl+L: {}", key_result.message);
                    }
                }

                tokio::time::sleep(Duration::from_millis(300)).await;

                // Step 3: Type the search URL into the focused address bar
                #[cfg(target_os = "macos")]
                {
                    let type_result = crate::window_details_collector::macos::macos_acting_engine::type_text_in_app(
                        "Google Chrome", &search_url, None
                    ).await;
                    if !type_result.success {
                        return Err(format!("Failed to type search URL: {}", type_result.message));
                    }
                }

                #[cfg(target_os = "windows")]
                {
                    use crate::window_details_collector::windows::windows_action_engine;
                    let url_clone = search_url.clone();
                    let type_result = windows_action_engine::type_text_in_element_by_id_async("", "", &url_clone).await;
                    if !type_result.success {
                        return Err(format!("Failed to type search URL: {}", type_result.message));
                    }
                }

                // Step 4: Press Enter to navigate
                #[cfg(target_os = "macos")]
                {
                    let enter_result = crate::window_details_collector::macos::macos_acting_engine::press_key_in_app(
                        "Google Chrome", "enter"
                    ).await;
                    if !enter_result.success {
                        return Err(format!("Failed to press Enter: {}", enter_result.message));
                    }
                }

                #[cfg(target_os = "windows")]
                {
                    use crate::window_details_collector::windows::windows_action_engine;
                    use crate::window_details_collector::windows::ActionResult;
                    let enter_result = tokio::task::spawn_blocking(|| {
                        windows_action_engine::press_key("enter")
                    }).await.unwrap_or_else(|e| ActionResult::failure(&format!("Failed to spawn blocking task: {}", e)));
                    if !enter_result.success {
                        return Err(format!("Failed to press Enter: {}", enter_result.message));
                    }
                }

                info!("Google Search navigation initiated for: {}", query);
                Ok(format!("Google search initiated for: {}. Results page loading.", query))
            } else {
                Err("Parameters required for GoogleSearch action".to_string())
            }
        }
    }
}

/// Update the execution progress in the UI
fn update_execution_progress(
    app_handle: &AppHandle,
    progress: f32,
    status_message: Option<String>
) -> Result<(), String> {
    // Only emit when progress is 100% (completion)
    if progress == 100.0 {
        let status = serde_json::json!({
            "isPlaying": false,
            "progress": progress,
            "statusMessage": status_message,
        });
        app_handle
            .emit("playback_status", status)
            .map_err(|e| format!("Failed to update execution progress: {}", e))?;
    }
    Ok(())
}

// ===== PROMPT CREATION AND PARSING =====

/// Create an incremental prompt for the LLM with only the current state update
fn create_incremental_prompt(
    app_state: &AppState,
    last_action_result: Option<&str>
) -> String {
    let mut prompt = String::new();
    
    // Add result of last action if available
    if let Some(result) = last_action_result {
        prompt.push_str(&format!("## Last Action Result\n{}\n\n", result));
    }
    
    // Don't include objective, script, or description - they're in the system prompt now
    
    // Show the ENTIRE history of recent actions so the model stays anchored
    prompt.push_str("## Recent Actions\n");
    if app_state.recent_actions.is_empty() {
        prompt.push_str("No actions performed yet\n\n");
    } else {
        // Check for repeated actions
        let mut repeated_action_warning = String::new();
        if app_state.recent_actions.len() >= 2 {
            // Count how many times the last action was repeated
            let last_action = &app_state.recent_actions[app_state.recent_actions.len() - 1];
            let mut repeat_count = 1;
            for i in (0..app_state.recent_actions.len() - 1).rev() {
                if app_state.recent_actions[i].command == last_action.command {
                    repeat_count += 1;
                } else {
                    break;
                }
            }
            
            if repeat_count >= 3 {
                repeated_action_warning = format!(
                    "\n🚨🚨🚨 CRITICAL: You performed '{}' {} TIMES IN A ROW! 🚨🚨🚨\n",
                    last_action.command, repeat_count
                );
                repeated_action_warning.push_str("THIS IS AN INFINITE LOOP! THE ACTION IS COMPLETELY BLOCKED!\n");
                repeated_action_warning.push_str("MANDATORY ACTIONS:\n");
                repeated_action_warning.push_str("1. Press ESC key to dismiss any modal\n");
                repeated_action_warning.push_str("2. If that fails, try browser back navigation (Alt+Left Arrow)\n");
                repeated_action_warning.push_str("3. If still stuck, ASK_CLARIFICATION for help\n");
                repeated_action_warning.push_str("DO NOT CLICK THE SAME ELEMENT AGAIN!\n\n");
            } else if repeat_count == 2 {
                repeated_action_warning = format!(
                    "\n⚠️ WARNING: You just performed '{}' TWICE IN A ROW! The action is NOT working!\n",
                    last_action.command
                );
                repeated_action_warning.push_str("🛑 STOP AND TRY SOMETHING DIFFERENT:\n");
                repeated_action_warning.push_str("- The UI might have a modal/dialog blocking your action\n");
                repeated_action_warning.push_str("- Try keyboard navigation (ESC, Tab, or browser shortcuts)\n");
                repeated_action_warning.push_str("- Look for dismiss/close buttons on any overlays\n");
                repeated_action_warning.push_str("- Try a completely different approach\n\n");
            }
        }
        
        // Show warning first if present
        if !repeated_action_warning.is_empty() {
            prompt.push_str(&repeated_action_warning);
        }
        
        // Check if last action was a Back button click in a browser - add tab navigation reminder
        // On macOS, command includes element label like "CLICK:42 Button:Back"
        // Also check justification as fallback for Windows or if LLM mentions "back" in reasoning
        if let Some(last_action) = app_state.recent_actions.last() {
            let is_browser = app_state.current_app.as_ref()
                .map(|app| app.contains("Chrome") || app.contains("Firefox") || 
                           app.contains("Edge") || app.contains("Safari") || app.contains("Opera"))
                .unwrap_or(false);
            
            let command_lower = last_action.command.to_lowercase();
            let is_back_click = command_lower.starts_with("click") && 
                (command_lower.contains("back") || 
                 last_action.justification.to_lowercase().contains("back"));
            
            if is_browser && is_back_click {
                prompt.push_str("\n💡 BROWSER TIP: If the Back button didn't work, the previous page may have opened in a NEW TAB.\n");
                prompt.push_str("Look for tab elements (often AXRadioButton or similar) and click the previous tab to return.\n\n");
            }
        }
        
        // Show ALL actions in reverse order (most recent first)
        for (idx, act) in app_state.recent_actions.iter().rev().enumerate() {
            prompt.push_str(&format!("{}. {} - {}\n", idx + 1, act.command, act.justification));
        }
        prompt.push_str("\n");
    }
    
    // Add current app state
    prompt.push_str("## Current State Update\n");
    if let Some(app_name) = &app_state.current_app {
        prompt.push_str(&format!("Active Application: {}\n", app_name));

        // Add app-specific commands if available for this app
        if let Some(app_commands) = crate::engine::app_commands::get_app_specific_commands(app_name) {
            prompt.push_str("\n## APP-SPECIFIC COMMANDS AVAILABLE\n");
            prompt.push_str("⚠️ The following commands are available ONLY for this application and are MORE RELIABLE than clicking UI elements:\n");
            prompt.push_str(&app_commands);
            prompt.push_str("\n");
        }
    }

    // Add user messages section if any were sent during execution
    {
        let msgs = USER_MESSAGES.lock().unwrap();
        if !msgs.is_empty() {
            prompt.push_str("## USER MESSAGES (sent during execution — treat as instructions)\n");
            for msg in msgs.iter() {
                prompt.push_str(&format!("{}\n", msg));
            }
            prompt.push_str("\n");
        }
    }

    // Add memory block
    {
        // Read from memory_manager (structured store), falling back to the
        // raw MEMORY_STORE if the manager is empty (e.g. mirror missed a path).
        // This is what populates the agent-mode CURRENT MEMORY block.
        let mem_contents = memory_manager::get_memory_contents();
        let raw = MEMORY_STORE.lock().unwrap();
        let body = if !mem_contents.is_empty() && mem_contents != "<empty>" {
            mem_contents
        } else {
            raw.clone()
        };
        prompt.push_str("## CURRENT MEMORY\n");
        if body.is_empty() {
            prompt.push_str("<empty>\n\n");
        } else {
            prompt.push_str(&body);
            prompt.push_str("\n\n");
        }
    }

    // Add only the actionable elements
    prompt.push_str("\n## Actionable UI Elements\n");

    // Check if current app is Linefox (our own app) - don't let LLM interact with it
    let is_linefox_app = app_state.current_app.as_ref()
        .map(|app| app.to_lowercase().contains("linefox"))
        .unwrap_or(false);

    if is_linefox_app {
        prompt.push_str("1. This is the Linefox agent app. Launch another app to carry out the user's task.\n");
    } else if app_state.accessible_elements.is_empty() {
        prompt.push_str("No actionable elements found\n");
    } else {
        let max_elements = std::cmp::min(app_state.accessible_elements.len(), 550);
        let mut element_number = 1;
        for element in app_state.accessible_elements.iter().take(max_elements) {
            // Check if this is a heading (not a clickable element)
            if element.starts_with("─── HEADING:") || element.starts_with("─── GROUP:") {
                // Add heading without a number (it's not clickable)
                prompt.push_str(&format!("\n{}\n", element));
            } else {
                // Regular element with a number
                let truncated_element = truncate_element_value(element, 200);
                prompt.push_str(&format!("{}. {}\n", element_number, truncated_element));
                element_number += 1;
            }
        }

        if app_state.accessible_elements.len() > 550 {
            prompt.push_str(&format!("... and {} more elements (use 'ALL_ELEMENTS' command to see all)\n",
                app_state.accessible_elements.len() - 550));
        }
    }
    
    // Add Word document info if available (cursor position + paragraph structure)
    if let Some(ref word_info) = app_state.word_document_info {
        prompt.push_str("\n## Word Document\n");
        prompt.push_str(word_info);
        prompt.push_str("\n");
    }

    // Add text content visible on screen (skip for Linefox app)
    if !is_linefox_app {
        if let Some(ref text_content) = app_state.text_content {
            prompt.push_str("\n## Other Visible Text\n");
            prompt.push_str(text_content);
            prompt.push_str("\n");
        }

        // Add raw element tree if available (condensed)
        if let Some(ref element_tree) = app_state.element_tree {
            prompt.push_str("\n## UI Context\n");
            let tree_preview = if element_tree.len() > 300 {
                let mut char_boundary = 300;
                while !element_tree.is_char_boundary(char_boundary) && char_boundary > 0 {
                    char_boundary -= 1;
                }
                format!("{}...", &element_tree[..char_boundary])
            } else {
                element_tree.clone()
            };
            prompt.push_str(&tree_preview);
            prompt.push_str("\n");
        }
    }
    
    prompt.push_str("\nProvide your next command with justification:\n");
    prompt.push_str("FORMAT: COMMAND || JUSTIFICATION\n");
    
    prompt
}

/// Helper to truncate element labels for command strings
fn truncate_element_label(label: &str, max_length: usize) -> String {
    if label.len() <= max_length {
        label.to_string()
    } else {
        let mut char_boundary = max_length;
        while !label.is_char_boundary(char_boundary) && char_boundary > 0 {
            char_boundary -= 1;
        }
        format!("{}...", &label[..char_boundary])
    }
}

/// Truncate element value field to specified max length
fn truncate_element_value(element: &str, max_value_length: usize) -> String {
    // Parse the element string to find and truncate V: field
    let parts: Vec<&str> = element.split(" | ").collect();
    let mut result_parts = Vec::new();
    
    for part in parts {
        if part.starts_with("V:") && part.len() > max_value_length + 2 {
            // Truncate the value part
            let value_start = 2; // "V:" is 2 chars
            let mut char_boundary = value_start + max_value_length;
            while char_boundary < part.len() && !part.is_char_boundary(char_boundary) && char_boundary > value_start {
                char_boundary -= 1;
            }
            if char_boundary > value_start && char_boundary < part.len() {
                result_parts.push(format!("{}...", &part[..char_boundary]));
            } else {
                result_parts.push(part.to_string());
            }
        } else {
            result_parts.push(part.to_string());
        }
    }
    
    result_parts.join(" | ")
}

/// Create a prompt with ALL elements when requested
fn create_all_elements_prompt(app_state: &AppState) -> String {
    let mut prompt = String::new();
    
    prompt.push_str("## Full Element Tree Requested\n");
    prompt.push_str(&format!("Active Application: {}\n", 
        app_state.current_app.as_deref().unwrap_or("Unknown")));
    
    prompt.push_str(&format!("\n## ALL {} Actionable UI Elements\n", 
        app_state.accessible_elements.len()));
    
    for (i, element) in app_state.accessible_elements.iter().enumerate() {
        prompt.push_str(&format!("{}. {}\n", i + 1, element));
    }
    
    prompt.push_str("\nNow you have access to all elements. Provide your next command:\n");
    prompt.push_str("FORMAT: COMMAND || JUSTIFICATION\n");
    
    prompt
}

/// Create a prompt with FULL text content when requested
/// Includes both actionable elements AND full text (some redundancy but comprehensive)
fn create_full_text_prompt(app_state: &AppState) -> String {
    let mut prompt = String::new();
    
    prompt.push_str("## Full Text Content Requested\n");
    prompt.push_str(&format!("Active Application: {}\n", 
        app_state.current_app.as_deref().unwrap_or("Unknown")));
    
    // Include memory for context
    {
        let mem_contents = memory_manager::get_memory_contents();
        let raw = MEMORY_STORE.lock().unwrap();
        let body = if !mem_contents.is_empty() && mem_contents != "<empty>" {
            mem_contents
        } else {
            raw.clone()
        };
        prompt.push_str("\n## CURRENT MEMORY\n");
        if body.is_empty() {
            prompt.push_str("<empty>\n");
        } else {
            prompt.push_str(&body);
            prompt.push_str("\n");
        }
    }
    
    // Include actionable elements (same as normal prompt - we have them anyway!)
    prompt.push_str("\n## Numbered list of actionable Elements\n");
    if app_state.accessible_elements.is_empty() {
        prompt.push_str("No actionable elements found\n");
    } else {
        let max_elements = std::cmp::min(app_state.accessible_elements.len(), 500);
        for (i, element) in app_state.accessible_elements.iter().take(max_elements).enumerate() {
            prompt.push_str(&format!("{}. {}\n", i + 1, element));
        }
        
        if app_state.accessible_elements.len() > 500 {
            prompt.push_str(&format!("... and {} more actionable elements\n", app_state.accessible_elements.len() - 500));
        }
    }
    
    // Include the FULL text content (not truncated)
    if let Some(ref full_text) = app_state.full_text_content {
        let char_count = full_text.len();
        prompt.push_str(&format!("\n## Complete Other Visible Text ({} characters)\n", char_count));
        prompt.push_str("(This may contain redundant info from elements above - that's fine)\n");
        prompt.push_str(full_text);
        prompt.push_str("\n");
    } else {
        prompt.push_str("\n## No additional text content available\n");
    }
    
    prompt.push_str("\nNow you have access to the full text content AND elements. Provide your next command:\n");
    prompt.push_str("FORMAT: COMMAND || JUSTIFICATION\n");
    
    prompt
}

/// Create a prompt for the LLM to decide the next action
fn create_action_decision_prompt(
    _app_handle: &AppHandle,
    execution_state: &ExecutionState,
    app_state: &AppState,
    _automation: &Automation
) -> String {
    let mut prompt = format!(
        "# Automation Execution\n\nName: {}\n",
        execution_state.name
    );
    
    // Don't include objective, script, or nl_description - they're in the system prompt now
    
    // =================== PRIORITIZE ACTION HISTORY FIRST ===================
    // Add recent actions as the FIRST and most prominent section
    prompt.push_str("\n## RECENT ACTIONS (REVIEW THESE FIRST! #1 is your MOST RECENT action)\n");
    if app_state.recent_actions.is_empty() {
        prompt.push_str("No actions performed yet\n");
    } else {
        prompt.push_str("Your complete action history (most recent first):\n");
        // Show ALL actions in reverse order (most recent first) - #1 is most recent
        for (i, action) in app_state.recent_actions.iter().rev().enumerate() {
            prompt.push_str(&format!(
                "{}. {} - {} ({})\n", 
                i + 1, 
                action.command, 
                action.justification,
                action.screen_context
            ));
        }
        
        // Check if last action was a Back button click in a browser - add tab navigation reminder
        // On macOS, command includes element label like "CLICK:42 Button:Back"
        // Also check justification as fallback for Windows or if LLM mentions "back" in reasoning
        if let Some(last_action) = app_state.recent_actions.last() {
            let is_browser = app_state.current_app.as_ref()
                .map(|app| app.contains("Chrome") || app.contains("Firefox") || 
                           app.contains("Edge") || app.contains("Safari") || app.contains("Opera"))
                .unwrap_or(false);
            
            let command_lower = last_action.command.to_lowercase();
            let is_back_click = command_lower.starts_with("click") && 
                (command_lower.contains("back") || 
                 last_action.justification.to_lowercase().contains("back"));
            
            if is_browser && is_back_click {
                prompt.push_str("\n💡 BROWSER TIP: If the Back button didn't work, the previous page may have opened in a NEW TAB.\n");
                prompt.push_str("Look for tab elements (often AXRadioButton or similar) and click the previous tab to return.\n");
            }
        }
    }
    
    // Add current app state with high prominence
    prompt.push_str("\n## CURRENT APP STATE\n");
    if let Some(app_name) = &app_state.current_app {
        prompt.push_str(&format!("ALREADY ACTIVE APPLICATION: {}\n", app_name.to_uppercase()));
    } else {
        prompt.push_str("ACTIVE APPLICATION: NONE\n");
    }
    
    // Add memory block
    {
        let mem_contents = memory_manager::get_memory_contents();
        let raw = MEMORY_STORE.lock().unwrap();
        let body = if !mem_contents.is_empty() && mem_contents != "<empty>" {
            mem_contents
        } else {
            raw.clone()
        };
        prompt.push_str("\n## CURRENT MEMORY\n");
        if body.is_empty() {
            prompt.push_str("<empty>\n");
        } else {
            prompt.push_str(&body);
            prompt.push_str("\n");
        }
    }
    
    // Add accessible elements (limit to minimize token usage while showing enough context)
    prompt.push_str("\n## Numbered list of actionable Elements\n");
    if app_state.accessible_elements.is_empty() {
        prompt.push_str("No actionable elements found\n");
    } else {
        let max_elements = std::cmp::min(app_state.accessible_elements.len(), 500);
        for (i, element) in app_state.accessible_elements.iter().take(max_elements).enumerate() {
            prompt.push_str(&format!("{}. {}\n", i + 1, element));
        }
        
        if app_state.accessible_elements.len() > 500 {
            prompt.push_str(&format!("... and {} more actionable elements\n", app_state.accessible_elements.len() - 500));
        }
    }
    
    // Add text content visible on screen for context
    if let Some(ref text_content) = app_state.text_content {
        prompt.push_str("\n## Other Visible Text (for context)\n");
        prompt.push_str("```\n");
        prompt.push_str(text_content);
        prompt.push_str("\n```\n");
    }
    
    // Add raw element tree if available (condensed)
    if let Some(ref element_tree) = app_state.element_tree {
        prompt.push_str("\n## Raw UI Structure (for context)\n");
        let tree_preview = if element_tree.len() > 300 {
            let mut char_boundary = 300;
            while !element_tree.is_char_boundary(char_boundary) && char_boundary > 0 {
                char_boundary -= 1;
            }
            format!("{}...", &element_tree[..char_boundary])
        } else {
            element_tree.clone()
        };
        prompt.push_str(&format!("```\n{}\n```\n", tree_preview));
    }
    
    // Add the request
    prompt.push_str("\nProvide your next command with justification:\n");
    prompt.push_str("FORMAT: COMMAND || JUSTIFICATION\n");
    
    prompt
}

/// Initialize automation log file
/// NOTE: File logging disabled to prevent reverse engineering
fn init_automation_log_file(_app_handle: &AppHandle, _automation_id: i64, _automation_name: &str) -> Result<(), String> {
    // File logging disabled - no logs are saved to disk
    Ok(())
}

/// Write message to both console and automation log file
fn log_and_file(level: &str, message: &str) {
    // Log to console (existing behavior)
    match level {
        "INFO" => info!("{}", message),
        "WARN" => warn!("{}", message),
        "ERROR" => error!("{}", message),
        "DEBUG" => debug!("{}", message),
        _ => info!("{}", message),
    }
    
    // Also log to file with timestamp
    let timestamp = chrono::Utc::now().format("%H:%M:%S%.3f").to_string();
    log_to_file(&format!("[{}][{}] {}", timestamp, level, message));
}

/// Write directly to automation log file (internal helper)
/// NOTE: File logging disabled to prevent reverse engineering
fn log_to_file(_message: &str) {
    // File logging disabled - no-op
}

/// Close automation log file
/// NOTE: File logging disabled to prevent reverse engineering
fn close_automation_log_file() {
    // File logging disabled - no-op
}



/// Log token usage for this automation step
fn log_token_usage(input_tokens: u32, output_tokens: u32, step_description: &str) {
    let message = format!(
        "TOKENS - Step: {} | Input: {} | Output: {} | Total: {}",
        step_description, input_tokens, output_tokens, input_tokens + output_tokens
    );
    log_and_file("TOKEN", &message);
    info!("Token usage: {}", message);
}

/// Log session-level token statistics
fn log_session_token_summary(session: &LLMSession, automation_id: i64) {
    let total_tokens = session.total_input_tokens + session.total_output_tokens;
    let api_calls = session.messages.len() / 2;
    
    let summary = format!(
        "TOKEN_SUMMARY - Automation ID: {} | Total Input: {} | Total Output: {} | Total Combined: {} | API Calls: {}",
        automation_id,
        session.total_input_tokens,
        session.total_output_tokens,
        total_tokens,
        api_calls
    );
    
    log_and_file("TOKEN_SUMMARY", &summary);
    info!("Session token summary: {}", summary);
}

// ===== AGENT MODE =====

use crate::engine::agent_mode_engine;
use crate::engine::memory_manager;

/// Maximum steps before agent mode forces completion (safety limit)
const MAX_AGENT_STEPS: i32 = 200;

/// How often the orchestrator reviews progress
const STEPS_PER_REVIEW: i32 = 30;

/// Return a static list of common apps for orchestrator context
fn get_installed_apps() -> Vec<String> {
    vec![
        "Google Chrome".to_string(),
        "Safari".to_string(),
        "Microsoft Excel".to_string(),
        "Microsoft Word".to_string(),
        "Finder".to_string(),
        "Notes".to_string(),
        "Terminal".to_string(),
    ]
}

/// Agent mode execution entry point
/// Uses orchestrator-executor pattern for multi-phase autonomous planning
pub async fn execute_automation_agent_mode(
    app_handle: &AppHandle,
    automation_id: i64,
    additional_instructions: Option<String>
) -> Result<(), String> {
    info!("Starting AGENT MODE execution for automation {}", automation_id);

    // Reset stop flag
    *SHOULD_STOP_EXECUTION.lock().unwrap() = false;

    // Get the automation
    let automation = app_handle
        .db(|db| automation_repository::get_automation_by_id(db, automation_id))
        .map_err(|e| format!("Failed to get automation: {}", e))?
        .ok_or_else(|| format!("Automation with ID {} not found", automation_id))?;

    // Fetch active role/persona from DB
    let active_role_content = app_handle
        .db(|db| {
            crate::repository::skill_repository::get_skills_by_type(
                db,
                &crate::entity::skill::SkillType::Role,
            )
        })
        .ok()
        .and_then(|skills| skills.into_iter().next())
        .map(|skill| crate::engine::skills_registry::format_role_for_prompt(&skill));

    agent_mode_engine::init_agent_mode(&automation.objective, active_role_content);

    // Emit agent mode started events
    let _ = app_handle.emit("automation_started", serde_json::json!({
        "automationId": automation_id,
        "automationName": automation.name.clone(),
        "agentMode": true,
    }));

    let _ = app_handle.emit("agent_mode_started", serde_json::json!({
        "automationId": automation_id,
        "objective": automation.objective.clone(),
    }));

    // Get user ID and create usage session
    let user_id = app_handle
        .db(|db| match get_setting(db, "user_id") {
            Ok(setting) => Some(setting.setting_value),
            Err(_) => None,
        })
        .unwrap_or_else(|| "default_user".to_string());

    let use_pro_model_for_billing = app_handle
        .db(|db| match get_setting(db, "use_pro_model") {
            Ok(setting) => setting.setting_value == "true",
            Err(_) => false,
        });

    let session_id = app_handle
        .db(|db| {
            crate::repository::usage_session_repository::create_usage_session(
                db,
                crate::entity::usage_session::NewUsageSession {
                    user_id: user_id.clone(),
                    automation_id,
                }
            )
        })
        .map_err(|e| format!("Failed to create usage session: {}", e))?;

    *CURRENT_USAGE_SESSION_ID.lock().unwrap() = Some(session_id);
    *SESSION_START_TIME.lock().unwrap() = Some(Instant::now());
    *USAGE_PAUSED.lock().unwrap() = false;
    *PAUSE_START_TIME.lock().unwrap() = None;
    *TOTAL_PAUSED_DURATION.lock().unwrap() = Duration::from_secs(0);

    // Spawn background task to update usage periodically
    let app_handle_billing = app_handle.clone();
    let billing_multiplier_percent = if use_pro_model_for_billing { 150 } else { 100 };
    let session_update_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            let session_id = match *CURRENT_USAGE_SESSION_ID.lock().unwrap() {
                Some(id) => id,
                None => break,
            };
            let elapsed = match *SESSION_START_TIME.lock().unwrap() {
                Some(start) => {
                    let total_elapsed = start.elapsed();
                    let total_paused = *TOTAL_PAUSED_DURATION.lock().unwrap();
                    let current_pause = if *USAGE_PAUSED.lock().unwrap() {
                        PAUSE_START_TIME.lock().unwrap()
                            .map(|pause_start| pause_start.elapsed())
                            .unwrap_or(Duration::from_secs(0))
                    } else {
                        Duration::from_secs(0)
                    };
                    total_elapsed.saturating_sub(total_paused + current_pause).as_secs() as i32
                }
                None => break,
            };
            if let Err(e) = app_handle_billing.db(|db| {
                crate::repository::usage_session_repository::update_usage_session_progress(
                    db, session_id, elapsed
                )
            }) {
                error!("Failed to update usage session progress: {}", e);
            }
            let base_minutes = (elapsed + 59) / 60;
            let billed_minutes = (base_minutes * billing_multiplier_percent + 99) / 100;
            let _ = app_handle_billing.emit("usage_update", serde_json::json!({
                "sessionId": session_id,
                "elapsedSeconds": elapsed,
                "billedMinutes": billed_minutes,
            }));
        }
    });

    // Create execution run
    let execution_run_id = app_handle
        .db(|db| {
            crate::repository::automation_execution_repository::create_execution_run(
                db,
                automation_id,
                additional_instructions.clone(),
            )
        })
        .map_err(|e| format!("Failed to create execution run: {}", e))?;

    *CURRENT_EXECUTION_RUN_ID.lock().unwrap() = Some(execution_run_id);
    STEP_NUMBER.store(0, std::sync::atomic::Ordering::SeqCst);

    // Initialize log file
    init_automation_log_file(app_handle, automation_id, &automation.name)?;
    log_and_file("INFO", &format!("Starting AGENT MODE execution: {} (ID: {})", automation.name, automation_id));

    // Get installed apps for orchestrator context
    let installed_apps = get_installed_apps();

    // Capture current UI context before planning
    let initial_ui_context = {
        let app_state = APP_STATE.lock().unwrap().clone();
        match app_state {
            Some(state) => {
                let mut ctx = String::new();
                if let Some(app) = &state.current_app {
                    ctx.push_str(&format!("Current App: {}\n", app));
                }
                if !state.accessible_elements.is_empty() {
                    ctx.push_str("Key UI Elements:\n");
                    for (i, el) in state.accessible_elements.iter().take(15).enumerate() {
                        ctx.push_str(&format!("{}. {}\n", i + 1, el));
                    }
                    if state.accessible_elements.len() > 15 {
                        ctx.push_str(&format!("... and {} more elements\n", state.accessible_elements.len() - 15));
                    }
                }
                if ctx.is_empty() { None } else { Some(ctx) }
            },
            None => None
        }
    };

    // Get initial plan from orchestrator
    log_and_file("INFO", "Requesting initial plan from orchestrator...");

    let _initial_phase = match agent_mode_engine::get_initial_plan(
        app_handle,
        &automation.objective,
        additional_instructions.as_deref(),
        &installed_apps,
        initial_ui_context.as_deref(),
    ).await {
        Ok(phase) => {
            log_and_file("INFO", "========== ORCHESTRATOR PHASE PLAN ==========");
            log_and_file("INFO", &format!("Phase {}: {}", phase.number, phase.name));
            log_and_file("INFO", &format!("Goal: {}", phase.goal));
            log_and_file("INFO", &format!("Steps ({}):", phase.steps.len()));
            for (i, step) in phase.steps.iter().enumerate() {
                log_and_file("INFO", &format!("  {}. {}", i + 1, step));
            }
            if !phase.next_phase_hint.is_empty() {
                log_and_file("INFO", &format!("Next phase hint: {}", phase.next_phase_hint));
            }
            log_and_file("INFO", "==============================================");

            let _ = app_handle.emit("agent_phase_started", serde_json::json!({
                "phaseName": phase.name,
                "phaseNumber": phase.number,
                "goal": phase.goal,
                "steps": phase.steps,
                "nextPhaseHint": phase.next_phase_hint,
            }));

            phase
        },
        Err(e) if e.starts_with("DIRECT_RESPONSE:") => {
            // The orchestrator decided this prompt didn't need an automation —
            // greeting, factual question, etc. The reply is in the error string.
            // We complete the run cleanly and surface the reply as a completion
            // message so the chat shows it as a quick reply, not a failure.
            let response_body = e.strip_prefix("DIRECT_RESPONSE:").unwrap_or("").to_string();
            log_and_file("INFO", &format!("Direct response (no execution needed): {}", &response_body[..std::cmp::min(200, response_body.len())]));

            if let Some(run_id) = *CURRENT_EXECUTION_RUN_ID.lock().unwrap() {
                let _ = app_handle.db(|db| {
                    crate::repository::automation_execution_repository::create_execution_step_with_type(
                        db, run_id, 1,
                        Some(response_body.clone()),
                        None, "completed", "direct_response",
                    )
                });
            }
            finalize_execution_run(app_handle, "completed", None);

            let _ = app_handle.emit("automation_completed", serde_json::json!({
                "automationId": automation.id,
                "completionMessage": response_body.clone(),
                "directResponse": true,
            }));
            // Also emit the standard completion-message event so the chat
            // sees it via its existing listener and renders a quick_reply
            // bubble instead of nothing.
            let _ = app_handle.emit("automation_completion_message", serde_json::json!({
                "automationId": automation.id,
                "message": response_body,
                "directResponse": true,
            }));
            return Ok(());
        },
        Err(e) => {
            log_and_file("ERROR", &format!("Orchestrator failed to create initial plan: {}", e));
            finalize_execution_run(app_handle, "failed", Some(e.clone()));
            return Err(e);
        }
    };

    // Create execution state
    let current_time = chrono::Utc::now().to_rfc3339();
    let execution_state = ExecutionState {
        automation_id: automation.id,
        name: automation.name.clone(),
        objective: automation.objective.clone(),
        status: "In progress".to_string(),
        started_at: current_time.clone(),
        milestones: Vec::new(),
    };

    *AUTOMATION_STATE.lock().unwrap() = Some(execution_state.clone());

    // Initialize app state
    *APP_STATE.lock().unwrap() = Some(AppState {
        current_app: None,
        current_pid: None,
        accessible_elements: Vec::new(),
        text_content: None,
        full_text_content: None,
        word_document_info: None,
        excel_sheet_info: None,
        recent_actions: Vec::new(),
        element_tree: None,
        content_map: Vec::new(),
        action_history_summary: None,
    });

    // Save run ID before the loop
    let saved_run_id = *CURRENT_EXECUTION_RUN_ID.lock().unwrap();

    // Execute the agent mode reactive loop
    let execution_result = execute_agent_mode_loop(
        app_handle,
        &execution_state,
        &automation.objective,
    ).await;

    // Handle completion - cleanup usage session
    if let Some(session_id) = *CURRENT_USAGE_SESSION_ID.lock().unwrap() {
        match &execution_result {
            Ok(_) => {
                if let Err(e) = app_handle.db(|db| {
                    crate::repository::usage_session_repository::complete_usage_session(db, session_id)
                }) {
                    error!("Failed to complete usage session: {}", e);
                }
            },
            Err(_) => {
                if let Err(e) = app_handle.db(|db| {
                    crate::repository::usage_session_repository::cancel_usage_session(db, session_id)
                }) {
                    error!("Failed to cancel usage session: {}", e);
                }
            }
        }
    }

    // Cleanup
    *CURRENT_USAGE_SESSION_ID.lock().unwrap() = None;
    *SESSION_START_TIME.lock().unwrap() = None;
    session_update_task.abort();

    if let Err(ref e) = execution_result {
        finalize_execution_run(app_handle, "failed", Some(e.clone()));
    }

    // Get memory contents for final response
    let memory_contents = memory_manager::get_memory_contents();
    let old_memory = MEMORY_STORE.lock().unwrap().clone();
    let combined_memory = if !memory_contents.is_empty() && !old_memory.is_empty() {
        format!("{}\n---\n{}", memory_contents, old_memory)
    } else if !memory_contents.is_empty() {
        memory_contents.clone()
    } else {
        old_memory.clone()
    };

    // Generate completion message and extract data
    if execution_result.is_ok() {
        let automation = app_handle
            .db(|db| automation_repository::get_automation_by_id(db, automation_id))
            .ok()
            .flatten();

        if let Some(automation) = automation {
            let recent_actions = {
                let app_guard = APP_STATE.lock().unwrap();
                if let Some(ref state) = *app_guard {
                    state.recent_actions.iter().rev()
                        .enumerate()
                        .take(10)
                        .map(|(idx, act)| format!("{}. {} - {}", idx + 1, act.command, act.justification))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    "No actions recorded".to_string()
                }
            };

            // Generate LLM completion message
            let completion_prompt = automation_completion_prompt::get_completion_prompt(
                &automation.objective,
                &combined_memory,
                &recent_actions,
            );

            let api_choice = app_handle
                .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
                .setting_value;

            let api_key = match api_choice.as_str() {
                "openai" => app_handle.db(|db| get_setting(db, "api_key_open_ai").expect("Failed to get OpenAI API key")).setting_value,
                "grok" => app_handle.db(|db| get_setting(db, "api_key_grok").expect("Failed to get Grok API key")).setting_value,
                "gemini" => app_handle.db(|db| get_setting(db, "api_key_gemini").expect("Failed to get Gemini API key")).setting_value,
                "proxy" => {
                    let user_id = app_handle.db(|db| match get_setting(db, "user_id") {
                        Ok(setting) => Some(setting.setting_value),
                        Err(_) => None,
                    }).unwrap_or_default();
                    app_handle.db(|db| crate::repository::user_auth_repository::get_valid_auth_token(db, &user_id))
                        .ok().flatten().unwrap_or_default()
                },
                "claude-subscription" => app_handle.db(|db| get_setting(db, "api_key_claude_oauth").map(|s| s.setting_value).unwrap_or_default()),
                "openai-codex" => crate::auth::openai_codex_oauth::load(&app_handle).map(|c| c.access).unwrap_or_default(),
                "claude" | _ => app_handle.db(|db| get_setting(db, "api_key_claude").expect("Failed to get Claude API key")).setting_value,
            };

            let completion_message = match api_choice.as_str() {
                "proxy" => crate::engine::llm_providers::proxy::call_llm_api(&api_key, completion_prompt.clone(), "", 1000).await,
                "openai" => crate::engine::llm_providers::openai::call_llm_api(&api_key, completion_prompt.clone(), "", 1000).await,
                "grok" => crate::engine::llm_providers::grok::call_llm_api(&api_key, completion_prompt.clone(), "", 1000).await,
                "gemini" => crate::engine::llm_providers::gemini::call_llm_api(&api_key, completion_prompt.clone(), "", 1000).await,
                "claude" | _ => crate::engine::llm_providers::claude::call_llm_api(&api_key, completion_prompt.clone(), "", 1000).await,
            }.map(|(msg, _, _)| msg.trim().to_string())
            .unwrap_or_else(|_| "Agent mode automation completed successfully.".to_string());

            info!("Agent mode completion message: {}", completion_message);

            // Save completion message and memory to DB
            if let Some(run_id) = saved_run_id {
                let _ = app_handle.db(|db| {
                    crate::repository::automation_execution_repository::update_execution_run_completion_message(
                        db, run_id, &completion_message,
                    )
                });
                let _ = app_handle.db(|db| {
                    crate::repository::automation_execution_repository::update_execution_run_status_with_clipboard(
                        db, run_id, "completed", None, if combined_memory.is_empty() { None } else { Some(combined_memory.clone()) }
                    )
                });

                // Extract and store structured data from memory
                let store_task_data = app_handle
                    .db(|db| match get_setting(db, "store_task_data") {
                        Ok(setting) => setting.setting_value != "false",
                        Err(_) => true,
                    });

                if store_task_data {
                    match extract_and_store_task_data(
                        app_handle, automation_id, run_id,
                        &automation.objective, &combined_memory, &recent_actions,
                        &api_choice, &api_key,
                    ).await {
                        Ok(ids) => {
                            if !ids.is_empty() {
                                info!("Agent mode: extracted and stored {} data records", ids.len());
                            }
                        },
                        Err(e) => warn!("Agent mode data extraction failed (non-fatal): {}", e),
                    }
                }
            }

            let _ = app_handle.emit("automation_completion_message", serde_json::json!({
                "message": completion_message,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }));
        }
    }

    // Emit agent-specific completion events
    let _ = app_handle.emit("agent_mode_completed", serde_json::json!({
        "automationId": automation_id,
        "memory": combined_memory,
        "phaseHistory": agent_mode_engine::get_phase_history(),
    }));

    let _ = app_handle.emit("automation_stopped", serde_json::json!({
        "clipboard": if combined_memory.is_empty() { None } else { Some(combined_memory) },
        "agentMode": true,
    }));

    // Cleanup agent mode state
    agent_mode_engine::cleanup_agent_mode();

    *AUTOMATION_STATE.lock().unwrap() = None;
    *APP_STATE.lock().unwrap() = None;
    *LLM_SESSION.lock().unwrap() = None;

    close_automation_log_file();

    info!("Agent mode automation {} completed", automation_id);

    execution_result
}

/// The main agent mode execution loop
/// Runs phases, handles PLAN commands, and communicates with orchestrator
async fn execute_agent_mode_loop(
    app_handle: &AppHandle,
    execution_state: &ExecutionState,
    objective: &str,
) -> Result<(), String> {
    let mut last_app_state: Option<AppState> = None;
    let mut last_action_result: Option<String> = None;

    // Initialize LLM session with agent mode executor prompt
    {
        let session_id = format!("agent_mode_{}_{}", execution_state.automation_id, chrono::Utc::now().to_rfc3339());
        let system_prompt = agent_mode_engine::get_executor_system_prompt(objective);
        let initial_context = "Ready to begin agent mode execution. Following the current phase plan.".to_string();

        let session = LLMSession::new(session_id, system_prompt, initial_context);
        *LLM_SESSION.lock().unwrap() = Some(session);
    }

    loop {
        log_and_file("DEBUG", "=== Agent mode loop iteration starting ===");

        // Check stop flag
        if *SHOULD_STOP_EXECUTION.lock().unwrap() {
            info!("Agent mode execution stopped by user");
            finalize_execution_run(app_handle, "stopped", None);
            return Ok(());
        }

        // Get current execution state
        let current_state = match AUTOMATION_STATE.lock().unwrap().clone() {
            Some(state) => state,
            None => {
                info!("Execution state removed - automation complete");
                finalize_execution_run(app_handle, "completed", None);
                return Ok(());
            }
        };

        // Get/update app state
        let app_state = if let Some(ref cached) = last_app_state {
            log_and_file("DEBUG", "Using cached app state");
            cached.clone()
        } else {
            log_and_file("DEBUG", "Fetching fresh app state...");
            match update_app_state().await {
                Ok(state) => state,
                Err(e) => {
                    log_and_file("ERROR", &format!("Failed to update app state: {}", e));
                    return Err(e);
                }
            }
        };
        last_app_state = None;
        log_and_file("DEBUG", &format!("App state ready: {} elements", app_state.accessible_elements.len()));

        // Check for pending user message and inject into LLM session
        if let Some(user_msg) = PENDING_USER_MESSAGE.lock().unwrap().take() {
            log_and_file("INFO", &format!("Injecting user message into agent session: {}", user_msg));
            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
            USER_MESSAGES.lock().unwrap().push(format!("[{}] {}", timestamp, user_msg));
            if let Some(session) = LLM_SESSION.lock().unwrap().as_mut() {
                session.add_user_message(format!(
                    "⚠️ IMPORTANT: The user sent you a message while you were executing:\n\n\
                    \"{}\"\n\n\
                    STOP and re-evaluate your current approach. The user may be correcting you, \
                    giving new instructions, or changing the goal. Adjust your next action accordingly. \
                    Do NOT continue with your previous plan if it conflicts with this message.\n\n\
                    If the user's message requires a different approach or changes the goal, \
                    issue: PLAN || User correction: <summary of what user wants> || User sent mid-execution message",
                    user_msg
                ));
            }
        }

        // Get phase info for context
        let (phase_name, _phase_goal) = agent_mode_engine::get_current_phase_info()
            .map(|(name, _, goal, _, _)| (name, goal))
            .unwrap_or(("Unknown".to_string(), "Complete objective".to_string()));

        // Decide next action (calls the executor LLM)
        let action = decide_next_action(app_handle, &current_state, &app_state, last_action_result.as_deref()).await?;
        last_action_result = None;

        // Check if executor requested PLAN (orchestrator intervention)
        if agent_mode_engine::is_plan_command(&action.reasoning) {
            log_and_file("INFO", &format!("Executor requested PLAN: {}", action.reasoning));

            if let Some(plan_request) = agent_mode_engine::parse_executor_plan_request(&action.reasoning) {
                // Track PLAN calls to detect loops
                if let Err(loop_msg) = agent_mode_engine::track_plan_call(&plan_request.reason, &plan_request.progress_summary) {
                    log_and_file("ERROR", &loop_msg);
                    let _ = app_handle.emit("agent_mode_error", serde_json::json!({
                        "error": loop_msg,
                        "errorType": "plan_loop_detected"
                    }));
                    finalize_execution_run(app_handle, "failed", Some(loop_msg));
                    return Err("Agent stuck in PLAN loop - stopping execution".to_string());
                }

                // Emit phase progress event
                let _ = app_handle.emit("agent_phase_progress", serde_json::json!({
                    "phaseName": phase_name,
                    "progress": plan_request.progress_summary,
                    "reason": plan_request.reason,
                }));

                // Get action summary for orchestrator
                let action_summary = agent_mode_engine::summarize_recent_actions(
                    &app_state.recent_actions,
                    30
                );

                // Build UI context for orchestrator
                let ui_context = {
                    let mut ctx = String::new();
                    if let Some(app) = &app_state.current_app {
                        ctx.push_str(&format!("Current App: {}\n", app));
                    }
                    if !app_state.accessible_elements.is_empty() {
                        ctx.push_str("Key UI Elements:\n");
                        for (i, el) in app_state.accessible_elements.iter().take(20).enumerate() {
                            ctx.push_str(&format!("{}. {}\n", i + 1, el));
                        }
                        if app_state.accessible_elements.len() > 20 {
                            ctx.push_str(&format!("... and {} more elements\n", app_state.accessible_elements.len() - 20));
                        }
                    }
                    if ctx.is_empty() { None } else { Some(ctx) }
                };

                // Request next phase from orchestrator
                match agent_mode_engine::request_next_phase(
                    app_handle,
                    objective,
                    &plan_request,
                    &action_summary,
                    ui_context.as_deref(),
                ).await {
                    Ok(decision) => {
                        match decision {
                            crate::engine::orchestrator_prompt::OrchestratorDecision::ContinueNextPhase {
                                phase_name, phase_number, goal, steps, ..
                            } => {
                                log_and_file("INFO", &format!("========== ORCHESTRATOR: NEW PHASE =========="));
                                log_and_file("INFO", &format!("Phase {}: {}", phase_number, phase_name));
                                log_and_file("INFO", &format!("Goal: {}", goal));
                                log_and_file("INFO", &format!("Steps ({}):", steps.len()));
                                for (i, step) in steps.iter().enumerate() {
                                    log_and_file("INFO", &format!("  {}. {}", i + 1, step));
                                }
                                log_and_file("INFO", "==============================================");

                                let _ = app_handle.emit("agent_phase_started", serde_json::json!({
                                    "phaseName": phase_name,
                                    "phaseNumber": phase_number,
                                    "goal": goal,
                                    "steps": steps,
                                }));

                                // Update executor system prompt for new phase
                                let new_system_prompt = agent_mode_engine::get_executor_system_prompt(objective);
                                if let Some(session) = LLM_SESSION.lock().unwrap().as_mut() {
                                    session.messages[0].content = new_system_prompt;
                                }

                                agent_mode_engine::clear_plan_history();
                            },
                            crate::engine::orchestrator_prompt::OrchestratorDecision::Complete { summary } => {
                                log_and_file("INFO", &format!("Orchestrator: Task complete - {}", summary));

                                let _ = app_handle.emit("agent_phase_completed", serde_json::json!({
                                    "summary": summary,
                                    "complete": true,
                                }));

                                finalize_execution_run(app_handle, "completed", None);
                                return Ok(());
                            },
                            crate::engine::orchestrator_prompt::OrchestratorDecision::DirectResponse { response, .. } => {
                                log_and_file("INFO", &format!("Orchestrator emitted DirectResponse: {}", &response[..std::cmp::min(100, response.len())]));

                                let _ = app_handle.emit("automation_completion_message", serde_json::json!({
                                    "message": response,
                                }));

                                finalize_execution_run(app_handle, "completed", None);
                                return Ok(());
                            },
                            crate::engine::orchestrator_prompt::OrchestratorDecision::ParseError { raw_response } => {
                                warn!("Failed to parse orchestrator response, continuing: {}",
                                    &raw_response[..std::cmp::min(100, raw_response.len())]);
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to get next phase from orchestrator: {}", e);
                    }
                }

                continue; // Skip normal action execution, get next decision
            }
        }

        // Handle sectioned memory commands (agent mode specific)
        if crate::engine::automation_agent_agentic_prompt::is_sectioned_memory_command(&format!("{:?}", action.action_type)) ||
           action.reasoning.to_uppercase().contains("MEMORY_SAVE:") ||
           action.reasoning.to_uppercase().contains("MEMORY_REVISE:") ||
           action.action_type == ActionType::MemorySave {
            // Try to extract memory command from reasoning or action
            let memory_cmd = if action.action_type == ActionType::MemorySave {
                action.parameters.as_ref()
                    .and_then(|p| p.get("memory"))
                    .cloned()
            } else {
                None
            };

            if let Some(cmd) = memory_cmd {
                log_and_file("INFO", &format!("Processing MEMORY_SAVE: {}...", &cmd[..std::cmp::min(50, cmd.len())]));

                match agent_mode_engine::handle_memory_command(&cmd) {
                    Ok(msg) => log_and_file("INFO", &format!("Memory: {}", msg)),
                    Err(e) => {
                        log_and_file("WARN", &format!("Structured memory failed: {}, using simple append", e));
                        let _ = memory_manager::save_to_memory(None, &cmd);
                    }
                }

                // Record in recent actions
                let updated_state = {
                    let mut state_guard = APP_STATE.lock().unwrap();
                    if let Some(ref mut state) = *state_guard {
                        let enhanced_action = EnhancedAction {
                            command: format!("MEMORY_SAVE:{}", &cmd[..std::cmp::min(100, cmd.len())]),
                            action_type: "MemorySave".to_string(),
                            justification: action.reasoning.clone(),
                            screen_context: summarize_current_screen(state),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        };
                        state.recent_actions.push(enhanced_action);
                    }
                    state_guard.clone()
                };
                last_app_state = updated_state;
            }
            continue;
        }

        // Check stop flag after LLM response
        if *SHOULD_STOP_EXECUTION.lock().unwrap() {
            info!("Agent mode stopped after LLM response");
            finalize_execution_run(app_handle, "stopped", None);
            return Ok(());
        }

        // Record step in database
        let step_num = STEP_NUMBER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(execution_run_id) = *CURRENT_EXECUTION_RUN_ID.lock().unwrap() {
            let _ = app_handle.db(|db| {
                crate::repository::automation_execution_repository::create_execution_step(
                    db,
                    execution_run_id,
                    step_num,
                    Some(action.reasoning.clone()),
                    None,
                    "executing",
                )
            });

            let _ = app_handle.emit("automation_step", serde_json::json!({
                "automationId": execution_state.automation_id,
                "explanation": action.reasoning,
                "agentMode": true,
                "phaseName": phase_name,
            }));
        }

        // Hard step limit - prevent infinite execution
        if step_num >= MAX_AGENT_STEPS {
            log_and_file("WARN", &format!("Hard step limit ({}) reached - forcing completion", MAX_AGENT_STEPS));
            let _ = app_handle.emit("agent_phase_completed", serde_json::json!({
                "summary": format!("Execution stopped after {} steps (safety limit)", step_num),
                "complete": true,
            }));
            finalize_execution_run(app_handle, "completed", None);
            return Ok(());
        }

        // Periodic orchestrator review
        if step_num > 0 && step_num % STEPS_PER_REVIEW == 0 {
            log_and_file("INFO", &format!("Periodic review at step {} - checking with orchestrator", step_num));

            let review_action_summary = agent_mode_engine::summarize_recent_actions(&app_state.recent_actions, 30);
            let review_request = crate::engine::automation_agent_agentic_prompt::PlanRequest {
                progress_summary: format!("Periodic review after {} steps", step_num),
                reason: format!("Automatic checkpoint at step {} - is execution on track or stuck in a loop?", step_num),
            };

            let review_ui_context = {
                let mut ctx = String::new();
                if let Some(app) = &app_state.current_app {
                    ctx.push_str(&format!("Current App: {}\n", app));
                }
                if !app_state.accessible_elements.is_empty() {
                    ctx.push_str("Key UI Elements:\n");
                    for (i, el) in app_state.accessible_elements.iter().take(20).enumerate() {
                        ctx.push_str(&format!("{}. {}\n", i + 1, el));
                    }
                }
                if ctx.is_empty() { None } else { Some(ctx) }
            };

            match agent_mode_engine::request_next_phase(
                app_handle, objective, &review_request, &review_action_summary, review_ui_context.as_deref(),
            ).await {
                Ok(decision) => {
                    match decision {
                        crate::engine::orchestrator_prompt::OrchestratorDecision::Complete { summary } => {
                            log_and_file("INFO", &format!("Periodic review: orchestrator says COMPLETE - {}", summary));
                            let _ = app_handle.emit("agent_phase_completed", serde_json::json!({
                                "summary": summary,
                                "complete": true,
                            }));
                            finalize_execution_run(app_handle, "completed", None);
                            return Ok(());
                        },
                        crate::engine::orchestrator_prompt::OrchestratorDecision::ContinueNextPhase {
                            phase_name: new_phase, phase_number, goal, steps, ..
                        } => {
                            log_and_file("INFO", &format!("Periodic review: orchestrator revised plan -> Phase {}: {}", phase_number, new_phase));
                            let _ = app_handle.emit("agent_phase_started", serde_json::json!({
                                "phaseName": new_phase,
                                "phaseNumber": phase_number,
                                "goal": goal,
                                "steps": steps,
                                "periodicReview": true,
                            }));
                            let new_system_prompt = agent_mode_engine::get_executor_system_prompt(objective);
                            if let Some(session) = LLM_SESSION.lock().unwrap().as_mut() {
                                session.messages[0].content = new_system_prompt;
                            }
                            agent_mode_engine::clear_plan_history();
                            continue;
                        },
                        _ => {
                            log_and_file("INFO", "Periodic review: orchestrator says continue as-is");
                        }
                    }
                },
                Err(e) => {
                    warn!("Periodic review failed, continuing: {}", e);
                }
            }
        }

        // Handle COMPLETE action - in agent mode, check with orchestrator first
        if action.action_type == ActionType::Complete {
            log_and_file("INFO", "Executor signaled COMPLETE - checking with orchestrator");

            let action_summary = agent_mode_engine::summarize_recent_actions(&app_state.recent_actions, 30);
            let plan_request = crate::engine::automation_agent_agentic_prompt::PlanRequest {
                progress_summary: "Phase execution complete".to_string(),
                reason: action.reasoning.clone(),
            };

            let ui_context = app_state.current_app.as_ref().map(|app| format!("Current App: {}", app));

            match agent_mode_engine::request_next_phase(app_handle, objective, &plan_request, &action_summary, ui_context.as_deref()).await {
                Ok(crate::engine::orchestrator_prompt::OrchestratorDecision::Complete { summary }) => {
                    log_and_file("INFO", &format!("Task fully complete: {}", summary));
                    finalize_execution_run(app_handle, "completed", None);
                    return Ok(());
                },
                Ok(_) => {
                    continue;
                },
                Err(e) => {
                    warn!("Orchestrator check failed, treating as complete: {}", e);
                    finalize_execution_run(app_handle, "completed", None);
                    return Ok(());
                }
            }
        }

        // Handle FULL_TEXT specially for agent mode
        if action.action_type == ActionType::FullText {
            log_and_file("INFO", "Agent mode: Processing FULL_TEXT request");

            let relaxed_full_text = if let Some(ref pid) = app_state.current_pid {
                #[cfg(target_os = "macos")]
                {
                    use crate::window_details_collector::macos::macos_action_detector_engine::{get_full_text_content_by_pid, get_full_text_via_clipboard};
                    match get_full_text_via_clipboard(pid) {
                        Ok(text) => {
                            info!("FULL_TEXT: Got {} chars from clipboard extraction", text.len());
                            text
                        }
                        Err(e) => {
                            info!("FULL_TEXT: Clipboard extraction failed ({}), falling back to AX scan", e);
                            let ax_text = get_full_text_content_by_pid(pid);
                            info!("FULL_TEXT: Got {} chars from AX scan fallback", ax_text.len());
                            ax_text
                        }
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    app_state.full_text_content.clone().unwrap_or_default()
                }
            } else {
                app_state.full_text_content.clone().unwrap_or_default()
            };

            log_and_file("INFO", &format!("FULL_TEXT collected {} chars", relaxed_full_text.len()));

            {
                let mut state_guard = APP_STATE.lock().unwrap();
                if let Some(ref mut state) = *state_guard {
                    let enhanced_action = EnhancedAction {
                        command: "FULL_TEXT".to_string(),
                        action_type: "FullText".to_string(),
                        justification: action.reasoning.clone(),
                        screen_context: summarize_current_screen(state),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    state.recent_actions.push(enhanced_action);
                    state.full_text_content = Some(relaxed_full_text.clone());
                }
            }

            let text_preview = if relaxed_full_text.len() > 400000 {
                format!("{}...\n[Text truncated - {} total chars]", &relaxed_full_text[..400000], relaxed_full_text.len())
            } else {
                relaxed_full_text.clone()
            };
            last_action_result = Some(format!(
                "FULL TEXT CONTENT ({} chars):\n{}",
                relaxed_full_text.len(),
                text_preview
            ));
            last_app_state = APP_STATE.lock().unwrap().clone();
            continue;
        }

        // Normal action execution with verification
        info!("Executing action: {:?}", action);

        let verification_result = execute_action_with_verification(
            app_handle,
            &action,
            &app_state
        ).await;

        match verification_result {
            Ok((new_app_state, verification_msg)) => {
                last_app_state = new_app_state;

                if let Some(ref msg) = verification_msg {
                    let _ = memory_manager::add_verification_result(msg);
                    log_and_file("INFO", &format!("Action verification: {}", msg));
                }
                last_action_result = verification_msg;
            },
            Err(e) => {
                warn!("Action execution failed: {}", e);
                let failure_msg = format!("Action FAILED: {}", e);
                let _ = memory_manager::add_verification_result(&failure_msg);
                last_action_result = Some(failure_msg);
            }
        }

        // Small delay between actions
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Execute action and verify the result by checking UI state changes
async fn execute_action_with_verification(
    app_handle: &AppHandle,
    action: &AgentAction,
    pre_state: &AppState,
) -> Result<(Option<AppState>, Option<String>), String> {
    let pre_elements_count = pre_state.accessible_elements.len();
    let pre_app = pre_state.current_app.clone().unwrap_or_default();

    // Execute the action
    let action_result = execute_action(app_handle, action).await?;

    // Wait briefly for UI to update
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Capture new state
    let post_state = update_app_state().await?;

    // Record action in recent actions (with auto-compression)
    {
        let mut state_opt = APP_STATE.lock().unwrap();
        if let Some(mut state) = state_opt.clone() {
            // Build descriptive command string (matching task mode format)
            let (cmd_str, action_type_str) = match action.action_type {
                ActionType::ClickElement => {
                    let target = action.target_element.as_deref().unwrap_or("?");
                    (format!("CLICK:{}", target), "ClickElement".to_string())
                },
                ActionType::TypeText => {
                    let text = action.parameters.as_ref()
                        .and_then(|p| p.get("text"))
                        .map(|t| t.as_str())
                        .unwrap_or("");
                    let truncated = if text.len() > 20 { &text[..20] } else { text };
                    (format!("TYPE:'{}'", truncated), "TypeText".to_string())
                },
                ActionType::NavigateURL => {
                    let url = action.parameters.as_ref()
                        .and_then(|p| p.get("url"))
                        .map(|u| u.as_str())
                        .unwrap_or("?");
                    (format!("NAVIGATE:{}", url), "NavigateURL".to_string())
                },
                ActionType::LaunchApp => {
                    let app = action.app_name.as_deref().unwrap_or("?");
                    (format!("LAUNCH:{}", app), "LaunchApp".to_string())
                },
                ActionType::PressKey => {
                    let key = action.parameters.as_ref()
                        .and_then(|p| p.get("key"))
                        .map(|k| k.as_str())
                        .unwrap_or("?");
                    (format!("KEY:{}", key), "PressKey".to_string())
                },
                ActionType::WordCommand => {
                    let cmd = action.parameters.as_ref()
                        .and_then(|p| p.get("command"))
                        .map(|c| c.as_str())
                        .unwrap_or("UNKNOWN");
                    (cmd.to_string(), "WordCommand".to_string())
                },
                ActionType::ExcelCommand => {
                    let cmd = action.parameters.as_ref()
                        .and_then(|p| p.get("command"))
                        .map(|c| c.as_str())
                        .unwrap_or("UNKNOWN");
                    (cmd.to_string(), "ExcelCommand".to_string())
                },
                ActionType::ExcelType => {
                    let cell_data = action.parameters.as_ref()
                        .and_then(|p| p.get("cell_data"))
                        .map(|d| d.as_str())
                        .unwrap_or("");
                    let cell_count = cell_data.split(":::").count();
                    (format!("EXCEL_TYPE:{} cells", cell_count), "ExcelType".to_string())
                },
                ActionType::PowerPointCommand => {
                    let cmd = action.parameters.as_ref()
                        .and_then(|p| p.get("command"))
                        .map(|c| c.as_str())
                        .unwrap_or("UNKNOWN");
                    (cmd.to_string(), "PowerPointCommand".to_string())
                },
                ActionType::TerminalRun => {
                    let cmd = action.parameters.as_ref()
                        .and_then(|p| p.get("command"))
                        .map(|c| c.as_str())
                        .unwrap_or("?");
                    (format!("TERMINAL:{}", cmd), "TerminalRun".to_string())
                },
                ActionType::GoogleSearch => {
                    let query = action.parameters.as_ref()
                        .and_then(|p| p.get("query"))
                        .map(|q| q.as_str())
                        .unwrap_or("?");
                    (format!("SEARCH:{}", query), "GoogleSearch".to_string())
                },
                ActionType::WaitTime => {
                    let wait = action.parameters.as_ref()
                        .and_then(|p| p.get("wait_time"))
                        .map(|w| w.as_str())
                        .unwrap_or("?");
                    (format!("WAIT:{}s", wait), "WaitTime".to_string())
                },
                _ => {
                    (format!("{:?}", action.action_type), format!("{:?}", action.action_type))
                }
            };

            let enhanced_action = EnhancedAction {
                command: cmd_str,
                action_type: action_type_str,
                justification: action.reasoning.clone(),
                screen_context: summarize_current_screen(&state),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            state.add_action(enhanced_action); // Uses add_action for auto-compression
            *state_opt = Some(state);
        }
    }

    // Generate verification message
    let mut verification_msg = generate_verification_message(action, pre_state, &post_state, &pre_app, pre_elements_count);

    // For TerminalRun and FetchPages actions, include the result so the LLM can see what happened.
    // Without this, the LLM is blind to terminal/fetch results and may re-run commands unnecessarily.
    if action.action_type == ActionType::TerminalRun || action.action_type == ActionType::TerminalBackground
        || action.action_type == ActionType::FetchPages || action.action_type == ActionType::WriteFile {
        let combined = if let Some(existing) = verification_msg {
            format!("{}\n{}", existing, action_result)
        } else {
            action_result.clone()
        };
        verification_msg = Some(combined);
    }

    // For ExcelCommand GET operations, include the result so the LLM can see the data.
    // Without this, EXCEL_GET_STRUCTURE etc. results are invisible and the LLM retries endlessly.
    if action.action_type == ActionType::ExcelCommand {
        if let Some(params) = &action.parameters {
            let command = params.get("command").map(|s| s.as_str()).unwrap_or("");
            #[cfg(target_os = "macos")]
            let is_get = crate::engine::app_commands::excel_macos::is_data_retrieval_command(command);
            #[cfg(not(target_os = "macos"))]
            let is_get = false;
            if is_get {
                let combined = if let Some(existing) = verification_msg {
                    format!("{}\n{}", existing, action_result)
                } else {
                    action_result.clone()
                };
                verification_msg = Some(combined);
            }
        }
    }

    let result_msg = verification_msg;

    Ok((Some(post_state), result_msg))
}

/// Generate a verification message describing what changed after an action
fn generate_verification_message(
    action: &AgentAction,
    _pre_state: &AppState,
    post_state: &AppState,
    pre_app: &str,
    pre_elements_count: usize,
) -> Option<String> {
    let post_elements_count = post_state.accessible_elements.len();
    let post_app = post_state.current_app.clone().unwrap_or_default();

    let mut changes = Vec::new();

    // Check app/window change
    if pre_app != post_app && !post_app.is_empty() {
        changes.push(format!("App changed to: {}", post_app));
    }

    // Check element count change (significant UI change)
    let element_diff = (post_elements_count as i32) - (pre_elements_count as i32);
    if element_diff.abs() > 5 {
        if element_diff > 0 {
            changes.push(format!("UI expanded: {} new elements visible", element_diff));
        } else {
            changes.push(format!("UI simplified: {} fewer elements", -element_diff));
        }
    }

    // Action-specific verification
    match action.action_type {
        ActionType::LaunchApp => {
            if let Some(app) = &action.app_name {
                if post_app.to_lowercase().contains(&app.to_lowercase()) {
                    changes.push(format!("{} launched successfully", app));
                }
            }
        },
        ActionType::ClickElement => {
            changes.push("Click executed".to_string());
        },
        ActionType::TypeText => {
            changes.push("Text typed".to_string());
        },
        ActionType::NavigateURL => {
            changes.push("URL navigation initiated".to_string());
        },
        ActionType::PressKey => {
            if let Some(params) = &action.parameters {
                if let Some(key) = params.get("key") {
                    changes.push(format!("Pressed {}", key));
                }
            }
        },
        _ => {}
    }

    if changes.is_empty() {
        None
    } else {
        Some(changes.join("; "))
    }
}
