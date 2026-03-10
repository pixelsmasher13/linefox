use serde::{Deserialize, Serialize};

/// Skill type - determines how the skill is matched
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillType {
    /// Site skill: activates based on URL domain matching
    Site,
    /// App skill: activates based on active application name matching
    App,
    /// Role: always-available persona/role, user selects which one is active
    Role,
}

impl Default for SkillType {
    fn default() -> Self {
        SkillType::Site
    }
}

impl std::fmt::Display for SkillType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillType::Site => write!(f, "site"),
            SkillType::App => write!(f, "app"),
            SkillType::Role => write!(f, "role"),
        }
    }
}

impl From<&str> for SkillType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "app" => SkillType::App,
            "role" => SkillType::Role,
            _ => SkillType::Site,
        }
    }
}

/// A skill contains instructions that are injected into the execution prompt
/// based on domain matching (site skills) or app name matching (app skills)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: i64,
    pub name: String,
    pub skill_type: SkillType,
    /// For site skills: domains to match (e.g., ["linkedin.com", "www.linkedin.com"])
    /// For app skills: app names to match (e.g., ["Visual Studio Code", "Code"])
    pub domains: Vec<String>,
    /// Deprecated - kept for backwards compatibility
    pub triggers: Vec<String>,
    /// Short description shown in the skills list
    pub description: String,
    /// Markdown content with instructions
    pub content: String,
    pub is_active: bool,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating/updating a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInput {
    pub name: String,
    pub skill_type: SkillType,
    pub domains: Vec<String>,
    pub triggers: Vec<String>,
    pub description: String,
    pub content: String,
    pub is_active: bool,
}
