#![allow(dead_code)]

use chrono::Local;
use rusqlite::{params, Connection, Error};

use crate::entity::automation::{Automation, AutomationScript, AutomationStep};

const TABLE_NAME: &str = "automations";

pub fn get_all_automations(db: &Connection) -> Result<Vec<Automation>, Error> {
    let mut statement = db.prepare(&format!("SELECT * FROM {} ORDER BY updated_at DESC", TABLE_NAME))?;
    let mut rows = statement.query([])?;
    
    let mut automations = Vec::new();
    while let Some(row) = rows.next()? {
        let nl_description = if check_column_exists(db, TABLE_NAME, "nl_description")? {
            row.get("nl_description").ok()
        } else {
            None
        };
        
        automations.push(Automation {
            id: row.get("id")?,
            name: row.get("name")?,
            objective: row.get("objective")?,
            raw_script: row.get("raw_script")?,
            generalized_script: row.get("generalized_script")?,
            nl_description,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        });
    }
    
    Ok(automations)
}

pub fn get_automation_by_id(db: &Connection, id: i64) -> Result<Option<Automation>, Error> {
    let query = format!("SELECT * FROM {} WHERE id = ?", TABLE_NAME);
    
    // First check if nl_description column exists
    let has_nl_description = check_column_exists(db, TABLE_NAME, "nl_description")?;
    
    let automation = db.query_row(
        &query,
        [id],
        |row| {
            let nl_description = if has_nl_description {
                row.get("nl_description").ok()
            } else {
                None
            };
            
            Ok(Automation {
                id: row.get("id")?,
                name: row.get("name")?,
                objective: row.get("objective")?,
                raw_script: row.get("raw_script")?,
                generalized_script: row.get("generalized_script")?,
                nl_description,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        },
    );
    
    match automation {
        Ok(automation) => Ok(Some(automation)),
        Err(Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn save_automation(
    db: &Connection,
    name: &str,
    objective: &str,
    raw_script: &str,
    generalized_script: &str,
) -> Result<i64, Error> {
    let now = Local::now().to_rfc3339();
    
    db.execute(
        &format!(
            "INSERT INTO {} (name, objective, raw_script, generalized_script, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            TABLE_NAME
        ),
        params![name, objective, raw_script, generalized_script, now, now],
    )?;
    
    Ok(db.last_insert_rowid())
}

pub fn update_automation(
    db: &Connection,
    id: i64,
    name: &str,
    objective: &str,
    raw_script: &str,
    generalized_script: &str,
) -> Result<(), Error> {
    let now = Local::now().to_rfc3339();
    
    db.execute(
        &format!(
            "UPDATE {} SET name = ?, objective = ?, raw_script = ?, generalized_script = ?, updated_at = ?
             WHERE id = ?",
            TABLE_NAME
        ),
        params![name, objective, raw_script, generalized_script, now, id],
    )?;
    
    Ok(())
}

pub fn delete_automation(db: &Connection, id: i64) -> Result<(), Error> {
    db.execute(
        &format!("DELETE FROM {} WHERE id = ?", TABLE_NAME),
        params![id],
    )?;
    
    Ok(())
}

pub fn get_automation_history(db: &Connection) -> Result<Vec<Automation>, Error> {
    // Get the most recently updated automations
    let query = format!(
        "SELECT * FROM {} ORDER BY updated_at DESC LIMIT 20",
        TABLE_NAME
    );
    
    let mut statement = db.prepare(&query)?;
    let mut rows = statement.query([])?;
    
    // Check if nl_description column exists
    let has_nl_description = check_column_exists(db, TABLE_NAME, "nl_description")?;
    
    let mut history = Vec::new();
    while let Some(row) = rows.next()? {
        let nl_description = if has_nl_description {
            row.get("nl_description").ok()
        } else {
            None
        };
        
        history.push(Automation {
            id: row.get("id")?,
            name: row.get("name")?,
            objective: row.get("objective")?,
            raw_script: row.get("raw_script")?,
            generalized_script: row.get("generalized_script")?,
            nl_description,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        });
    }
    
    Ok(history)
}

pub fn get_automation_script(db: &Connection, id: i64) -> Result<Option<AutomationScript>, Error> {
    // First, get the automation
    let automation = match get_automation_by_id(db, id)? {
        Some(automation) => automation,
        None => return Ok(None),
    };
    
    // Ensure nl_description column exists
    if !check_column_exists(db, TABLE_NAME, "nl_description")? {
        // Add nl_description column if missing
        let _ = db.execute(
            &format!("ALTER TABLE {} ADD COLUMN nl_description TEXT", TABLE_NAME),
            [],
        );
    }
    
    // Parse the generalized_script to get steps
    // Here we're assuming the generalized_script is a JSON array of steps
    let steps: Result<Vec<AutomationStep>, _> = serde_json::from_str(&automation.generalized_script);
    let steps = match steps {
        Ok(steps) => steps,
        Err(_) => {
            // If not valid JSON, create a single step with the raw content
            vec![AutomationStep {
                id: 1,
                action: "SCRIPT".to_string(),
                target: "".to_string(),
                description: "Execute raw script".to_string(),
                params: Some(serde_json::json!({ "script": automation.generalized_script })),
            }]
        }
    };
    
    // Check if additional_instructions exists
    let has_additional_instructions = check_column_exists(db, TABLE_NAME, "additional_instructions")?;
    
    if !has_additional_instructions {
        // Add additional_instructions column if missing
        let _ = db.execute(
            &format!("ALTER TABLE {} ADD COLUMN additional_instructions TEXT", TABLE_NAME),
            [],
        );
    }
    
    // Fix double-wrapped Option issue
    let additional_instructions: Option<String> = if has_additional_instructions {
        match db.query_row(
            &format!("SELECT additional_instructions FROM {} WHERE id = ?", TABLE_NAME),
            [id],
            |row| row.get::<_, Option<String>>("additional_instructions")
        ) {
            Ok(Some(value)) => Some(value),
            _ => None
        }
    } else {
        None
    };
    
    Ok(Some(AutomationScript {
        id: automation.id,
        name: automation.name,
        objective: automation.objective,
        nl_description: automation.nl_description,
        steps,
        additional_instructions,
        created_at: automation.created_at,
        updated_at: automation.updated_at,
    }))
}

pub fn update_automation_script(
    db: &Connection,
    id: i64,
    script: &AutomationScript,
) -> Result<(), Error> {
    // Convert the steps to a JSON string
    let generalized_script = serde_json::to_string(&script.steps).unwrap_or_default();
    
    // Ensure columns exist
    let has_nl_description = check_column_exists(db, TABLE_NAME, "nl_description")?;
    let has_additional_instructions = check_column_exists(db, TABLE_NAME, "additional_instructions")?;
    
    if !has_nl_description {
        // Add nl_description column if missing
        let _ = db.execute(
            &format!("ALTER TABLE {} ADD COLUMN nl_description TEXT", TABLE_NAME),
            [],
        );
    }
    
    if !has_additional_instructions {
        // Add additional_instructions column if missing
        let _ = db.execute(
            &format!("ALTER TABLE {} ADD COLUMN additional_instructions TEXT", TABLE_NAME),
            [],
        );
    }
    
    // Update with all fields
    db.execute(
        &format!(
            "UPDATE {} SET name = ?, objective = ?, generalized_script = ?, nl_description = ?, additional_instructions = ?, updated_at = ?
             WHERE id = ?",
            TABLE_NAME
        ),
        params![
            script.name,
            script.objective,
            generalized_script,
            script.nl_description,
            script.additional_instructions,
            Local::now().to_rfc3339(),
            id
        ],
    )?;
    
    Ok(())
}

/// Check if a column exists in a table
fn check_column_exists(db: &Connection, table: &str, column: &str) -> Result<bool, Error> {
    let result = db.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name='{}'", table, column),
        [],
        |row| row.get::<_, i64>(0)
    )?;
    
    Ok(result > 0)
} 