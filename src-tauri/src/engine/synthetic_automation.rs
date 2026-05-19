use log::{info, error};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use crate::repository::automation_repository;
use crate::repository::automation_execution_repository;
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
    pub direct_response: Option<String>,
    pub execution_run_id: Option<i64>,
}

/// Generate a synthetic automation from a natural language prompt
pub async fn generate_synthetic_automation(
    app_handle: &AppHandle,
    request: SyntheticGenerationRequest,
) -> Result<SyntheticGenerationResponse, String> {
    info!("🤖 Starting synthetic automation generation for prompt: {}", request.prompt);

    // Get API choice setting - EXACT same pattern as automation_summary_engine
    let api_choice = request.api_choice.unwrap_or_else(|| {
        app_handle
            .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
            .setting_value
    });
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
                direct_response: None,
                    execution_run_id: None,
                });
            }
            key
        },
        "proxy" => {
            // Get JWT token for proxy authentication
            let user_id = app_handle
                .db(|db| match get_setting(db, "user_id") {
                    Ok(setting) => Some(setting.setting_value),
                    Err(_) => None,
                })
                .unwrap_or_default();

            if user_id.is_empty() {
                return Ok(SyntheticGenerationResponse {
                    success: false,
                    automation_id: None,
                    error: Some("User not authenticated - please login first".to_string()),
                    generated_name: None,
                direct_response: None,
                    execution_run_id: None,
                });
            }

            // Get valid auth token for the user
            match app_handle
                .db(|db| crate::repository::user_auth_repository::get_valid_auth_token(db, &user_id)) {
                Ok(Some(token)) => {
                    info!("🔑 Using JWT token for Heelix proxy (user: {})", user_id);
                    token
                },
                Ok(None) => {
                    return Ok(SyntheticGenerationResponse {
                        success: false,
                        automation_id: None,
                        error: Some("Authentication expired - please login again".to_string()),
                        generated_name: None,
                        direct_response: None,
                        execution_run_id: None,
                    });
                },
                Err(e) => {
                    return Ok(SyntheticGenerationResponse {
                        success: false,
                        automation_id: None,
                        error: Some(format!("Failed to get auth token: {}", e)),
                        generated_name: None,
                        direct_response: None,
                        execution_run_id: None,
                    });
                }
            }
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
                        direct_response: None,
                        execution_run_id: None,
                    });
                }
            }
        },
        "openai-codex" => {
            match crate::auth::openai_codex_oauth::get_active_credentials(app_handle).await {
                Ok((token, _acct)) => {
                    info!("🔑 Using ChatGPT subscription (OAuth)");
                    token
                },
                Err(e) => {
                    return Ok(SyntheticGenerationResponse {
                        success: false,
                        automation_id: None,
                        error: Some(format!("ChatGPT subscription not signed in: {}", e)),
                        generated_name: None,
                        direct_response: None,
                        execution_run_id: None,
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
                        direct_response: None,
                        execution_run_id: None,
                    });
                }
            }
        },
        "claude-subscription" => {
            match app_handle.db(|db| get_setting(db, "api_key_claude_oauth")) {
                Ok(setting) if !setting.setting_value.is_empty() => {
                    info!("🔑 Using Claude subscription OAuth token");
                    setting.setting_value
                },
                _ => {
                    return Ok(SyntheticGenerationResponse {
                        success: false,
                        automation_id: None,
                        error: Some("Claude OAuth token is not configured. Run `claude setup-token` and paste the token in Settings.".to_string()),
                        generated_name: None,
                        direct_response: None,
                        execution_run_id: None,
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
                        direct_response: None,
                        execution_run_id: None,
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

    match generator.generate_script_from_prompt_with_memory(app_handle, &request.prompt, &request.name, &installed_apps).await {
        Ok((generalized_script, nl_description, generated_name)) => {
            // Use the LLM-generated name if available, otherwise fall back to user-provided name
            let final_name = if !generated_name.is_empty() {
                info!("Using LLM-generated name: {}", generated_name);
                generated_name
            } else {
                info!("Using user-provided name: {}", request.name);
                request.name.clone()
            };

            // Check if this is a direct response (no execution needed)
            if nl_description.starts_with("DIRECT_RESPONSE:") {
                let response_body = nl_description.strip_prefix("DIRECT_RESPONSE:").unwrap_or("").to_string();
                info!("📨 Direct response detected, skipping execution. Response length: {}", response_body.len());

                // Create the automation record
                let automation_id = app_handle
                    .db(|db| automation_repository::save_automation(
                        db,
                        &final_name,
                        &request.prompt,
                        "",
                        "",
                    ))
                    .map_err(|e| format!("Failed to create automation: {}", e))?;

                // Create a completed execution run with the response as completion_message
                let run_id = app_handle
                    .db(|db| automation_execution_repository::create_execution_run(db, automation_id, None))
                    .map_err(|e| format!("Failed to create execution run: {}", e))?;

                app_handle
                    .db(|db| automation_execution_repository::update_execution_run_status(db, run_id, "completed", None))
                    .map_err(|e| format!("Failed to update execution run status: {}", e))?;

                app_handle
                    .db(|db| automation_execution_repository::update_execution_run_completion_message(db, run_id, &response_body))
                    .map_err(|e| format!("Failed to save completion message: {}", e))?;

                info!("✅ Created direct-response automation with ID: {}, run_id: {}", automation_id, run_id);

                return Ok(SyntheticGenerationResponse {
                    success: true,
                    automation_id: Some(automation_id),
                    error: None,
                    generated_name: Some(final_name),
                    direct_response: Some(response_body),
                    execution_run_id: Some(run_id),
                });
            }

            // Normal plan flow — create automation and let frontend execute it
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
                direct_response: None,
                execution_run_id: None,
            })
        }
        Err(e) => {
            error!("❌ Failed to generate script: {}", e);

            Ok(SyntheticGenerationResponse {
                success: false,
                automation_id: None,
                error: Some(format!("Failed to generate automation: {}", e)),
                generated_name: None,
                direct_response: None,
                execution_run_id: None,
            })
        }
    }
}

/// Create a minimal automation entry for agent mode execution
/// This skips the expensive script generation since agent mode uses the orchestrator
/// to plan dynamically based on the objective
pub async fn create_agent_mode_automation(
    app_handle: &AppHandle,
    objective: String,
    name: String,
) -> Result<SyntheticGenerationResponse, String> {
    info!("🤖 Creating minimal automation for agent mode: {}", objective);

    // Create a minimal automation record - no script generation needed
    // Agent mode will use the orchestrator to plan dynamically
    let automation_id = app_handle
        .db(|db| automation_repository::save_automation(
            db,
            &name,
            &objective,
            "",  // Empty raw_script
            "",  // Empty generalized_script - agent mode doesn't need it
        ))
        .map_err(|e| format!("Failed to create automation: {}", e))?;

    info!("✅ Created agent mode automation with ID: {}", automation_id);

    Ok(SyntheticGenerationResponse {
        success: true,
        automation_id: Some(automation_id),
        error: None,
        generated_name: Some(name),
        direct_response: None,
        execution_run_id: None,
    })
}
