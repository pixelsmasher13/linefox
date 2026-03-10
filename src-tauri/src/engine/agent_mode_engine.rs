//! Agent Mode Engine
//!
//! This module implements the "true agent mode" where a powerful orchestrator LLM
//! breaks down complex tasks into phases, and an executor LLM carries them out.
//!
//! Flow:
//! 1. Orchestrator analyzes user task and creates initial phase plan
//! 2. Executor runs phase steps, saving to memory
//! 3. Executor calls PLAN when phase complete or stuck
//! 4. Orchestrator reviews progress and creates next phase
//! 5. Repeat until task complete

use log::{info, warn, error};
use tauri::AppHandle;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::configuration::state::ServiceAccess;
use crate::engine::orchestrator_prompt::{
    OrchestratorDecision, get_orchestrator_system_prompt_with_role,
    get_initial_planning_prompt, get_review_prompt, parse_orchestrator_response,
};
use crate::engine::automation_agent_agentic_prompt::{
    get_agentic_system_prompt_with_context, parse_plan_command,
    parse_sectioned_memory_command, is_sectioned_memory_command, PlanRequest,
};
use crate::engine::memory_manager::{
    init_memory, save_to_memory, revise_memory_section,
    get_memory_contents, clear_memory,
};
use crate::engine::types::{EnhancedAction, LLMSession};
use crate::repository::settings_repository::get_setting;

lazy_static::lazy_static! {
    /// Current phase being executed
    static ref CURRENT_PHASE: Mutex<Option<AgentPhase>> = Mutex::new(None);

    /// Phase history for context
    static ref PHASE_HISTORY: Mutex<Vec<PhaseResult>> = Mutex::new(Vec::new());

    /// Orchestrator session (uses powerful model like Claude Sonnet)
    static ref ORCHESTRATOR_SESSION: Mutex<Option<LLMSession>> = Mutex::new(None);

    /// Executor session (uses configured model)
    static ref EXECUTOR_SESSION: Mutex<Option<LLMSession>> = Mutex::new(None);

    /// Track recent PLAN calls to detect loops
    static ref PLAN_CALL_HISTORY: Mutex<Vec<PlanCallRecord>> = Mutex::new(Vec::new());

    /// Active role/persona content for the current agent session
    static ref ACTIVE_ROLE_CONTENT: Mutex<Option<String>> = Mutex::new(None);
}

/// Record of a PLAN call for loop detection
#[derive(Debug, Clone)]
pub struct PlanCallRecord {
    pub reason: String,
    pub progress: String,
    pub timestamp: std::time::Instant,
}

/// Current phase number
static PHASE_NUMBER: AtomicU32 = AtomicU32::new(1);

/// Current step within phase
static PHASE_STEP_NUMBER: AtomicU32 = AtomicU32::new(1);

/// Represents a phase of execution
#[derive(Debug, Clone)]
pub struct AgentPhase {
    pub name: String,
    pub number: u32,
    pub goal: String,
    pub steps: Vec<String>,
    pub memory_instructions: String,
    pub next_phase_hint: String,
    pub completed_steps: Vec<String>,
}

/// Result of a completed phase
#[derive(Debug, Clone)]
pub struct PhaseResult {
    pub phase_name: String,
    pub phase_number: u32,
    pub goal: String,
    pub outcome: PhaseOutcome,
    pub actions_taken: Vec<String>,
    pub memory_at_completion: String,
}

#[derive(Debug, Clone)]
pub enum PhaseOutcome {
    Completed,
    PartiallyCompleted { reason: String },
    Failed { reason: String },
    UserInputRequired { question: String },
}

/// Initialize agent mode for a new automation
pub fn init_agent_mode(objective: &str, active_role_content: Option<String>) {
    // Clear previous state
    *CURRENT_PHASE.lock().unwrap() = None;
    PHASE_HISTORY.lock().unwrap().clear();
    *ORCHESTRATOR_SESSION.lock().unwrap() = None;
    *EXECUTOR_SESSION.lock().unwrap() = None;
    *ACTIVE_ROLE_CONTENT.lock().unwrap() = active_role_content;

    PHASE_NUMBER.store(1, Ordering::SeqCst);
    PHASE_STEP_NUMBER.store(1, Ordering::SeqCst);

    // Initialize structured memory for agent mode
    init_memory(true);

    info!("Agent mode initialized for objective: {}", objective);
}

/// Get initial plan from orchestrator
pub async fn get_initial_plan(
    app_handle: &AppHandle,
    objective: &str,
    additional_instructions: Option<&str>,
    installed_apps: &[String],
    current_ui_context: Option<&str>, // Current app context if user is already in an app
) -> Result<AgentPhase, String> {
    info!("Getting initial plan from orchestrator");

    // Initialize orchestrator session
    let role_content = ACTIVE_ROLE_CONTENT.lock().unwrap().clone();
    let system_prompt = get_orchestrator_system_prompt_with_role(role_content.as_deref());
    let planning_prompt = get_initial_planning_prompt(objective, additional_instructions, installed_apps, current_ui_context);

    // Call orchestrator LLM (always use Claude for orchestration)
    let response = call_orchestrator_llm(app_handle, &system_prompt, &planning_prompt).await?;

    info!("Orchestrator response: {}", &response[..std::cmp::min(500, response.len())]);

    // Parse the response
    match parse_orchestrator_response(&response) {
        OrchestratorDecision::ContinueNextPhase {
            phase_name, phase_number, goal, steps, memory_instructions, next_phase_hint
        } => {
            let phase = AgentPhase {
                name: phase_name,
                number: phase_number,
                goal,
                steps,
                memory_instructions,
                next_phase_hint,
                completed_steps: Vec::new(),
            };

            // Store as current phase
            *CURRENT_PHASE.lock().unwrap() = Some(phase.clone());

            info!("Initial phase: {} with {} steps", phase.name, phase.steps.len());
            Ok(phase)
        },
        OrchestratorDecision::RequestUserInput { question } => {
            Err(format!("Orchestrator needs clarification: {}", question))
        },
        OrchestratorDecision::ParseError { raw_response } => {
            error!("Failed to parse orchestrator response: {}", &raw_response[..std::cmp::min(200, raw_response.len())]);
            Err("Failed to parse orchestrator's plan".to_string())
        },
        _ => {
            Err("Unexpected orchestrator response for initial planning".to_string())
        }
    }
}

/// Request next phase from orchestrator after executor calls PLAN
pub async fn request_next_phase(
    app_handle: &AppHandle,
    original_objective: &str,
    plan_request: &PlanRequest,
    action_summary: &str,
    current_ui_context: Option<&str>, // Current app + key UI elements for context
) -> Result<OrchestratorDecision, String> {
    info!("Requesting next phase from orchestrator");

    let current_phase = CURRENT_PHASE.lock().unwrap().clone();
    let (phase_name, phase_num) = match &current_phase {
        Some(p) => (p.name.clone(), p.number),
        None => ("Unknown".to_string(), 0),
    };

    let memory_contents = get_memory_contents();

    // Build review prompt
    let review_prompt = get_review_prompt(
        original_objective,
        &phase_name,
        phase_num,
        &plan_request.progress_summary,
        &plan_request.reason,
        &memory_contents,
        action_summary,
        current_ui_context,
    );

    // Get orchestrator system prompt with active role
    let role_content = ACTIVE_ROLE_CONTENT.lock().unwrap().clone();
    let system_prompt = get_orchestrator_system_prompt_with_role(role_content.as_deref());

    // Call orchestrator
    let response = call_orchestrator_llm(app_handle, &system_prompt, &review_prompt).await?;

    info!("Orchestrator review response: {}", &response[..std::cmp::min(500, response.len())]);

    // Parse response
    let decision = parse_orchestrator_response(&response);

    // Update state based on decision
    match &decision {
        OrchestratorDecision::ContinueNextPhase {
            phase_name, phase_number, goal, steps, memory_instructions, next_phase_hint
        } => {
            // Record completed phase
            if let Some(completed) = current_phase {
                PHASE_HISTORY.lock().unwrap().push(PhaseResult {
                    phase_name: completed.name,
                    phase_number: completed.number,
                    goal: completed.goal,
                    outcome: PhaseOutcome::Completed,
                    actions_taken: completed.completed_steps,
                    memory_at_completion: memory_contents.clone(),
                });
            }

            // Set new current phase
            let new_phase = AgentPhase {
                name: phase_name.clone(),
                number: *phase_number,
                goal: goal.clone(),
                steps: steps.clone(),
                memory_instructions: memory_instructions.clone(),
                next_phase_hint: next_phase_hint.clone(),
                completed_steps: Vec::new(),
            };
            *CURRENT_PHASE.lock().unwrap() = Some(new_phase);

            PHASE_NUMBER.store(*phase_number, Ordering::SeqCst);
            PHASE_STEP_NUMBER.store(1, Ordering::SeqCst);
        },
        OrchestratorDecision::RetryCurrentPhase { phase_name, phase_number, goal, steps, memory_instructions, reason } => {
            info!("Orchestrator requested retry of current phase: {}", reason);

            // Update current phase with new steps
            let retry_phase = AgentPhase {
                name: phase_name.clone(),
                number: *phase_number,
                goal: goal.clone(),
                steps: steps.clone(),
                memory_instructions: memory_instructions.clone(),
                next_phase_hint: String::new(),
                completed_steps: Vec::new(),
            };
            *CURRENT_PHASE.lock().unwrap() = Some(retry_phase);

            PHASE_STEP_NUMBER.store(1, Ordering::SeqCst);
        },
        OrchestratorDecision::Complete { summary } => {
            info!("Orchestrator marked task as complete: {}", summary);

            // Record final phase
            if let Some(completed) = current_phase {
                PHASE_HISTORY.lock().unwrap().push(PhaseResult {
                    phase_name: completed.name,
                    phase_number: completed.number,
                    goal: completed.goal,
                    outcome: PhaseOutcome::Completed,
                    actions_taken: completed.completed_steps,
                    memory_at_completion: memory_contents,
                });
            }

            *CURRENT_PHASE.lock().unwrap() = None;
        },
        OrchestratorDecision::RequestUserInput { question } => {
            info!("Orchestrator needs user input: {}", question);
        },
        OrchestratorDecision::ParseError { raw_response } => {
            warn!("Failed to parse orchestrator response");
        }
    }

    Ok(decision)
}

/// Create executor system prompt for current phase
/// Note: App-specific commands (Excel, Word, PowerPoint) are added to the incremental prompt,
/// not the system prompt, so the executor sees them dynamically based on current app.
pub fn get_executor_system_prompt(objective: &str) -> String {
    let phase = CURRENT_PHASE.lock().unwrap().clone();
    let role_content = ACTIVE_ROLE_CONTENT.lock().unwrap().clone();
    let role = role_content.as_deref();

    match phase {
        Some(p) => {
            let steps_str = p.steps.iter()
                .enumerate()
                .map(|(i, s)| format!("{}. {}", i + 1, s))
                .collect::<Vec<_>>()
                .join("\n");

            get_agentic_system_prompt_with_context(
                Some(objective),
                Some(&p.name),
                Some(&p.goal),
                Some(&steps_str),
                if p.next_phase_hint.is_empty() { None } else { Some(&p.next_phase_hint) },
                role,
            )
        },
        None => {
            // No active phase - basic prompt
            get_agentic_system_prompt_with_context(Some(objective), None, None, None, None, role)
        }
    }
}

/// Get remaining steps in current phase
pub fn get_remaining_steps() -> Vec<String> {
    let phase = CURRENT_PHASE.lock().unwrap().clone();
    match phase {
        Some(p) => {
            let completed_count = p.completed_steps.len();
            p.steps.iter().skip(completed_count).cloned().collect()
        },
        None => Vec::new(),
    }
}

/// Mark a step as completed in current phase
pub fn mark_step_completed(step_description: &str) {
    if let Some(phase) = CURRENT_PHASE.lock().unwrap().as_mut() {
        phase.completed_steps.push(step_description.to_string());
        PHASE_STEP_NUMBER.fetch_add(1, Ordering::SeqCst);
    }
}

/// Get current phase info for UI display
pub fn get_current_phase_info() -> Option<(String, u32, String, usize, usize)> {
    let phase = CURRENT_PHASE.lock().unwrap().clone();
    phase.map(|p| (
        p.name,
        p.number,
        p.goal,
        p.completed_steps.len(),
        p.steps.len(),
    ))
}

/// Get phase history for UI display
pub fn get_phase_history() -> Vec<(String, u32, bool)> {
    PHASE_HISTORY.lock().unwrap()
        .iter()
        .map(|r| (
            r.phase_name.clone(),
            r.phase_number,
            matches!(r.outcome, PhaseOutcome::Completed),
        ))
        .collect()
}

/// Handle memory commands from executor (sectioned memory support)
pub fn handle_memory_command(command: &str) -> Result<String, String> {
    if !is_sectioned_memory_command(command) {
        // Fall back to simple memory save (for backward compatibility)
        let content = command.strip_prefix("MEMORY_SAVE:").unwrap_or(command);
        save_to_memory(None, content)?;
        return Ok(format!("Saved to memory: {}...", &content[..std::cmp::min(50, content.len())]));
    }

    match parse_sectioned_memory_command(command) {
        Some((operation, section, content)) => {
            if operation == "MEMORY_SAVE" {
                save_to_memory(Some(&section), &content)?;
                Ok(format!("Saved to {}: {}...", section, &content[..std::cmp::min(50, content.len())]))
            } else if operation == "MEMORY_REVISE" {
                revise_memory_section(&section, &content)?;
                Ok(format!("Revised {}: {}...", section, &content[..std::cmp::min(50, content.len())]))
            } else {
                Err(format!("Unknown memory operation: {}", operation))
            }
        },
        None => Err("Failed to parse memory command".to_string()),
    }
}

/// Call orchestrator LLM - uses user's configured API choice (proxy, claude, etc.)
async fn call_orchestrator_llm(
    app_handle: &AppHandle,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    // Get user's API choice setting
    let api_choice = app_handle
        .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
        .setting_value;

    info!("Orchestrator using API choice: {}", api_choice);

    // Get the appropriate API key and call the right provider
    let response = match api_choice.as_str() {
        "proxy" => {
            // Get user ID for proxy authentication
            let user_id = app_handle
                .db(|db| match get_setting(db, "user_id") {
                    Ok(setting) => Some(setting.setting_value),
                    Err(_) => None,
                })
                .unwrap_or_default();

            if user_id.is_empty() {
                return Err("User not authenticated - please login first for proxy mode".to_string());
            }

            // Get valid auth token
            let token = app_handle
                .db(|db| crate::repository::user_auth_repository::get_valid_auth_token(db, &user_id))
                .map_err(|e| format!("Failed to get auth token: {}", e))?
                .ok_or("Authentication expired - please login again")?;

            crate::engine::llm_providers::proxy::call_llm_api(
                &token,
                user_prompt.to_string(),
                system_prompt,
                4000,
            ).await?
        },
        "gemini" => {
            let api_key = app_handle
                .db(|db| get_setting(db, "api_key_gemini").expect("Failed to get Gemini API key"))
                .setting_value;
            if api_key.is_empty() {
                return Err("Gemini API key is not configured".to_string());
            }
            crate::engine::llm_providers::gemini::call_llm_api(
                &api_key,
                user_prompt.to_string(),
                system_prompt,
                4000,
            ).await?
        },
        "openai" => {
            let api_key = app_handle
                .db(|db| get_setting(db, "api_key_open_ai").expect("Failed to get OpenAI API key"))
                .setting_value;
            if api_key.is_empty() {
                return Err("OpenAI API key is not configured".to_string());
            }
            crate::engine::llm_providers::openai::call_llm_api(
                &api_key,
                user_prompt.to_string(),
                system_prompt,
                4000,
            ).await?
        },
        "grok" => {
            let api_key = app_handle
                .db(|db| get_setting(db, "api_key_grok").expect("Failed to get Grok API key"))
                .setting_value;
            if api_key.is_empty() {
                return Err("Grok API key is not configured".to_string());
            }
            crate::engine::llm_providers::grok::call_llm_api(
                &api_key,
                user_prompt.to_string(),
                system_prompt,
                4000,
            ).await?
        },
        "claude" | _ => {
            let api_key = app_handle
                .db(|db| get_setting(db, "api_key_claude").expect("Failed to get Claude API key"))
                .setting_value;
            if api_key.is_empty() {
                return Err("Claude API key is not configured".to_string());
            }
            crate::engine::llm_providers::claude::call_llm_api(
                &api_key,
                user_prompt.to_string(),
                system_prompt,
                4000,
            ).await?
        }
    };

    Ok(response.0) // Return just the response text
}

/// Check if a response from executor is a PLAN command
pub fn is_plan_command(response: &str) -> bool {
    response.trim().to_uppercase().starts_with("PLAN")
}

/// Parse PLAN command from executor response
pub fn parse_executor_plan_request(response: &str) -> Option<PlanRequest> {
    parse_plan_command(response)
}

/// Get summary of recent actions for orchestrator review
pub fn summarize_recent_actions(actions: &[EnhancedAction], max_actions: usize) -> String {
    let recent: Vec<_> = actions.iter().rev().take(max_actions).collect();

    if recent.is_empty() {
        return "No actions taken yet".to_string();
    }

    recent.iter().rev()
        .enumerate()
        .map(|(i, a)| format!("{}. {} - {}", i + 1, a.command, a.justification))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Clean up agent mode state
pub fn cleanup_agent_mode() {
    *CURRENT_PHASE.lock().unwrap() = None;
    PHASE_HISTORY.lock().unwrap().clear();
    *ORCHESTRATOR_SESSION.lock().unwrap() = None;
    *EXECUTOR_SESSION.lock().unwrap() = None;
    PLAN_CALL_HISTORY.lock().unwrap().clear();
    PHASE_NUMBER.store(1, Ordering::SeqCst);
    PHASE_STEP_NUMBER.store(1, Ordering::SeqCst);
    clear_memory();

    info!("Agent mode cleaned up");
}

/// Track a PLAN call and check for loops.
/// Returns Err with message if loop detected (should stop agent).
pub fn track_plan_call(reason: &str, progress: &str) -> Result<(), String> {
    let mut history = PLAN_CALL_HISTORY.lock().unwrap();

    // Add this call
    history.push(PlanCallRecord {
        reason: reason.to_lowercase(),
        progress: progress.to_lowercase(),
        timestamp: std::time::Instant::now(),
    });

    // Keep only last 10 calls
    if history.len() > 10 {
        history.remove(0);
    }

    // Check for repeated PLAN calls with similar reasons (loop detection)
    if history.len() >= 3 {
        let recent: Vec<_> = history.iter().rev().take(3).collect();

        // Check if last 3 reasons are similar (contain same keywords)
        let reasons_similar = recent.windows(2).all(|pair| {
            reasons_are_similar(&pair[0].reason, &pair[1].reason)
        });

        if reasons_similar {
            // Also check timing - if 3 PLAN calls in under 60 seconds, likely stuck
            let time_span = recent[0].timestamp.duration_since(recent[2].timestamp);
            if time_span.as_secs() < 60 {
                let msg = format!(
                    "PLAN loop detected: executor called PLAN 3 times in {}s with similar reason: '{}'",
                    time_span.as_secs(),
                    &recent[0].reason[..std::cmp::min(50, recent[0].reason.len())]
                );
                warn!("{}", msg);
                return Err(msg);
            }
        }
    }

    Ok(())
}

/// Clear PLAN call history - call this when orchestrator provides a new plan
/// This prevents false loop detection after a successful plan transition
pub fn clear_plan_history() {
    PLAN_CALL_HISTORY.lock().unwrap().clear();
    info!("PLAN call history cleared after successful plan transition");
}

/// Check if two PLAN reasons are similar (indicating a loop)
fn reasons_are_similar(r1: &str, r2: &str) -> bool {
    // Simple check: if they share significant keywords
    let keywords1: std::collections::HashSet<_> = r1.split_whitespace()
        .filter(|w| w.len() > 3) // ignore short words
        .collect();
    let keywords2: std::collections::HashSet<_> = r2.split_whitespace()
        .filter(|w| w.len() > 3)
        .collect();

    if keywords1.is_empty() || keywords2.is_empty() {
        return r1 == r2; // fallback to exact match
    }

    // If they share >50% keywords, consider similar
    let common = keywords1.intersection(&keywords2).count();
    let total = keywords1.len().min(keywords2.len());

    common as f32 / total as f32 > 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_plan_command() {
        assert!(is_plan_command("PLAN || progress || reason"));
        assert!(is_plan_command("plan || just reason"));
        assert!(is_plan_command("PLAN"));
        assert!(!is_plan_command("CLICK:5 || clicking button"));
        assert!(!is_plan_command("TYPE:3:hello"));
    }

    #[test]
    fn test_handle_memory_command() {
        init_memory(true);

        let result = handle_memory_command("MEMORY_SAVE:DATA:Apple revenue $100B");
        assert!(result.is_ok());

        let contents = get_memory_contents();
        assert!(contents.contains("Apple revenue"));
    }
}
