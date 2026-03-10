#![allow(dead_code)]

use rusqlite::{params, Connection, Result};
use log::info;
use chrono::Utc;

use crate::entity::skill::{Skill, SkillType, SkillInput};

/// Get all active skills
pub fn get_active_skills(db: &Connection) -> Result<Vec<Skill>> {
    let mut stmt = db.prepare(
        "SELECT id, name, skill_type, domains, triggers, description, content, is_active, is_default, created_at, updated_at
         FROM user_skills
         WHERE is_active = 1
         ORDER BY is_default DESC, name ASC"
    )?;

    let skills = stmt.query_map([], |row| {
        let domains_json: String = row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "[]".to_string());
        let triggers_json: String = row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "[]".to_string());
        let skill_type_str: String = row.get(2)?;
        
        Ok(Skill {
            id: row.get(0)?,
            name: row.get(1)?,
            skill_type: SkillType::from(skill_type_str.as_str()),
            domains: serde_json::from_str(&domains_json).unwrap_or_default(),
            triggers: serde_json::from_str(&triggers_json).unwrap_or_default(),
            description: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            content: row.get(6)?,
            is_active: row.get::<_, i32>(7)? == 1,
            is_default: row.get::<_, i32>(8)? == 1,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(skills)
}

/// Get all skills (including inactive)
pub fn get_all_skills(db: &Connection) -> Result<Vec<Skill>> {
    let mut stmt = db.prepare(
        "SELECT id, name, skill_type, domains, triggers, description, content, is_active, is_default, created_at, updated_at
         FROM user_skills
         ORDER BY is_default DESC, name ASC"
    )?;

    let skills = stmt.query_map([], |row| {
        let domains_json: String = row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "[]".to_string());
        let triggers_json: String = row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "[]".to_string());
        let skill_type_str: String = row.get(2)?;
        
        Ok(Skill {
            id: row.get(0)?,
            name: row.get(1)?,
            skill_type: SkillType::from(skill_type_str.as_str()),
            domains: serde_json::from_str(&domains_json).unwrap_or_default(),
            triggers: serde_json::from_str(&triggers_json).unwrap_or_default(),
            description: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            content: row.get(6)?,
            is_active: row.get::<_, i32>(7)? == 1,
            is_default: row.get::<_, i32>(8)? == 1,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(skills)
}

/// Get skills by type
pub fn get_skills_by_type(db: &Connection, skill_type: &SkillType) -> Result<Vec<Skill>> {
    let type_str = skill_type.to_string();
    let mut stmt = db.prepare(
        "SELECT id, name, skill_type, domains, triggers, description, content, is_active, is_default, created_at, updated_at
         FROM user_skills
         WHERE skill_type = ?1 AND is_active = 1
         ORDER BY is_default DESC, name ASC"
    )?;

    let skills = stmt.query_map(params![type_str], |row| {
        let domains_json: String = row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "[]".to_string());
        let triggers_json: String = row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "[]".to_string());
        let skill_type_str: String = row.get(2)?;
        
        Ok(Skill {
            id: row.get(0)?,
            name: row.get(1)?,
            skill_type: SkillType::from(skill_type_str.as_str()),
            domains: serde_json::from_str(&domains_json).unwrap_or_default(),
            triggers: serde_json::from_str(&triggers_json).unwrap_or_default(),
            description: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            content: row.get(6)?,
            is_active: row.get::<_, i32>(7)? == 1,
            is_default: row.get::<_, i32>(8)? == 1,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })?
    .collect::<Result<Vec<_>>>()?;

    Ok(skills)
}

/// Get a skill by ID
pub fn get_skill_by_id(db: &Connection, id: i64) -> Result<Option<Skill>> {
    let mut stmt = db.prepare(
        "SELECT id, name, skill_type, domains, triggers, description, content, is_active, is_default, created_at, updated_at
         FROM user_skills
         WHERE id = ?1"
    )?;

    let mut rows = stmt.query(params![id])?;
    
    if let Some(row) = rows.next()? {
        let domains_json: String = row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "[]".to_string());
        let triggers_json: String = row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "[]".to_string());
        let skill_type_str: String = row.get(2)?;
        
        Ok(Some(Skill {
            id: row.get(0)?,
            name: row.get(1)?,
            skill_type: SkillType::from(skill_type_str.as_str()),
            domains: serde_json::from_str(&domains_json).unwrap_or_default(),
            triggers: serde_json::from_str(&triggers_json).unwrap_or_default(),
            description: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            content: row.get(6)?,
            is_active: row.get::<_, i32>(7)? == 1,
            is_default: row.get::<_, i32>(8)? == 1,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        }))
    } else {
        Ok(None)
    }
}

/// Create a new skill
pub fn create_skill(db: &Connection, input: &SkillInput) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    let domains_json = serde_json::to_string(&input.domains).unwrap_or_else(|_| "[]".to_string());
    let triggers_json = serde_json::to_string(&input.triggers).unwrap_or_else(|_| "[]".to_string());
    let type_str = input.skill_type.to_string();

    db.execute(
        "INSERT INTO user_skills (name, skill_type, domains, triggers, description, content, is_active, is_default, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)",
        params![
            input.name,
            type_str,
            domains_json,
            triggers_json,
            input.description,
            input.content,
            if input.is_active { 1 } else { 0 },
            now
        ],
    )?;

    let id = db.last_insert_rowid();
    info!("Created skill '{}' with ID {}", input.name, id);
    Ok(id)
}

/// Update an existing skill
pub fn update_skill(db: &Connection, id: i64, input: &SkillInput) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let domains_json = serde_json::to_string(&input.domains).unwrap_or_else(|_| "[]".to_string());
    let triggers_json = serde_json::to_string(&input.triggers).unwrap_or_else(|_| "[]".to_string());
    let type_str = input.skill_type.to_string();

    db.execute(
        "UPDATE user_skills
         SET name = ?1, skill_type = ?2, domains = ?3, triggers = ?4, description = ?5, content = ?6, is_active = ?7, updated_at = ?8
         WHERE id = ?9",
        params![
            input.name,
            type_str,
            domains_json,
            triggers_json,
            input.description,
            input.content,
            if input.is_active { 1 } else { 0 },
            now,
            id
        ],
    )?;

    info!("Updated skill ID {}", id);
    Ok(())
}

/// Delete a skill (only non-default skills can be deleted)
pub fn delete_skill(db: &Connection, id: i64) -> Result<bool> {
    let deleted = db.execute(
        "DELETE FROM user_skills WHERE id = ?1 AND is_default = 0",
        params![id],
    )?;

    if deleted > 0 {
        info!("Deleted skill ID {}", id);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Toggle skill active status
pub fn toggle_skill_active(db: &Connection, id: i64, is_active: bool) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    
    db.execute(
        "UPDATE user_skills SET is_active = ?1, updated_at = ?2 WHERE id = ?3",
        params![if is_active { 1 } else { 0 }, now, id],
    )?;

    Ok(())
}

/// Insert a default skill (used for seeding built-in skills)
pub fn insert_default_skill(
    db: &Connection,
    name: &str,
    skill_type: &SkillType,
    domains: &[String],
    triggers: &[String],
    description: &str,
    content: &str,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    let domains_json = serde_json::to_string(domains).unwrap_or_else(|_| "[]".to_string());
    let triggers_json = serde_json::to_string(triggers).unwrap_or_else(|_| "[]".to_string());
    let type_str = skill_type.to_string();

    db.execute(
        "INSERT OR REPLACE INTO user_skills (name, skill_type, domains, triggers, description, content, is_active, is_default, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 1, ?7, ?7)",
        params![
            name,
            type_str,
            domains_json,
            triggers_json,
            description,
            content,
            now
        ],
    )?;

    Ok(db.last_insert_rowid())
}

/// Check if default skills have been seeded
pub fn has_default_skills(db: &Connection) -> Result<bool> {
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM user_skills WHERE is_default = 1",
        [],
        |row| row.get(0)
    )?;
    Ok(count > 0)
}
