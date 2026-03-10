use chrono::{Local, NaiveTime, Datelike, Duration, TimeZone, Weekday};
use log::info;
use rusqlite::{params, Connection, Error};

use crate::entity::schedule::{AutomationSchedule, ScheduleWithName};

const TABLE_NAME: &str = "automation_schedules";

/// Get all schedules (active and inactive)
pub fn get_all_schedules(db: &Connection) -> Result<Vec<AutomationSchedule>, Error> {
    let mut stmt = db.prepare(&format!(
        "SELECT * FROM {} ORDER BY created_at DESC", TABLE_NAME
    ))?;
    let mut rows = stmt.query([])?;
    let mut schedules = Vec::new();

    while let Some(row) = rows.next()? {
        schedules.push(AutomationSchedule {
            id: row.get("id")?,
            automation_id: row.get("automation_id")?,
            recurrence_type: row.get("recurrence_type")?,
            recurrence_days: row.get("recurrence_days")?,
            execution_hour: row.get("execution_hour")?,
            execution_minute: row.get("execution_minute")?,
            timezone: row.get("timezone")?,
            is_active: row.get::<_, i32>("is_active")? != 0,
            next_run_at: row.get("next_run_at")?,
            last_run_at: row.get("last_run_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            persistent_run_id: row.get("persistent_run_id").unwrap_or(None),
            continuation_prompt: row.get("continuation_prompt").unwrap_or(None),
        });
    }

    Ok(schedules)
}

/// Get all schedules joined with automation names
pub fn get_all_schedules_with_names(db: &Connection) -> Result<Vec<ScheduleWithName>, Error> {
    let mut stmt = db.prepare(
        "SELECT s.*, a.name as automation_name FROM automation_schedules s \
         LEFT JOIN automations a ON s.automation_id = a.id \
         ORDER BY s.created_at DESC"
    )?;
    let mut rows = stmt.query([])?;
    let mut schedules = Vec::new();

    while let Some(row) = rows.next()? {
        let days_json: Option<String> = row.get("recurrence_days")?;
        let days: Option<Vec<i32>> = days_json.and_then(|j| serde_json::from_str(&j).ok());

        schedules.push(ScheduleWithName {
            id: row.get("id")?,
            automation_id: row.get("automation_id")?,
            automation_name: row.get::<_, Option<String>>("automation_name")?
                .unwrap_or_else(|| format!("Automation #{}", row.get::<_, i64>("automation_id").unwrap_or(0))),
            recurrence_type: row.get("recurrence_type")?,
            recurrence_days: days,
            execution_hour: row.get("execution_hour")?,
            execution_minute: row.get("execution_minute")?,
            timezone: row.get("timezone")?,
            is_active: row.get::<_, i32>("is_active")? != 0,
            next_run_at: row.get("next_run_at")?,
            last_run_at: row.get("last_run_at")?,
        });
    }

    Ok(schedules)
}

/// Get active schedules that are due to run
pub fn get_due_schedules(db: &Connection, now_iso: &str) -> Result<Vec<AutomationSchedule>, Error> {
    let mut stmt = db.prepare(&format!(
        "SELECT * FROM {} WHERE is_active = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?",
        TABLE_NAME
    ))?;
    let mut rows = stmt.query(params![now_iso])?;
    let mut schedules = Vec::new();

    while let Some(row) = rows.next()? {
        schedules.push(AutomationSchedule {
            id: row.get("id")?,
            automation_id: row.get("automation_id")?,
            recurrence_type: row.get("recurrence_type")?,
            recurrence_days: row.get("recurrence_days")?,
            execution_hour: row.get("execution_hour")?,
            execution_minute: row.get("execution_minute")?,
            timezone: row.get("timezone")?,
            is_active: row.get::<_, i32>("is_active")? != 0,
            next_run_at: row.get("next_run_at")?,
            last_run_at: row.get("last_run_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            persistent_run_id: row.get("persistent_run_id").unwrap_or(None),
            continuation_prompt: row.get("continuation_prompt").unwrap_or(None),
        });
    }

    Ok(schedules)
}

/// Create a new schedule and compute next_run_at
pub fn create_schedule(
    db: &Connection,
    automation_id: i64,
    recurrence_type: &str,
    recurrence_days: Option<Vec<i32>>,
    execution_hour: i32,
    execution_minute: i32,
    timezone: &str,
    persistent_run_id: Option<i64>,
    continuation_prompt: Option<&str>,
) -> Result<i64, Error> {
    let now = Local::now().to_rfc3339();
    let days_json = recurrence_days.as_ref().map(|d| serde_json::to_string(d).unwrap_or_default());
    let next_run = compute_next_run(recurrence_type, recurrence_days.as_deref(), execution_hour, execution_minute, timezone);

    info!("[Scheduler] Creating schedule for automation {}: {} at {:02}:{:02} {}, next_run={}, continuation={}",
          automation_id, recurrence_type, execution_hour, execution_minute, timezone,
          next_run.as_deref().unwrap_or("none"),
          persistent_run_id.map(|id| id.to_string()).as_deref().unwrap_or("none"));

    db.execute(
        &format!(
            "INSERT INTO {} (automation_id, recurrence_type, recurrence_days, execution_hour, \
             execution_minute, timezone, is_active, next_run_at, created_at, updated_at, \
             persistent_run_id, continuation_prompt) \
             VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?)",
            TABLE_NAME
        ),
        params![automation_id, recurrence_type, days_json, execution_hour, execution_minute,
                timezone, next_run, now, now, persistent_run_id, continuation_prompt],
    )?;

    Ok(db.last_insert_rowid())
}

/// Update a schedule
pub fn update_schedule(
    db: &Connection,
    schedule_id: i64,
    is_active: Option<bool>,
    recurrence_type: Option<&str>,
    recurrence_days: Option<Vec<i32>>,
    execution_hour: Option<i32>,
    execution_minute: Option<i32>,
) -> Result<(), Error> {
    // Fetch current schedule first
    let current = get_schedule_by_id(db, schedule_id)?;
    let current = match current {
        Some(s) => s,
        None => return Err(Error::QueryReturnedNoRows),
    };

    let new_active = is_active.unwrap_or(current.is_active);
    let new_rec_type = recurrence_type.unwrap_or(&current.recurrence_type).to_string();
    let new_hour = execution_hour.unwrap_or(current.execution_hour);
    let new_minute = execution_minute.unwrap_or(current.execution_minute);
    let new_days = recurrence_days.or_else(|| {
        current.recurrence_days.as_ref().and_then(|j| serde_json::from_str(j).ok())
    });
    let days_json = new_days.as_ref().map(|d| serde_json::to_string(d).unwrap_or_default());

    let next_run = if new_active {
        compute_next_run(&new_rec_type, new_days.as_deref(), new_hour, new_minute, &current.timezone)
    } else {
        None
    };

    let now = Local::now().to_rfc3339();
    db.execute(
        &format!(
            "UPDATE {} SET is_active = ?, recurrence_type = ?, recurrence_days = ?, \
             execution_hour = ?, execution_minute = ?, next_run_at = ?, updated_at = ? WHERE id = ?",
            TABLE_NAME
        ),
        params![
            new_active as i32,
            new_rec_type,
            days_json,
            new_hour,
            new_minute,
            next_run,
            now,
            schedule_id
        ],
    )?;

    Ok(())
}

/// Delete a schedule
pub fn delete_schedule(db: &Connection, schedule_id: i64) -> Result<(), Error> {
    db.execute(
        &format!("DELETE FROM {} WHERE id = ?", TABLE_NAME),
        params![schedule_id],
    )?;
    Ok(())
}

/// Get a schedule by ID
pub fn get_schedule_by_id(db: &Connection, schedule_id: i64) -> Result<Option<AutomationSchedule>, Error> {
    let result = db.query_row(
        &format!("SELECT * FROM {} WHERE id = ?", TABLE_NAME),
        params![schedule_id],
        |row| {
            Ok(AutomationSchedule {
                id: row.get("id")?,
                automation_id: row.get("automation_id")?,
                recurrence_type: row.get("recurrence_type")?,
                recurrence_days: row.get("recurrence_days")?,
                execution_hour: row.get("execution_hour")?,
                execution_minute: row.get("execution_minute")?,
                timezone: row.get("timezone")?,
                is_active: row.get::<_, i32>("is_active")? != 0,
                next_run_at: row.get("next_run_at")?,
                last_run_at: row.get("last_run_at")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
                persistent_run_id: row.get("persistent_run_id").unwrap_or(None),
                continuation_prompt: row.get("continuation_prompt").unwrap_or(None),
            })
        },
    );

    match result {
        Ok(s) => Ok(Some(s)),
        Err(Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Get schedule(s) for a given automation
pub fn get_schedules_for_automation(db: &Connection, automation_id: i64) -> Result<Vec<AutomationSchedule>, Error> {
    let mut stmt = db.prepare(&format!(
        "SELECT * FROM {} WHERE automation_id = ? ORDER BY created_at DESC", TABLE_NAME
    ))?;
    let mut rows = stmt.query(params![automation_id])?;
    let mut schedules = Vec::new();

    while let Some(row) = rows.next()? {
        schedules.push(AutomationSchedule {
            id: row.get("id")?,
            automation_id: row.get("automation_id")?,
            recurrence_type: row.get("recurrence_type")?,
            recurrence_days: row.get("recurrence_days")?,
            execution_hour: row.get("execution_hour")?,
            execution_minute: row.get("execution_minute")?,
            timezone: row.get("timezone")?,
            is_active: row.get::<_, i32>("is_active")? != 0,
            next_run_at: row.get("next_run_at")?,
            last_run_at: row.get("last_run_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            persistent_run_id: row.get("persistent_run_id").unwrap_or(None),
            continuation_prompt: row.get("continuation_prompt").unwrap_or(None),
        });
    }

    Ok(schedules)
}

/// Mark a schedule as just-ran and compute the next run time
pub fn mark_schedule_ran(db: &Connection, schedule_id: i64) -> Result<(), Error> {
    let schedule = get_schedule_by_id(db, schedule_id)?;
    let schedule = match schedule {
        Some(s) => s,
        None => return Err(Error::QueryReturnedNoRows),
    };

    let now = Local::now().to_rfc3339();
    let days: Option<Vec<i32>> = schedule.recurrence_days.as_ref().and_then(|j| serde_json::from_str(j).ok());
    let next_run = compute_next_run(
        &schedule.recurrence_type,
        days.as_deref(),
        schedule.execution_hour,
        schedule.execution_minute,
        &schedule.timezone,
    );

    db.execute(
        &format!(
            "UPDATE {} SET last_run_at = ?, next_run_at = ?, updated_at = ? WHERE id = ?",
            TABLE_NAME
        ),
        params![now, next_run, now, schedule_id],
    )?;

    Ok(())
}

// ── Helpers ──

/// Compute the next run datetime (ISO 8601) based on recurrence settings.
/// Returns None if the schedule can't be computed.
fn compute_next_run(
    recurrence_type: &str,
    recurrence_days: Option<&[i32]>,
    hour: i32,
    minute: i32,
    _timezone: &str,
) -> Option<String> {
    // Use local time for simplicity (the user's machine timezone)
    let now = Local::now();
    let target_time = NaiveTime::from_hms_opt(hour as u32, minute as u32, 0)?;

    // Start from today
    let today_target = now.date_naive().and_time(target_time);
    let today_target_local = Local.from_local_datetime(&today_target).single()?;

    match recurrence_type {
        "daily" => {
            if today_target_local > now {
                Some(today_target_local.to_rfc3339())
            } else {
                let tomorrow = today_target_local + Duration::days(1);
                Some(tomorrow.to_rfc3339())
            }
        }
        "weekdays" => {
            // Mon=1..Fri=5 (chrono Weekday::num_days_from_monday() + 1)
            let mut candidate = if today_target_local > now {
                today_target_local
            } else {
                today_target_local + Duration::days(1)
            };
            // Advance until we hit a weekday
            for _ in 0..7 {
                let wd = candidate.weekday();
                if wd != Weekday::Sat && wd != Weekday::Sun {
                    return Some(candidate.to_rfc3339());
                }
                candidate = candidate + Duration::days(1);
            }
            None
        }
        "weekly" => {
            let days = recurrence_days?;
            if days.is_empty() {
                return None;
            }
            // days are 0=Sun,1=Mon,...6=Sat
            let mut candidate = if today_target_local > now {
                today_target_local
            } else {
                today_target_local + Duration::days(1)
            };
            for _ in 0..8 {
                let day_num = candidate.weekday().num_days_from_sunday() as i32;
                if days.contains(&day_num) {
                    return Some(candidate.to_rfc3339());
                }
                candidate = candidate + Duration::days(1);
            }
            None
        }
        _ => None,
    }
}
