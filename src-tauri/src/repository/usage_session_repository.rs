#![allow(dead_code)]

use log::info;
use rusqlite::{params, Connection, Result};
use rusqlite::OptionalExtension;
use chrono::Datelike;
use crate::entity::usage_session::{UsageSession, NewUsageSession, UsageStats};

pub fn create_usage_session(db: &Connection, session: NewUsageSession) -> Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    
    db.execute(
        "INSERT INTO usage_sessions (user_id, automation_id, started_at, status, created_at) 
         VALUES (?1, ?2, ?3, 'active', ?4)",
        params![session.user_id, session.automation_id, now, now],
    )?;
    
    let id = db.last_insert_rowid();
    info!("Created usage session {} for user {} automation {}", id, session.user_id, session.automation_id);
    
    Ok(id)
}

pub fn update_usage_session_progress(db: &Connection, session_id: i64, elapsed_seconds: i32) -> Result<()> {
    db.execute(
        "UPDATE usage_sessions SET total_seconds = ?1 WHERE id = ?2 AND status = 'active'",
        params![elapsed_seconds, session_id],
    )?;
    
    Ok(())
}

pub fn complete_usage_session(db: &Connection, session_id: i64) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    
    // Get total seconds
    let total_seconds: i32 = db.query_row(
        "SELECT total_seconds FROM usage_sessions WHERE id = ?1",
        params![session_id],
        |row| row.get(0)
    )?;
    
    // Calculate billed minutes (round up)
    let billed_minutes = (total_seconds + 59) / 60; // Round up to next minute
    
    db.execute(
        "UPDATE usage_sessions 
         SET ended_at = ?1, status = 'completed', billed_minutes = ?2 
         WHERE id = ?3",
        params![now, billed_minutes, session_id],
    )?;
    
    info!("Completed session {} - {} seconds = {} billed minutes", session_id, total_seconds, billed_minutes);
    
    Ok(())
}

pub fn cancel_usage_session(db: &Connection, session_id: i64) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    
    // Get total seconds
    let total_seconds: i32 = db.query_row(
        "SELECT total_seconds FROM usage_sessions WHERE id = ?1",
        params![session_id],
        |row| row.get(0)
    ).unwrap_or(0);
    
    // Calculate billed minutes (round up) - user still pays for partial usage
    let billed_minutes = (total_seconds + 59) / 60;
    
    db.execute(
        "UPDATE usage_sessions 
         SET ended_at = ?1, status = 'cancelled', billed_minutes = ?2 
         WHERE id = ?3",
        params![now, billed_minutes, session_id],
    )?;
    
    info!("Cancelled session {} - {} seconds = {} billed minutes", session_id, total_seconds, billed_minutes);
    
    Ok(())
}

pub fn get_active_session(db: &Connection, user_id: &str) -> Result<Option<UsageSession>> {
    let mut stmt = db.prepare(
        "SELECT id, user_id, automation_id, started_at, ended_at, total_seconds, billed_minutes, status, created_at
         FROM usage_sessions 
         WHERE user_id = ?1 AND status = 'active' 
         ORDER BY started_at DESC 
         LIMIT 1"
    )?;
    
    let session = stmt.query_row(params![user_id], |row| {
        Ok(UsageSession {
            id: row.get(0)?,
            user_id: row.get(1)?,
            automation_id: row.get(2)?,
            started_at: row.get(3)?,
            ended_at: row.get(4)?,
            total_seconds: row.get(5)?,
            billed_minutes: row.get(6)?,
            status: row.get(7)?,
            created_at: row.get(8)?,
        })
    }).optional()?;
    
    Ok(session)
}

pub fn get_user_usage_stats(db: &Connection, user_id: &str) -> Result<UsageStats> {
    // Total billed minutes
    let total_minutes: i32 = db.query_row(
        "SELECT COALESCE(SUM(billed_minutes), 0) FROM usage_sessions WHERE user_id = ?1",
        params![user_id],
        |row| row.get(0)
    )?;
    
    // Total session count
    let sessions_count: i32 = db.query_row(
        "SELECT COUNT(*) FROM usage_sessions WHERE user_id = ?1",
        params![user_id],
        |row| row.get(0)
    )?;
    
    // Current month minutes
    let current_month_start = chrono::Utc::now()
        .date_naive()
        .with_day(1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .to_rfc3339();
    
    let current_month_minutes: i32 = db.query_row(
        "SELECT COALESCE(SUM(billed_minutes), 0) 
         FROM usage_sessions 
         WHERE user_id = ?1 AND started_at >= ?2",
        params![user_id, current_month_start],
        |row| row.get(0)
    )?;
    
    Ok(UsageStats {
        total_minutes,
        sessions_count,
        current_month_minutes,
    })
}

pub fn get_user_sessions(db: &Connection, user_id: &str, limit: i32) -> Result<Vec<UsageSession>> {
    let mut stmt = db.prepare(
        "SELECT id, user_id, automation_id, started_at, ended_at, total_seconds, billed_minutes, status, created_at
         FROM usage_sessions 
         WHERE user_id = ?1
         ORDER BY started_at DESC 
         LIMIT ?2"
    )?;
    
    let sessions = stmt.query_map(params![user_id, limit], |row| {
        Ok(UsageSession {
            id: row.get(0)?,
            user_id: row.get(1)?,
            automation_id: row.get(2)?,
            started_at: row.get(3)?,
            ended_at: row.get(4)?,
            total_seconds: row.get(5)?,
            billed_minutes: row.get(6)?,
            status: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?.collect::<Result<Vec<_>>>()?;
    
    Ok(sessions)
} 