#![allow(dead_code)]

use rusqlite::{params, Connection, Result};
use log::info;
use chrono::Utc;
use serde_json::Value as JsonValue;

use crate::entity::task_extracted_data::{TaskExtractedData, ExtractedDataRecord};

/// Insert multiple extracted data records for a task execution
pub fn insert_extracted_data_batch(
    db: &Connection,
    execution_run_id: i64,
    automation_id: i64,
    records: &[ExtractedDataRecord],
) -> Result<Vec<i64>> {
    let created_at = Utc::now().to_rfc3339();
    let mut inserted_ids = Vec::new();

    for record in records {
        let data_json = serde_json::to_string(&record.data)
            .unwrap_or_else(|_| "{}".to_string());

        db.execute(
            "INSERT INTO task_extracted_data 
             (execution_run_id, automation_id, record_name, record_type, source_context, data, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                execution_run_id,
                automation_id,
                record.record_name,
                record.record_type,
                record.source_context,
                data_json,
                created_at
            ],
        )?;
        inserted_ids.push(db.last_insert_rowid());
    }

    if !inserted_ids.is_empty() {
        info!(
            "Inserted {} extracted data records for execution run {}",
            inserted_ids.len(),
            execution_run_id
        );
    }

    Ok(inserted_ids)
}

/// Get all extracted data for a specific execution run
pub fn get_extracted_data_by_run(
    db: &Connection,
    execution_run_id: i64,
) -> Result<Vec<TaskExtractedData>> {
    let mut stmt = db.prepare(
        "SELECT id, execution_run_id, automation_id, record_name, record_type, 
                source_context, data, created_at
         FROM task_extracted_data
         WHERE execution_run_id = ?1
         ORDER BY id ASC"
    )?;

    let records = stmt.query_map(params![execution_run_id], |row| {
        let data_str: String = row.get(6)?;
        let data: JsonValue = serde_json::from_str(&data_str).unwrap_or(JsonValue::Null);
        
        Ok(TaskExtractedData {
            id: row.get(0)?,
            execution_run_id: row.get(1)?,
            automation_id: row.get(2)?,
            record_name: row.get(3)?,
            record_type: row.get(4)?,
            source_context: row.get(5)?,
            data,
            created_at: row.get(7)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(records)
}

/// Get all extracted data for a specific automation (across all runs)
pub fn get_extracted_data_by_automation(
    db: &Connection,
    automation_id: i64,
) -> Result<Vec<TaskExtractedData>> {
    let mut stmt = db.prepare(
        "SELECT id, execution_run_id, automation_id, record_name, record_type, 
                source_context, data, created_at
         FROM task_extracted_data
         WHERE automation_id = ?1
         ORDER BY created_at DESC"
    )?;

    let records = stmt.query_map(params![automation_id], |row| {
        let data_str: String = row.get(6)?;
        let data: JsonValue = serde_json::from_str(&data_str).unwrap_or(JsonValue::Null);
        
        Ok(TaskExtractedData {
            id: row.get(0)?,
            execution_run_id: row.get(1)?,
            automation_id: row.get(2)?,
            record_name: row.get(3)?,
            record_type: row.get(4)?,
            source_context: row.get(5)?,
            data,
            created_at: row.get(7)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(records)
}

/// Get all extracted data by record type (e.g., all "contact" records)
pub fn get_extracted_data_by_type(
    db: &Connection,
    record_type: &str,
    limit: Option<i32>,
) -> Result<Vec<TaskExtractedData>> {
    let limit_val = limit.unwrap_or(100);
    let mut stmt = db.prepare(
        "SELECT id, execution_run_id, automation_id, record_name, record_type, 
                source_context, data, created_at
         FROM task_extracted_data
         WHERE record_type = ?1
         ORDER BY created_at DESC
         LIMIT ?2"
    )?;

    let records = stmt.query_map(params![record_type, limit_val], |row| {
        let data_str: String = row.get(6)?;
        let data: JsonValue = serde_json::from_str(&data_str).unwrap_or(JsonValue::Null);
        
        Ok(TaskExtractedData {
            id: row.get(0)?,
            execution_run_id: row.get(1)?,
            automation_id: row.get(2)?,
            record_name: row.get(3)?,
            record_type: row.get(4)?,
            source_context: row.get(5)?,
            data,
            created_at: row.get(7)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(records)
}

/// Search extracted data by record name (partial match)
pub fn search_extracted_data(
    db: &Connection,
    search_term: &str,
    limit: Option<i32>,
) -> Result<Vec<TaskExtractedData>> {
    let limit_val = limit.unwrap_or(50);
    let search_pattern = format!("%{}%", search_term);
    
    let mut stmt = db.prepare(
        "SELECT id, execution_run_id, automation_id, record_name, record_type, 
                source_context, data, created_at
         FROM task_extracted_data
         WHERE record_name LIKE ?1 OR source_context LIKE ?1
         ORDER BY created_at DESC
         LIMIT ?2"
    )?;

    let records = stmt.query_map(params![search_pattern, limit_val], |row| {
        let data_str: String = row.get(6)?;
        let data: JsonValue = serde_json::from_str(&data_str).unwrap_or(JsonValue::Null);
        
        Ok(TaskExtractedData {
            id: row.get(0)?,
            execution_run_id: row.get(1)?,
            automation_id: row.get(2)?,
            record_name: row.get(3)?,
            record_type: row.get(4)?,
            source_context: row.get(5)?,
            data,
            created_at: row.get(7)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(records)
}

/// Get recent extracted data across all automations
pub fn get_recent_extracted_data(
    db: &Connection,
    limit: i32,
) -> Result<Vec<TaskExtractedData>> {
    let mut stmt = db.prepare(
        "SELECT id, execution_run_id, automation_id, record_name, record_type, 
                source_context, data, created_at
         FROM task_extracted_data
         ORDER BY created_at DESC
         LIMIT ?1"
    )?;

    let records = stmt.query_map(params![limit], |row| {
        let data_str: String = row.get(6)?;
        let data: JsonValue = serde_json::from_str(&data_str).unwrap_or(JsonValue::Null);
        
        Ok(TaskExtractedData {
            id: row.get(0)?,
            execution_run_id: row.get(1)?,
            automation_id: row.get(2)?,
            record_name: row.get(3)?,
            record_type: row.get(4)?,
            source_context: row.get(5)?,
            data,
            created_at: row.get(7)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(records)
}

/// Delete extracted data by record ID
pub fn delete_extracted_data(db: &Connection, record_id: i64) -> Result<()> {
    db.execute(
        "DELETE FROM task_extracted_data WHERE id = ?1",
        params![record_id],
    )?;
    Ok(())
}

/// Get distinct record types that exist in the database
pub fn get_distinct_record_types(db: &Connection) -> Result<Vec<String>> {
    let mut stmt = db.prepare(
        "SELECT DISTINCT record_type FROM task_extracted_data ORDER BY record_type"
    )?;

    let types = stmt.query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>>>()?;

    Ok(types)
}

/// Summary of a record for the planner (lightweight, no full data)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecordSummary {
    pub id: i64,
    pub record_name: String,
    pub record_type: String,
    pub source_context: Option<String>,
    pub created_at: String,
}

/// Get memory summary for the planner
/// Returns up to 100 record summaries (name, type, id) or just types if more than 100
pub fn get_memory_summary_for_planner(db: &Connection) -> Result<(Vec<RecordSummary>, Vec<String>, i64)> {
    // First, count total records
    let total_count: i64 = db.query_row(
        "SELECT COUNT(*) FROM task_extracted_data",
        [],
        |row| row.get(0)
    )?;

    // Get distinct types
    let types = get_distinct_record_types(db)?;

    // Get up to 100 most recent record summaries
    let mut stmt = db.prepare(
        "SELECT id, record_name, record_type, source_context, created_at
         FROM task_extracted_data
         ORDER BY created_at DESC
         LIMIT 100"
    )?;

    let summaries = stmt.query_map([], |row| {
        Ok(RecordSummary {
            id: row.get(0)?,
            record_name: row.get(1)?,
            record_type: row.get(2)?,
            source_context: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok((summaries, types, total_count))
}

/// Get records by their IDs
pub fn get_records_by_ids(
    db: &Connection,
    ids: &[i64],
) -> Result<Vec<TaskExtractedData>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Build the IN clause
    let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
    let query = format!(
        "SELECT id, execution_run_id, automation_id, record_name, record_type, 
                source_context, data, created_at
         FROM task_extracted_data
         WHERE id IN ({})
         ORDER BY created_at DESC",
        placeholders.join(", ")
    );

    let mut stmt = db.prepare(&query)?;
    
    // Convert ids to rusqlite params
    let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
    
    let records = stmt.query_map(params.as_slice(), |row| {
        let data_str: String = row.get(6)?;
        let data: JsonValue = serde_json::from_str(&data_str).unwrap_or(JsonValue::Null);
        
        Ok(TaskExtractedData {
            id: row.get(0)?,
            execution_run_id: row.get(1)?,
            automation_id: row.get(2)?,
            record_name: row.get(3)?,
            record_type: row.get(4)?,
            source_context: row.get(5)?,
            data,
            created_at: row.get(7)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(records)
}

/// Get records by type with optional limit
pub fn get_records_by_type_for_planner(
    db: &Connection,
    record_type: &str,
    limit: i32,
) -> Result<Vec<TaskExtractedData>> {
    let mut stmt = db.prepare(
        "SELECT id, execution_run_id, automation_id, record_name, record_type, 
                source_context, data, created_at
         FROM task_extracted_data
         WHERE record_type = ?1
         ORDER BY created_at DESC
         LIMIT ?2"
    )?;

    let records = stmt.query_map(params![record_type, limit], |row| {
        let data_str: String = row.get(6)?;
        let data: JsonValue = serde_json::from_str(&data_str).unwrap_or(JsonValue::Null);
        
        Ok(TaskExtractedData {
            id: row.get(0)?,
            execution_run_id: row.get(1)?,
            automation_id: row.get(2)?,
            record_name: row.get(3)?,
            record_type: row.get(4)?,
            source_context: row.get(5)?,
            data,
            created_at: row.get(7)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(records)
}
