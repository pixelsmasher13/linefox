//! Local task scheduler — runs a background loop that checks for due schedules
//! every 30 seconds and executes the corresponding automations.

use chrono::Local;
use log::{info, warn, error};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;
use crate::configuration::state::ServiceAccess;
use crate::repository::{schedule_repository, automation_execution_repository, automation_repository};

/// Global flag: is the scheduler loop running?
static SCHEDULER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Start the background scheduler.  Safe to call multiple times; second+ calls
/// are no-ops while the loop is already alive.
pub fn start_scheduler(app_handle: AppHandle) {
    if SCHEDULER_RUNNING.swap(true, Ordering::SeqCst) {
        info!("[Scheduler] Already running — skipping duplicate start");
        return;
    }

    info!("[Scheduler] Starting local schedule checker (30s interval)");

    // Spawn an async task on the Tauri runtime
    tauri::async_runtime::spawn(async move {
        loop {
            // Sleep 30 seconds between checks
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            if !SCHEDULER_RUNNING.load(Ordering::SeqCst) {
                info!("[Scheduler] Stopped");
                break;
            }

            check_and_run_due_schedules(&app_handle).await;
        }
    });
}

/// Stop the scheduler loop (it will exit on the next iteration).
#[allow(dead_code)]
pub fn stop_scheduler() {
    SCHEDULER_RUNNING.store(false, Ordering::SeqCst);
    info!("[Scheduler] Stop requested");
}

/// Core check: fetch due schedules, execute each one, and advance next_run_at.
async fn check_and_run_due_schedules(app_handle: &AppHandle) {
    let now_iso = chrono::Local::now().to_rfc3339();

    // Fetch due schedules from DB
    let due_schedules = match app_handle.db(|db| schedule_repository::get_due_schedules(db, &now_iso)) {
        Ok(s) => s,
        Err(e) => {
            // Table might not exist yet on first run after migration
            if !e.to_string().contains("no such table") {
                error!("[Scheduler] Failed to query due schedules: {}", e);
            }
            return;
        }
    };

    if due_schedules.is_empty() {
        return;
    }

    info!("[Scheduler] Found {} due schedule(s)", due_schedules.len());

    for schedule in due_schedules {
        info!("[Scheduler] Executing schedule {} (automation_id={})", schedule.id, schedule.automation_id);

        // Mark as ran BEFORE execution so we don't re-trigger if execution is slow
        if let Err(e) = app_handle.db(|db| schedule_repository::mark_schedule_ran(db, schedule.id)) {
            error!("[Scheduler] Failed to mark schedule {} as ran: {}", schedule.id, e);
            continue;
        }

        // Check if another automation is already running
        if crate::engine::automation_agent_engine::is_running() {
            warn!("[Scheduler] Skipping schedule {} — another automation is already running", schedule.id);
            continue;
        }

        // Execute the automation
        let app_clone = app_handle.clone();
        let auto_id = schedule.automation_id;
        let persistent_run_id = schedule.persistent_run_id;

        let custom_prompt = schedule.continuation_prompt;

        tauri::async_runtime::spawn(async move {
            // Check if this is a continuation mode schedule
            if let Some(run_id) = persistent_run_id {
                let today = Local::now().format("%A, %B %-d, %Y").to_string();
                let continuation_prompt = match custom_prompt {
                    Some(ref prompt) if !prompt.trim().is_empty() => {
                        format!("(Today's date is {}). {}", today, prompt.trim())
                    }
                    _ => format!("(Today's date is {}). Continue this run.", today),
                };

                info!("[Scheduler] Running continuation mode for automation {} from run {}", auto_id, run_id);

                // Get the previous run info for context
                let prev_run = match app_clone.db(|db| automation_execution_repository::get_execution_run_by_id(db, run_id)) {
                    Ok(Some(run)) => run,
                    Ok(None) => {
                        error!("[Scheduler] Continuation run {} not found, falling back to fresh execution", run_id);
                        // Fallback to fresh execution
                        match crate::engine::automation_agent_engine::execute_automation(
                            &app_clone,
                            auto_id,
                            None,
                        ).await {
                            Ok(_) => info!("[Scheduler] Schedule execution completed for automation {}", auto_id),
                            Err(e) => error!("[Scheduler] Schedule execution failed for automation {}: {}", auto_id, e),
                        }
                        return;
                    }
                    Err(e) => {
                        error!("[Scheduler] Failed to fetch continuation run {}: {}", run_id, e);
                        return;
                    }
                };

                // Get automation name
                let automation_name = match app_clone.db(|db| automation_repository::get_automation_by_id(db, auto_id)) {
                    Ok(Some(auto)) => auto.name,
                    _ => format!("Automation #{}", auto_id),
                };

                // Build continuation plan from previous run context
                let continuation_plan = format!(
                    "Previous completion: {}\n\nContinue executing this task with today's date context.",
                    prev_run.completion_message.as_deref().unwrap_or("No previous completion message")
                );

                let synthesized_objective = format!(
                    "Continue '{}' - {}",
                    automation_name,
                    continuation_prompt
                );

                match crate::engine::automation_agent_engine::execute_automation_continuation(
                    &app_clone,
                    auto_id,
                    run_id,
                    continuation_plan,
                    format!("{} (Scheduled Continuation)", automation_name),
                    synthesized_objective,
                    continuation_prompt,
                ).await {
                    Ok(_) => info!("[Scheduler] Continuation execution completed for automation {}", auto_id),
                    Err(e) => error!("[Scheduler] Continuation execution failed for automation {}: {}", auto_id, e),
                }
            } else {
                // Fresh execution (same path as manual "Play" button)
                match crate::engine::automation_agent_engine::execute_automation(
                    &app_clone,
                    auto_id,
                    None, // no additional instructions for scheduled runs
                ).await {
                    Ok(_) => info!("[Scheduler] Schedule execution completed for automation {}", auto_id),
                    Err(e) => error!("[Scheduler] Schedule execution failed for automation {}: {}", auto_id, e),
                }
            }
        });

        // Only run one scheduled task at a time — break and let the next cycle pick up others
        break;
    }
}
