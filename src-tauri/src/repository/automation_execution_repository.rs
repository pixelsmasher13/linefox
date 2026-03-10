#![allow(dead_code)]

use rusqlite::{params, Connection, Result};
use log::info;
use chrono::Utc;

use crate::entity::automation_execution::{
    AutomationExecutionRun, AutomationExecutionStep, ExecutionRunWithSteps
};

pub fn create_execution_run(
    db: &Connection,
    automation_id: i64,
    additional_instructions: Option<String>,
) -> Result<i64> {
    let started_at = Utc::now().to_rfc3339();
    
    db.execute(
        "INSERT INTO automation_execution_runs (automation_id, started_at, status, additional_instructions, created_at) 
         VALUES (?1, ?2, 'running', ?3, ?4)",
        params![automation_id, started_at, additional_instructions, started_at],
    )?;
    
    Ok(db.last_insert_rowid())
}

pub fn update_execution_run_status(
    db: &Connection,
    run_id: i64,
    status: &str,
    error_message: Option<String>,
) -> Result<()> {
    let completed_at = if status != "running" {
        Some(Utc::now().to_rfc3339())
    } else {
        None
    };

    db.execute(
        "UPDATE automation_execution_runs
         SET status = ?1, completed_at = ?2, error_message = ?3
         WHERE id = ?4",
        params![status, completed_at, error_message, run_id],
    )?;

    Ok(())
}

pub fn update_execution_run_status_with_clipboard(
    db: &Connection,
    run_id: i64,
    status: &str,
    error_message: Option<String>,
    clipboard: Option<String>,
) -> Result<()> {
    let completed_at = if status != "running" {
        Some(Utc::now().to_rfc3339())
    } else {
        None
    };

    db.execute(
        "UPDATE automation_execution_runs
         SET status = ?1, completed_at = ?2, error_message = ?3, clipboard = ?4
         WHERE id = ?5",
        params![status, completed_at, error_message, clipboard, run_id],
    )?;

    Ok(())
}

pub fn update_execution_run_completion_message(
    db: &Connection,
    run_id: i64,
    completion_message: &str,
) -> Result<()> {
    db.execute(
        "UPDATE automation_execution_runs
         SET completion_message = ?1
         WHERE id = ?2",
        params![completion_message, run_id],
    )?;

    Ok(())
}

pub fn create_execution_step(
    db: &Connection,
    execution_run_id: i64,
    step_number: i32,
    explanation: Option<String>,
    next_step: Option<String>,
    status: &str,
) -> Result<i64> {
    create_execution_step_with_type(db, execution_run_id, step_number, explanation, next_step, status, "action")
}

/// Create an execution step with a specific type
/// step_type: 'action' (default), 'user_prompt', 'completion'
pub fn create_execution_step_with_type(
    db: &Connection,
    execution_run_id: i64,
    step_number: i32,
    explanation: Option<String>,
    next_step: Option<String>,
    status: &str,
    step_type: &str,
) -> Result<i64> {
    let timestamp = Utc::now().to_rfc3339();
    
    db.execute(
        "INSERT INTO automation_execution_steps 
         (execution_run_id, step_number, timestamp, explanation, next_step, status, created_at, step_type) 
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![execution_run_id, step_number, timestamp, explanation, next_step, status, timestamp, step_type],
    )?;
    
    Ok(db.last_insert_rowid())
}

pub fn update_execution_step_status(
    db: &Connection,
    step_id: i64,
    status: &str,
    error_message: Option<String>,
) -> Result<()> {
    db.execute(
        "UPDATE automation_execution_steps 
         SET status = ?1, error_message = ?2 
         WHERE id = ?3",
        params![status, error_message, step_id],
    )?;
    
    Ok(())
}

pub fn get_execution_run_by_id(
    db: &Connection,
    run_id: i64,
) -> Result<Option<AutomationExecutionRun>> {
    let mut stmt = db.prepare(
        "SELECT id, automation_id, started_at, completed_at, status, additional_instructions, error_message, clipboard, completion_message, created_at
         FROM automation_execution_runs
         WHERE id = ?1"
    )?;

    let mut runs = stmt.query_map(params![run_id], |row| {
        Ok(AutomationExecutionRun {
            id: row.get(0)?,
            automation_id: row.get(1)?,
            started_at: row.get(2)?,
            completed_at: row.get(3)?,
            status: row.get(4)?,
            additional_instructions: row.get(5)?,
            error_message: row.get(6)?,
            clipboard: row.get(7)?,
            completion_message: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(runs.into_iter().next())
}

pub fn get_execution_runs_by_automation(
    db: &Connection,
    automation_id: i64,
) -> Result<Vec<AutomationExecutionRun>> {
    let mut stmt = db.prepare(
        "SELECT id, automation_id, started_at, completed_at, status, additional_instructions, error_message, clipboard, completion_message, created_at
         FROM automation_execution_runs
         WHERE automation_id = ?1
         ORDER BY started_at DESC"
    )?;

    let runs = stmt.query_map(params![automation_id], |row| {
        Ok(AutomationExecutionRun {
            id: row.get(0)?,
            automation_id: row.get(1)?,
            started_at: row.get(2)?,
            completed_at: row.get(3)?,
            status: row.get(4)?,
            additional_instructions: row.get(5)?,
            error_message: row.get(6)?,
            clipboard: row.get(7)?,
            completion_message: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(runs)
}

pub fn get_execution_steps_by_run(
    db: &Connection,
    execution_run_id: i64,
) -> Result<Vec<AutomationExecutionStep>> {
    let mut stmt = db.prepare(
        "SELECT id, execution_run_id, step_number, timestamp, explanation, next_step, status, error_message, created_at, step_type 
         FROM automation_execution_steps 
         WHERE execution_run_id = ?1 
         ORDER BY step_number ASC"
    )?;
    
    let steps = stmt.query_map(params![execution_run_id], |row| {
        Ok(AutomationExecutionStep {
            id: row.get(0)?,
            execution_run_id: row.get(1)?,
            step_number: row.get(2)?,
            timestamp: row.get(3)?,
            explanation: row.get(4)?,
            next_step: row.get(5)?,
            status: row.get(6)?,
            error_message: row.get(7)?,
            created_at: row.get(8)?,
            step_type: row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "action".to_string()),
        })
    })?
    .collect::<Result<Vec<_>>>()?;
    
    Ok(steps)
}

pub fn get_recent_execution_runs_with_steps(
    db: &Connection,
    limit: i32,
) -> Result<Vec<ExecutionRunWithSteps>> {
    // Get recent runs
    let mut stmt = db.prepare(
        "SELECT r.id, r.automation_id, r.started_at, r.completed_at, r.status,
                r.additional_instructions, r.error_message, r.clipboard, r.completion_message, r.created_at,
                a.name as automation_name
         FROM automation_execution_runs r
         JOIN automations a ON r.automation_id = a.id
         ORDER BY r.started_at DESC
         LIMIT ?1"
    )?;

    let runs = stmt.query_map(params![limit], |row| {
        Ok(AutomationExecutionRun {
            id: row.get(0)?,
            automation_id: row.get(1)?,
            started_at: row.get(2)?,
            completed_at: row.get(3)?,
            status: row.get(4)?,
            additional_instructions: row.get(5)?,
            error_message: row.get(6)?,
            clipboard: row.get(7)?,
            completion_message: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    // Get steps for each run
    let mut result = Vec::new();
    for run in runs {
        let steps = get_execution_steps_by_run(db, run.id)?;
        result.push(ExecutionRunWithSteps {
            run: run.clone(),
            steps,
        });
    }

    Ok(result)
}

#[allow(dead_code)]
pub fn get_current_execution_run(
    db: &Connection,
    automation_id: i64,
) -> Result<Option<AutomationExecutionRun>> {
    let mut stmt = db.prepare(
        "SELECT id, automation_id, started_at, completed_at, status, additional_instructions, error_message, clipboard, completion_message, created_at
         FROM automation_execution_runs
         WHERE automation_id = ?1 AND status = 'running'
         ORDER BY started_at DESC
         LIMIT 1"
    )?;

    let mut runs = stmt.query_map(params![automation_id], |row| {
        Ok(AutomationExecutionRun {
            id: row.get(0)?,
            automation_id: row.get(1)?,
            started_at: row.get(2)?,
            completed_at: row.get(3)?,
            status: row.get(4)?,
            additional_instructions: row.get(5)?,
            error_message: row.get(6)?,
            clipboard: row.get(7)?,
            completion_message: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?;

    Ok(runs.next().transpose()?)
}

/// Clean up all stale running executions (e.g., from app crashes)
pub fn cleanup_stale_running_executions(db: &Connection) -> Result<()> {
    let error_message = "Task interrupted (app was restarted)";
    let completed_at = Utc::now().to_rfc3339();
    
    let updated_count = db.execute(
        "UPDATE automation_execution_runs 
         SET status = 'interrupted', 
             completed_at = ?1, 
             error_message = ?2 
         WHERE status = 'running'",
        params![completed_at, error_message],
    )?;
    
    if updated_count > 0 {
        info!("Cleaned up {} stale running executions", updated_count);
    }
    
    Ok(())
}

/// Clean up stale running executions except for a specific run ID (for the currently running automation)
#[allow(dead_code)]
pub fn cleanup_stale_running_executions_except(db: &Connection, except_run_id: Option<i64>) -> Result<()> {
    let error_message = "Task interrupted (app was restarted)";
    let completed_at = Utc::now().to_rfc3339();
    
    let updated_count = if let Some(run_id) = except_run_id {
        db.execute(
            "UPDATE automation_execution_runs 
             SET status = 'interrupted', 
                 completed_at = ?1, 
                 error_message = ?2 
             WHERE status = 'running' AND id != ?3",
            params![completed_at, error_message, run_id],
        )?
    } else {
        db.execute(
            "UPDATE automation_execution_runs 
             SET status = 'interrupted', 
                 completed_at = ?1, 
                 error_message = ?2 
             WHERE status = 'running'",
            params![completed_at, error_message],
        )?
    };
    
    if updated_count > 0 {
        info!("Cleaned up {} stale running executions", updated_count);
    }
    
    Ok(())
}

/// Delete a specific execution run and all its steps
pub fn delete_execution_run(db: &Connection, run_id: i64) -> Result<()> {
    // Steps will be automatically deleted due to CASCADE DELETE
    let deleted_count = db.execute(
        "DELETE FROM automation_execution_runs WHERE id = ?1",
        params![run_id],
    )?;
    
    if deleted_count > 0 {
        info!("Deleted execution run {} and its steps", run_id);
    }
    
    Ok(())
}

/// Reopen an execution run for continuation
/// This sets the status back to 'running' and clears completion info
pub fn reopen_execution_run(db: &Connection, run_id: i64) -> Result<()> {
    db.execute(
        "UPDATE automation_execution_runs 
         SET status = 'running', 
             completed_at = NULL, 
             error_message = NULL,
             completion_message = NULL
         WHERE id = ?1",
        params![run_id],
    )?;
    
    info!("Reopened execution run {} for continuation", run_id);
    Ok(())
}

/// Get the current step count for an execution run
pub fn get_step_count(db: &Connection, run_id: i64) -> Result<i32> {
    let count: i32 = db.query_row(
        "SELECT COUNT(*) FROM automation_execution_steps WHERE execution_run_id = ?1",
        params![run_id],
        |row| row.get(0),
    )?;
    Ok(count)
}