use log::{info, error};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use crate::repository::automation_repository;
use crate::repository::permissions_repository;
use crate::engine::script_generator::ScriptGenerator;
use crate::configuration::state::ServiceAccess;
use crate::repository::settings_repository::get_setting;

#[derive(Debug, Serialize, Deserialize)]
pub struct SyntheticGenerationRequest {
    pub prompt: String,
    pub name: String,
    pub api_key: Option<String>,
    pub api_choice: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyntheticGenerationResponse {
    pub success: bool,
    pub automation_id: Option<i64>,
    pub error: Option<String>,
    pub generated_name: Option<String>,
}

/// Generate a synthetic automation from a natural language prompt
pub async fn generate_synthetic_automation(
    app_handle: &AppHandle,
    request: SyntheticGenerationRequest,
) -> Result<SyntheticGenerationResponse, String> {
    info!("🤖 Starting synthetic automation generation for prompt: {}", request.prompt);

    let api_choice_raw = request.api_choice.unwrap_or_else(|| {
        app_handle
            .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
            .setting_value
    });
    // "proxy" is a cloud-only provider not available in the open source build.
    // If a stale DB value slips through, fall back to Claude.
    let api_choice = match api_choice_raw.as_str() {
        "proxy" => {
            info!("⚠️ api_choice 'proxy' is not supported — falling back to 'claude'. Update your API Choice in Settings.");
            "claude".to_string()
        }
        other => other.to_string(),
    };
    info!("🎯 Using api_choice: {}", api_choice);

    // Get the appropriate API key based on the choice - EXACT same pattern
    let api_key = match api_choice.as_str() {
        "gemini" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_gemini").expect("Failed to get Gemini API key"))
                .setting_value;
            if key.is_empty() {
                return Ok(SyntheticGenerationResponse {
                    success: false,
                    automation_id: None,
                    error: Some("Gemini API key is not configured".to_string()),
                    generated_name: None,
                });
            }
            key
        },
        "openai" => {
            match app_handle.db(|db| get_setting(db, "api_key_open_ai")) {
                Ok(setting) if !setting.setting_value.is_empty() => {
                    info!("🔑 Using OpenAI API key");
                    setting.setting_value
                },
                _ => {
                    return Ok(SyntheticGenerationResponse {
                        success: false,
                        automation_id: None,
                        error: Some("OpenAI API key is not configured".to_string()),
                        generated_name: None,
                    });
                }
            }
        },
        "grok" => {
            match app_handle.db(|db| get_setting(db, "api_key_grok")) {
                Ok(setting) if !setting.setting_value.is_empty() => {
                    info!("🔑 Using Grok API key");
                    setting.setting_value
                },
                _ => {
                    return Ok(SyntheticGenerationResponse {
                        success: false,
                        automation_id: None,
                        error: Some("Grok API key is not configured".to_string()),
                        generated_name: None,
                    });
                }
            }
        },
        "deepseek" => {
            match app_handle.db(|db| get_setting(db, "api_key_deepseek")) {
                Ok(setting) if !setting.setting_value.is_empty() => {
                    info!("🔑 Using DeepSeek API key");
                    setting.setting_value
                },
                _ => {
                    return Ok(SyntheticGenerationResponse {
                        success: false,
                        automation_id: None,
                        error: Some("DeepSeek API key is not configured".to_string()),
                        generated_name: None,
                    });
                }
            }
        },
        "claude" | _ => {
            match app_handle.db(|db| get_setting(db, "api_key_claude")) {
                Ok(setting) if !setting.setting_value.is_empty() => {
                    info!("🔑 Using Claude API key");
                    setting.setting_value
                },
                _ => {
                    return Ok(SyntheticGenerationResponse {
                        success: false,
                        automation_id: None,
                        error: Some("Claude API key is not configured".to_string()),
                        generated_name: None,
                    });
                }
            }
        }
    };

    // Get installed apps from permissions table
    let installed_apps = app_handle
        .db(|db| {
            permissions_repository::get_permissions(db)
                .map(|perms| {
                    perms.into_iter()
                        .map(|p| p.app_name)
                        .filter(|name| !name.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|e| {
                    error!("Failed to get installed apps: {}", e);
                    vec![]
                })
        });

    info!("📱 Found {} installed apps for prompt generation", installed_apps.len());

    // Generate the generalized script and NL description FIRST before saving
    // Use memory-enabled version to allow planner to access stored data from past tasks
    info!("📝 Creating ScriptGenerator with api_choice: {} and key length: {}", api_choice, api_key.len());
    let generator = ScriptGenerator::new(api_key, api_choice);

    match generator.generate_script_from_prompt(&request.prompt, &request.name, &installed_apps).await {
        Ok((generalized_script, nl_description, generated_name)) => {
            // Use the LLM-generated name if available, otherwise fall back to user-provided name
            let final_name = if !generated_name.is_empty() {
                info!("Using LLM-generated name: {}", generated_name);
                generated_name
            } else {
                info!("Using user-provided name: {}", request.name);
                request.name.clone()
            };

            // Now create the automation record with the generated script
            // For synthetic automations, raw_script is empty since there are no recorded actions
            let automation_id = app_handle
                .db(|db| automation_repository::save_automation(
                    db,
                    &final_name,
                    &request.prompt,
                    "",  // Empty raw_script for synthetic automations
                    &generalized_script,
                ))
                .map_err(|e| format!("Failed to create automation: {}", e))?;

            info!("✅ Created automation record with ID: {}", automation_id);

            // Update with NL description
            app_handle
                .db_mut(|db| {
                    db.execute(
                        "UPDATE automations SET nl_description = ?, updated_at = datetime('now') WHERE id = ?",
                        [&nl_description, &automation_id.to_string()]
                    )
                    .map(|_| ())
                    .map_err(|e| format!("Failed to update automation: {}", e))
                })
                .map_err(|e| format!("Failed to update automation description: {}", e))?;

            info!("✅ Successfully generated synthetic automation");

            Ok(SyntheticGenerationResponse {
                success: true,
                automation_id: Some(automation_id),
                error: None,
                generated_name: Some(final_name),
            })
        }
        Err(e) => {
            error!("❌ Failed to generate script: {}", e);

            Ok(SyntheticGenerationResponse {
                success: false,
                automation_id: None,
                error: Some(format!("Failed to generate automation: {}", e)),
                generated_name: None,
            })
        }
    }
}

/// Create a minimal automation for agent mode (no script generation).
/// Agent mode uses the orchestrator to plan dynamically, so we just need a DB record.
pub async fn create_agent_mode_automation(
    app_handle: &AppHandle,
    objective: String,
    name: String,
) -> Result<SyntheticGenerationResponse, String> {
    info!("Creating minimal automation for agent mode: {}", objective);

    let automation_id = app_handle
        .db(|db| automation_repository::save_automation(
            db,
            &name,
            &objective,
            "",  // Empty raw_script
            "",  // Empty generalized_script - agent mode doesn't need it
        ))
        .map_err(|e| format!("Failed to create automation: {}", e))?;

    info!("Created agent mode automation with ID: {}", automation_id);

    Ok(SyntheticGenerationResponse {
        success: true,
        automation_id: Some(automation_id),
        error: None,
        generated_name: Some(name),
    })
}
