use std::env;
use std::fs::remove_file;
use std::path::PathBuf;

use lazy_static::lazy_static;
use log::{info, error, warn};
use rusqlite::Connection;
use serde_derive::Serialize;
use tauri::{AppHandle, Manager, State, Emitter, Listener, Wry};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::menu::{Menu, MenuItem};

use configuration::settings::Settings;

use crate::bootstrap::{fix_path_env, prerequisites};
use crate::configuration::database;
use crate::configuration::state::{AppState, ServiceAccess};
use crate::entity::activity_item::ActivityItem;
use crate::entity::permission::Permission;
use crate::entity::setting::Setting;
use crate::entity::automation::{Automation, AutomationScript};
use crate::entity::automation_execution::ExecutionRunWithSteps;
use crate::permissions::permission_engine::init_permissions;
use crate::repository::permissions_repository::{get_permissions, update_permission};
use crate::repository::settings_repository::{get_setting, get_settings, insert_or_update_setting};
use crate::repository::automation_repository;
use crate::repository::automation_execution_repository;
use crate::repository::activity_log_repository;
use crate::repository::skill_repository;
use crate::entity::skill::{Skill, SkillInput, SkillType};
use crate::entity::schedule::ScheduleWithName;
use crate::repository::schedule_repository;
use tauri_plugin_autostart::MacosLauncher;
// Import Chrome automation functions
use crate::chrome_automation::{
    // Add new functions for app automation
    launch_app,
    get_app_actionable_elements,
    perform_random_app_action
};

// Add new imports for automation logging and summarization
use crate::entity::automation_log::{AutomationLog, AutomationSummary};
use crate::repository::automation_log_repository;
use crate::repository::user_auth_repository;
use crate::repository::usage_session_repository;

mod bootstrap;
mod configuration;
mod engine;
mod entity;
mod monitoring;
pub mod permissions;
mod repository;
pub mod window_details_collector;
mod chrome_automation;
mod auth;

#[derive(Clone, Serialize)]
#[allow(dead_code)]
struct Payload {
    data: bool,
}

#[cfg(debug_assertions)]
const USE_LOCALHOST_SERVER: bool = false;
#[cfg(not(debug_assertions))]
const USE_LOCALHOST_SERVER: bool = true;

lazy_static! {
    static ref LOCK_FILE_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
}

//#[cfg(any(target_os = "macos"))]
//static ACCESSIBILITY_PERMISSIONS_GRANTED: AtomicBool = AtomicBool::new(false);


// Auto-updater removed for open source version
// Users can manually download new releases from GitHub

// ===== Terminal Permission Commands =====

/// Get current terminal permissions configuration
#[tauri::command]
fn get_terminal_permissions() -> Result<serde_json::Value, String> {
    use crate::engine::terminal::permissions::get_permissions;
    let perms = get_permissions();
    Ok(serde_json::json!({
        "security_mode": format!("{:?}", perms.security_mode),
        "ask_mode": format!("{:?}", perms.ask_mode),
        "allowlist": perms.allowlist.iter().map(|e| serde_json::json!({
            "pattern": e.pattern,
            "description": e.description
        })).collect::<Vec<_>>(),
        "blocklist": perms.blocklist.clone(),
        "default_cwd": perms.default_cwd.clone(),
        "session_allowlist": perms.session_allowlist.iter().cloned().collect::<Vec<_>>()
    }))
}

/// Update terminal security mode
#[tauri::command]
fn set_terminal_security_mode(mode: String) -> Result<(), String> {
    use crate::engine::terminal::permissions::{get_permissions_mut, TerminalSecurityMode};
    let mut perms = get_permissions_mut();
    perms.security_mode = match mode.to_lowercase().as_str() {
        "deny" => TerminalSecurityMode::Deny,
        "allowlist" => TerminalSecurityMode::Allowlist,
        "full" => TerminalSecurityMode::Full,
        _ => return Err(format!("Invalid security mode: {}", mode))
    };
    info!("Terminal security mode set to: {:?}", perms.security_mode);
    Ok(())
}

/// Add a command to the permanent allowlist
#[tauri::command]
fn add_terminal_allowlist(command: String, description: Option<String>) -> Result<(), String> {
    use crate::engine::terminal::permissions::{get_permissions_mut, AllowlistEntry};
    let mut perms = get_permissions_mut();
    if let Some(desc) = description {
        perms.allowlist.push(AllowlistEntry::with_description(&command, &desc));
    } else {
        perms.allowlist.push(AllowlistEntry::new(&command));
    }
    info!("Added '{}' to terminal allowlist", command);
    Ok(())
}

/// Remove a command from the allowlist
#[tauri::command]
fn remove_terminal_allowlist(command: String) -> Result<(), String> {
    use crate::engine::terminal::permissions::get_permissions_mut;
    let mut perms = get_permissions_mut();
    let initial_len = perms.allowlist.len();
    perms.allowlist.retain(|e| e.pattern != command);
    if perms.allowlist.len() < initial_len {
        info!("Removed '{}' from terminal allowlist", command);
        Ok(())
    } else {
        Err(format!("Command '{}' not found in allowlist", command))
    }
}

/// Add a pattern to the blocklist
#[tauri::command]
fn add_terminal_blocklist(pattern: String) -> Result<(), String> {
    use crate::engine::terminal::permissions::get_permissions_mut;
    let mut perms = get_permissions_mut();
    perms.blocklist.push(pattern.clone());
    info!("Added '{}' to terminal blocklist", pattern);
    Ok(())
}

/// Respond to a terminal command approval request
#[tauri::command]
async fn respond_terminal_approval(request_id: String, decision: String, allow_always: bool) -> Result<(), String> {
    use crate::engine::terminal::executor;
    
    let decision_enum = match decision.to_lowercase().as_str() {
        "allow" => executor::ApprovalResponse::Allow,
        "allow_session" => executor::ApprovalResponse::AllowSession,
        "deny" => executor::ApprovalResponse::Deny,
        _ => return Err(format!("Invalid decision: {}", decision))
    };
    
    executor::resolve_approval(&request_id, decision_enum, allow_always).await;
    info!("Terminal approval {} resolved: {:?}", request_id, decision);
    Ok(())
}

/// Clear session-only allowlist (call when agent session ends)
#[tauri::command]
fn clear_terminal_session() -> Result<(), String> {
    use crate::engine::terminal::permissions::get_permissions_mut;
    let mut perms = get_permissions_mut();
    perms.session_allowlist.clear();
    info!("Terminal session allowlist cleared");
    Ok(())
}

/// Set default working directory for terminal commands
#[tauri::command]
fn set_terminal_default_cwd(cwd: Option<String>) -> Result<(), String> {
    use crate::engine::terminal::permissions::get_permissions_mut;
    let mut perms = get_permissions_mut();
    
    // Validate path if provided
    if let Some(ref path) = cwd {
        let expanded = if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&path[2..]).to_string_lossy().to_string()
            } else {
                path.clone()
            }
        } else {
            path.clone()
        };
        
        if !std::path::Path::new(&expanded).exists() {
            return Err(format!("Directory does not exist: {}", expanded));
        }
    }
    
    perms.default_cwd = cwd.clone();
    if let Err(e) = perms.save() {
        warn!("Failed to save terminal permissions: {}", e);
    }
    info!("Terminal default cwd set to: {:?}", cwd);
    Ok(())
}

#[tokio::main]
async fn main() {
    // Check for single instance before initializing anything else
    let lock_file_path = match check_single_instance() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    
    // Store lock file path for cleanup
    if let Ok(mut path) = LOCK_FILE_PATH.lock() {
        *path = Some(lock_file_path);
    }
    
    let port = 5173;
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_oauth::init());

    fix_path_env::fix_all_vars().expect("Failed to load env");
    // TODO: Re-enable system tray for Tauri v2
    // let tray = build_system_tray();

    let context = tauri::generate_context!();

    // TODO: Update for Tauri v2 - WindowUrl no longer exists
    // let url = format!("http://localhost:{}", port).parse().unwrap();
    // let window_url = WindowUrl::External(url);

    if USE_LOCALHOST_SERVER == true {
        // TODO: Update for Tauri v2 - AppUrl configuration has changed
        // context.config_mut().build.dist_dir = AppUrl::Url(window_url.clone());
        // context.config_mut().build.dev_path = AppUrl::Url(window_url.clone());
        builder = builder.plugin(tauri_plugin_localhost::Builder::new(port).build());
    }

    builder
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_oauth::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_positioner::init())
        // TODO: Re-enable system tray for Tauri v2
        // .system_tray(tray)
        // .on_system_tray_event(|app, event| match event {
        //     SystemTrayEvent::LeftClick { .. } => {
        //         let window = app.get_webview_window("main").unwrap();
        //         if window.is_visible().unwrap() {
        //             window.hide().unwrap();
        //         } else {
        //             window.show().unwrap();
        //             window.set_focus().unwrap();
        //         }
        //     }
        //     SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
        //         "start_stop_recording" => {
        //             let wrapped_window = app.get_webview_window("main");
        //             if let Some(window) = wrapped_window {
        //                 window
        //                     .emit("toggle_recording", Payload { data: true })
        //                     .unwrap();
        //             }
        //         }
        //         "quit" => {
        //             std::process::exit(0);
        //         }
        //         _ => {}
        //     },
        //     _ => {}
        // })
        .invoke_handler(tauri::generate_handler![
            refresh_activity_log,
            update_settings,
            get_latest_settings,
            record_single_activity,
            update_app_permissions,
            get_app_permissions,
            prompt_for_accessibility_permissions,
            check_accessibility_permissions,
            start_keyboard_monitoring,
            observe_app_for_actions,
            test_user_action_capture,
            test_user_actions,
            // Register new app automation commands
            launch_app,
            get_app_actionable_elements,
            perform_random_app_action,
            get_all_automations,
            get_automation_history,
            get_automation_execution_history,
            get_automation_script,
            start_recording_automation,
            stop_recording_automation,
            play_automation,
            stop_automation,
            continue_automation_task,
            update_automation_script,
            delete_automation,
            delete_execution_run,
            stop_recording,
            get_activity_history,
            delete_activity,
            save_automation,
            generate_synthetic_automation,
            create_agent_mode_automation,
            get_activity_full_text_by_id,
            get_activity_logs_by_automation,
            complete_takeover,
            submit_clarification,
            send_user_message,
            generate_intake_questions,
            save_intake_answers,
            get_automation_clarifications,
            // New commands for automation logs and summaries
            process_automation_logs,
            get_automation_logs,
            get_automation_summary,
            get_llm_script,
            generate_automation_summary_with_llm,
            update_all_automations_with_summaries,
            process_automation_events_with_llm,
            get_automation_by_id,
            test_keyboard_monitoring,
            authenticate_user,
            logout_user,
            get_current_user,
            get_usage_stats,
            get_usage_history,
            // Skills commands
            get_all_skills,
            create_skill,
            update_skill,
            delete_skill,
            toggle_skill_active,
            // Terminal permission commands
            get_terminal_permissions,
            set_terminal_security_mode,
            add_terminal_allowlist,
            remove_terminal_allowlist,
            add_terminal_blocklist,
            respond_terminal_approval,
            clear_terminal_session,
            set_terminal_default_cwd,
            // Schedule commands
            get_all_schedules,
            get_schedules_for_automation,
            create_schedule,
            update_schedule,
            delete_schedule,
            // Remote bridge commands (legacy WebSocket + new Supabase relay)
            crate::engine::remote_bridge::get_remote_bridge_status,
            crate::engine::remote_bridge::regenerate_pairing_code,
            crate::engine::remote_bridge::start_remote_bridge,
            crate::engine::remote_bridge::get_all_automations_command,
            crate::engine::remote_bridge::get_execution_history_command,
            crate::engine::remote_bridge::execute_remote_task,
            crate::engine::remote_bridge::stop_remote_task,
            crate::engine::remote_bridge::get_execution_details_command,
            crate::engine::remote_bridge::classify_desktop_prompt,
            crate::engine::remote_bridge::classify_continuation_prompt,
            // Telegram bot commands
            crate::engine::telegram_bot::set_telegram_bot_token,
            crate::engine::telegram_bot::set_telegram_allowed_users,
            crate::engine::telegram_bot::get_telegram_config,
            crate::engine::telegram_bot::disconnect_telegram_bot,
            // Discord bot commands
            crate::engine::discord_bot::set_discord_bot_token,
            crate::engine::discord_bot::set_discord_allowed_users,
            crate::engine::discord_bot::get_discord_config,
            crate::engine::discord_bot::disconnect_discord_bot,
            // OpenAI Codex (ChatGPT subscription) OAuth commands
            crate::auth::openai_codex_oauth::openai_codex_login,
            crate::auth::openai_codex_oauth::openai_codex_logout,
            crate::auth::openai_codex_oauth::openai_codex_status,
            // CLI probe commands
            get_detected_cli_tools,
            refresh_cli_probe,
        ])
        .manage(AppState {
            db: Default::default(),
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // Don't kill NVDA here since we're just hiding the window
                api.prevent_close();
                window.hide().unwrap(); // Hide window on close
            }
            tauri::WindowEvent::Destroyed => {
                // Clean up lock file when window is destroyed
                cleanup_lock_file();
            }
            _ => {}
        })
        .setup(move |app| {
            // Build tray menu
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit])?;
            
            // Build tray icon with unique ID to prevent duplicates
            let tray_icon = app.default_window_icon().unwrap().clone();
            
            let tray = TrayIconBuilder::with_id("heelix-main-tray")
                .menu(&menu)
                .icon(tray_icon)
                .menu_on_left_click(false)  // Don't show menu on left click
                .on_tray_icon_event(|tray, event| {
                    match event {
                        TrayIconEvent::Click { button, button_state, .. } => {
                            // Only handle the button UP event to avoid double-clicking behavior
                            if button == tauri::tray::MouseButton::Left && button_state == tauri::tray::MouseButtonState::Up {
                                let app = tray.app_handle();
                                if let Some(window) = app.get_webview_window("main") {
                                    let is_visible = window.is_visible().unwrap_or(false);
                                    let is_minimized = window.is_minimized().unwrap_or(false);
                                    let is_focused = window.is_focused().unwrap_or(false);
                                    
                                    // If window is hidden OR minimized OR not focused, show and focus it
                                    if !is_visible || is_minimized || !is_focused {
                                        let _ = window.show();
                                        if is_minimized {
                                            let _ = window.unminimize();
                                        }
                                        let _ = window.set_focus();
                                        
                                        // Add delay and temporary always on top to ensure it comes to foreground
                                        std::thread::sleep(std::time::Duration::from_millis(100));
                                        let _ = window.set_always_on_top(true);
                                        std::thread::sleep(std::time::Duration::from_millis(100));
                                        let _ = window.set_always_on_top(false);
                                    } else {
                                        // Window is visible, focused, and not minimized - hide it
                                        let _ = window.hide();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                })
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "quit" => {
                            cleanup_lock_file();
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;
            
            // Store tray reference to ensure proper cleanup
            app.manage(tray);
            
            let args: Vec<String> = env::args().collect();
            let should_start_minimized = args.contains(&"--minimized".to_string());

            let window = app.get_webview_window("main").unwrap();

            if should_start_minimized {
                window.hide().unwrap();
            } else {
                window.show().unwrap();
            }

            let app_handle = app.handle();
            
            // Initialize terminal executor with app handle for event emission
            engine::terminal::set_app_handle(app_handle.clone());
            
            prerequisites::check_and_install_prerequisites(
                app_handle
                    .path()
                    .resource_dir()
                    .unwrap()
                    .to_str()
                    .unwrap(),
            );
            
            // Continuous accessibility monitoring on macOS
            #[cfg(target_os = "macos")]
            {
                info!("🚀 Initializing continuous accessibility monitoring");
                
                // Initialize the continuous monitoring system
                #[cfg(target_os = "macos")]
                crate::window_details_collector::macos::macos_accessibility_engine::initialize_continuous_monitoring();
                
                // Set the app handle for direct DB access
                #[cfg(target_os = "macos")]
                crate::window_details_collector::macos::macos_accessibility_engine::set_app_handle(app_handle.clone());
                
                // Create a background thread to aggressively process accessibility events
                std::thread::spawn(move || {
                    info!("🔄 Starting accessibility event processing thread");
                    
                    // Keep track of the last active app to detect changes
                    let mut last_app_name = String::new();
                    let mut check_counter = 0;
                    
                    // Run a continuous loop for processing events
                    loop {
                        unsafe {
                            use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoopRunInMode};
                            use core_foundation::base::Boolean;
                            
                            // Process events for longer - critical for event capture
                            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 1.0, Boolean::from(false));
                        }
                        
                        // Periodically check if the active app changed (every 10 iterations = ~1 second)
                        check_counter += 1;
                        if check_counter >= 10 {
                            check_counter = 0;
                            
                            // Check for app changes
                            let window = crate::monitoring::active_windows::get_active_window();
                            if last_app_name != window.app_name {
                                info!("🔄 Main thread detected app change to: {} (PID: {})",
                                      window.app_name, window.process_id);
                                last_app_name = window.app_name.clone();
                                
                                // Re-observe the new app
                                #[cfg(target_os = "macos")]
                                crate::window_details_collector::macos::macos_accessibility_engine::observe_by_pid(
                                    &window.process_id.to_string()
                                );
                            }
                        }
                        
                        // Shorter sleep to be more responsive
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                });
            }
            
            setup_keypress_listener(&app_handle);
            init_app_permissions(app_handle.clone());

            // Start the local task scheduler (checks every 30s for due scheduled tasks)
            crate::engine::local_scheduler::start_scheduler(app_handle.clone());

            // Start the Telegram bot polling loop (no-op if token not yet configured)
            crate::engine::telegram_bot::start_telegram_bot(app_handle.clone());

            // Start the Discord bot gateway loop (no-op if token not yet configured)
            crate::engine::discord_bot::start_discord_bot(app_handle.clone());

            // Probe for available CLI tools in the background (results injected into agent context)
            tokio::spawn(async {
                crate::engine::cli_probe::probe_and_cache().await;
            });

            // Load model overrides from DB into the in-memory cache so every
            // LLM call uses the user's saved model without touching the call sites.
            {
                let m_claude        = app_handle.db(|db| get_setting(db, "model_claude").map(|s| s.setting_value).unwrap_or_default());
                let m_openai        = app_handle.db(|db| get_setting(db, "model_openai").map(|s| s.setting_value).unwrap_or_default());
                let m_openai_codex  = app_handle.db(|db| get_setting(db, "model_openai_codex").map(|s| s.setting_value).unwrap_or_default());
                let m_grok          = app_handle.db(|db| get_setting(db, "model_grok").map(|s| s.setting_value).unwrap_or_default());
                let m_gemini        = app_handle.db(|db| get_setting(db, "model_gemini").map(|s| s.setting_value).unwrap_or_default());
                let m_deepseek      = app_handle.db(|db| get_setting(db, "model_deepseek").map(|s| s.setting_value).unwrap_or_default());
                let codex_effort    = app_handle.db(|db| get_setting(db, "openai_codex_reasoning_effort").map(|s| s.setting_value).unwrap_or_default());
                crate::engine::provider_config::init_from_settings_with_codex(
                    &m_claude, &m_openai, &m_openai_codex, &m_grok, &m_gemini, &m_deepseek,
                );
                if !codex_effort.is_empty() {
                    crate::engine::provider_config::set_openai_codex_reasoning_effort(&codex_effort);
                }
            }
            
            // Set up event listener for stop_automation_request
            let app_handle_clone = app_handle.clone();
            let _id = app_handle.listen("stop_automation_request", move |event| {
                info!("Received stop_automation_request event");
                
                // Extract automation_id from the event payload if needed
                let payload_str = event.payload();
                if !payload_str.is_empty() {
                    info!("Stop automation request payload: {}", payload_str);
                }
                
                // Call the stop_execution function
                if let Err(e) = crate::engine::automation_agent_engine::stop_execution() {
                    error!("Failed to stop automation: {}", e);
                } else {
                    info!("Automation stop request processed successfully");
                    
                    // Emit confirmation event
                    let _ = app_handle_clone.emit("automation_stopped", serde_json::json!({
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }));
                }
            });

            // Auto-updater removed for open source version

            // Start the remote bridge WebSocket server for web app connections
            let app_handle_for_bridge = app.app_handle().clone();
            let bridge_state = crate::engine::remote_bridge::global_bridge_state();
            tauri::async_runtime::spawn(async move {
                info!("Starting remote bridge WebSocket server...");
                crate::engine::remote_bridge::start_bridge_server(app_handle_for_bridge, bridge_state).await;
            });

            Ok(())
        })
        .build(context)
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { has_visible_windows, .. } => {
                // When dock icon is clicked on macOS and no windows are visible
                if !has_visible_windows {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
            tauri::RunEvent::ExitRequested { api, .. } => {
                // Cleanup on exit
                cleanup_lock_file();

                // On Windows, allow the app to exit normally
                // On other platforms, prevent exit (app stays in system tray)
                #[cfg(not(target_os = "windows"))]
                api.prevent_exit();
            }
            _ => {}
        });
}

// TODO: Update system tray for Tauri v2
// fn build_system_tray() -> TrayIcon {
//     let quit = CustomMenuItem::new("quit".to_string(), "Quit");
//     let start_stop_recording =
//         CustomMenuItem::new("start_stop_recording".to_string(), "Start/Stop");
//     let tray_menu = SystemTrayMenu::new()
//       //  .add_item(start_stop_recording)
//         .add_item(quit);
//     SystemTray::new().with_menu(tray_menu)
// }

fn setup_keypress_listener(app_handle: &AppHandle) {
    let app_state: State<AppState> = app_handle.state();

    let db: Connection =
        database::initialize_database(&app_handle).expect("Database initialization failed!");
    
    // Clean up any stale running executions from previous app sessions
    if let Err(e) = crate::repository::automation_execution_repository::cleanup_stale_running_executions(&db) {
        error!("Failed to clean up stale running executions: {}", e);
    }

    // Seed default roles if they don't exist yet
    {
        let has_roles = db.query_row(
            "SELECT COUNT(*) FROM user_skills WHERE skill_type = 'role'",
            [],
            |row| row.get::<_, i64>(0)
        ).unwrap_or(0);

        if has_roles == 0 {
            info!("Seeding default roles...");
            for (name, description, content) in crate::engine::skills_registry::get_default_roles() {
                if let Err(e) = skill_repository::insert_default_skill(
                    &db, name, &crate::entity::skill::SkillType::Role, &[], &[], description, content
                ) {
                    error!("Failed to seed role '{}': {}", name, e);
                } else {
                    info!("Seeded default role: {}", name);
                }
            }
        }
    }

    // Update descriptions for existing default roles (backfill for users who already have them)
    {
        for (name, description, _content) in crate::engine::skills_registry::get_default_roles() {
            let _ = db.execute(
                "UPDATE user_skills SET description = ?1 WHERE name = ?2 AND is_default = 1 AND description = ''",
                rusqlite::params![description, name],
            );
        }
    }

    *app_state.db.lock().unwrap() = Some(db);
    
    // Keyboard monitoring will be started when user grants accessibility permissions
    // to avoid showing two permission dialogs at startup
}

#[tauri::command]
fn refresh_activity_log(app_handle: AppHandle, _action: &str) -> Result<Vec<ActivityItem>, ()> {
    return Ok(get_latest_activity_log(app_handle.clone()));
}

fn get_latest_activity_log(app_handle: AppHandle) -> Vec<ActivityItem> {
    return app_handle
        .db(|db| activity_log_repository::get_all_activity_logs(db))
        .unwrap_or_default();
}

#[tauri::command]
fn get_latest_settings(app_handle: AppHandle) -> Result<Vec<Setting>, ()> {
    info!("get_latest_settings: fetching latest settings from DB");
    let settings = app_handle.db(|db| get_settings(db).unwrap());
    info!("get_latest_settings: fetched {} settings", settings.len());
    return Ok(settings);
}

#[tauri::command]
async fn update_settings(app_handle: AppHandle, settings: Settings) {
    info!("update_settings: {:?}", settings);
    app_handle.db(|db| {
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("interval"),
                setting_value: format!("{}", settings.interval),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("is_dev_mode"),
                setting_value: format!("{}", settings.is_dev_mode),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("auto_start"),
                setting_value: format!("{}", settings.auto_start),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("api_choice"),
                setting_value: format!("{}", settings.api_choice),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("api_key_claude"),
                setting_value: format!("{}", settings.api_key_claude),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("api_key_claude_oauth"),
                setting_value: settings.api_key_claude_oauth.clone(),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("api_key_open_ai"),
                setting_value: format!("{}", settings.api_key_open_ai),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("api_key_grok"),
                setting_value: format!("{}", settings.api_key_grok),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("api_key_gemini"),
                setting_value: format!("{}", settings.api_key_gemini),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("use_pro_model"),
                setting_value: format!("{}", settings.use_pro_model),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("store_task_data"),
                setting_value: format!("{}", settings.store_task_data),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("model_claude"),
                setting_value: settings.model_claude.clone(),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("model_openai"),
                setting_value: settings.model_openai.clone(),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("model_openai_codex"),
                setting_value: settings.model_openai_codex.clone(),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("openai_codex_reasoning_effort"),
                setting_value: settings.openai_codex_reasoning_effort.clone(),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("model_grok"),
                setting_value: settings.model_grok.clone(),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("model_gemini"),
                setting_value: settings.model_gemini.clone(),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("api_key_deepseek"),
                setting_value: settings.api_key_deepseek.clone(),
            },
        )
        .unwrap();
        insert_or_update_setting(
            db,
            Setting {
                setting_key: String::from("model_deepseek"),
                setting_value: settings.model_deepseek.clone(),
            },
        )
        .unwrap();
    });

    crate::engine::provider_config::init_from_settings_with_codex(
        &settings.model_claude,
        &settings.model_openai,
        &settings.model_openai_codex,
        &settings.model_grok,
        &settings.model_gemini,
        &settings.model_deepseek,
    );
    if !settings.openai_codex_reasoning_effort.is_empty() {
        crate::engine::provider_config::set_openai_codex_reasoning_effort(
            &settings.openai_codex_reasoning_effort,
        );
    }
}

#[tauri::command]
fn init_app_permissions(app_handle: AppHandle) {
    init_permissions(app_handle);
}

#[tauri::command]
fn update_app_permissions(app_handle: AppHandle, app_path: String, allow: bool) {
    app_handle.db(|database| {
        update_permission(database, app_path, allow).expect("Failed to update permission");
    })
}

#[tauri::command]
fn get_app_permissions(app_handle: AppHandle) -> Result<Vec<Permission>, ()> {
    let permissions = app_handle.db(|database| get_permissions(database).unwrap());
    return Ok(permissions);
}

#[tauri::command]
async fn record_single_activity(
    _app_handle: AppHandle,
) -> Result<(), ()> {
    // Enable recording - this starts data collection in the accessibility engine
    #[cfg(target_os = "macos")]
    crate::window_details_collector::macos::macos_accessibility_engine::set_recording_enabled(true);
    
    info!("✅ Recording started - actions will be saved automatically in real-time");

    Ok(())
}

#[cfg(target_os = "macos")]
#[tauri::command]
fn prompt_for_accessibility_permissions() {
    unsafe {
        crate::window_details_collector::macos::macos_accessibility_engine::prompt_for_accessibility_permissions();
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn prompt_for_accessibility_permissions() {
    // No-op for non-macOS platforms
}

#[cfg(target_os = "macos")]
#[tauri::command]
fn check_accessibility_permissions() -> bool {
    crate::window_details_collector::macos::macos_accessibility_engine::check_accessibility_permissions()
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn check_accessibility_permissions() -> bool {
    true // Always return true on non-macOS platforms
}

#[cfg(target_os = "macos")]
#[tauri::command]
fn start_keyboard_monitoring() -> Result<String, String> {
    match crate::window_details_collector::macos::keyboard_monitor::start_keyboard_monitoring() {
        Ok(_) => {
            // Don't log every time - this is called frequently
            Ok("Keyboard monitoring started successfully".to_string())
        },
        Err(e) => {
            error!("❌ Failed to start keyboard monitoring: {}", e);
            Err(format!("Failed to start keyboard monitoring: {}", e))
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn start_keyboard_monitoring() -> Result<String, String> {
    Ok("Keyboard monitoring not needed on this platform".to_string())
}

#[tauri::command]
fn observe_app_for_actions(_app_handle: AppHandle, pid: &str) -> Result<String, String> {
    // Start observing the application for a short period
    #[cfg(target_os = "macos")]
    {
        use crate::window_details_collector::macos::macos_accessibility_engine::observe_by_pid;
        observe_by_pid(pid);
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Not implemented for other platforms
    }
    
    // Return any detected actions (will be collected during the next monitoring cycle)
    Ok("Started observing application. Actions will be collected in the next monitoring cycle.".to_string())
}

#[tauri::command]
async fn test_user_action_capture(_app_handle: AppHandle, target_app_name: &str) -> Result<String, String> {
    // Get the active window and check if it matches the requested app
    let active_window = crate::monitoring::active_windows::get_active_window();
    
    if !active_window.app_name.contains(target_app_name) {
        return Err(format!("Active window is {} not {}. Please switch to the target application.", 
            active_window.app_name, target_app_name));
    }
    
    let pid = active_window.process_id.to_string();
    info!("Found app {} with PID {}", active_window.app_name, pid);
    
    // First capture the current state before observing
    let (initial_element_tree, _) = 
        crate::window_details_collector::window_details_collector::get_element_tree_by_window_app_name(&pid);
    
    info!("Initial content captured. Starting observation...");
    
    // Start observing the application
    #[cfg(target_os = "macos")]
    {
        use crate::window_details_collector::macos::macos_accessibility_engine::observe_by_pid;
        observe_by_pid(&pid);
    }
    
    // Provide instructions to the user
    let instructions = format!(
        "Now observing '{}' for 5 seconds. Please perform some actions in the application (click buttons, type, navigate).", 
        active_window.app_name
    );
    info!("{}", instructions);
    
    // Wait longer to collect more events
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    // Get the collected data after user actions
    let (element_tree_after, actions) = 
        crate::window_details_collector::window_details_collector::get_element_tree_by_window_app_name(&pid);
    
    // Create a summary with before/after comparison
    let content_changed = initial_element_tree != element_tree_after;
    
    let summary = format!(
        "Application: {}\nWindow Title: {}\n\n=== ACTIONS DETECTED ===\n{}\n\n=== SCREEN CONTENT ===\nContent changed: {}\n\nBEFORE (sample):\n{}\n\nAFTER (sample):\n{}", 
        active_window.app_name,
        active_window.title,
        if actions.is_empty() { "No actions detected" } else { &actions },
        content_changed,
        initial_element_tree.chars().take(300).collect::<String>(),
        element_tree_after.chars().take(300).collect::<String>()
    );
    
    info!("Capture results: {}", summary);
    
    Ok(summary)
}

#[tauri::command]
async fn test_user_actions(_app_handle: AppHandle, app_name: &str) -> Result<String, String> {
    use crate::monitoring::active_windows::get_active_window;
    #[cfg(target_os = "macos")]
    use crate::window_details_collector::macos::macos_accessibility_engine::observe_by_pid;
    use crate::window_details_collector::window_details_collector::get_element_tree_by_window_app_name;
    use tokio::time::Duration;
    
    // Get the active window and check if it matches the requested app
    let active_window = get_active_window();
    
    if !active_window.app_name.contains(app_name) {
        return Err(format!("Active window is {} not {}. Please switch to the target application.", 
            active_window.app_name, app_name));
    }
    
    let pid = active_window.process_id.to_string();
    
    info!("Testing actions for {} (PID: {})", active_window.app_name, pid);
    info!("Please interact with the application for 5 seconds...");
    
    // Start observing
    #[cfg(target_os = "macos")]
    observe_by_pid(&pid);
    
    // Wait for user to perform actions
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Get the element tree and detected actions
    let (element_tree, actions) = get_element_tree_by_window_app_name(&pid);
    
    // Return useful information
    let result = format!(
        "App: {}\nWindow: {}\n\nDetected Actions:\n{}\n\nScreen Content (first 200 chars):\n{}",
        active_window.app_name,
        active_window.title,
        if actions.is_empty() { "No actions detected" } else { &actions },
        element_tree.chars().take(200).collect::<String>()
    );
    
    Ok(result)
}

// Automation Commands
#[tauri::command]
fn get_all_automations(app_handle: AppHandle) -> Result<Vec<Automation>, String> {
    app_handle
        .db(|db| automation_repository::get_all_automations(db))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_automation_history(app_handle: AppHandle) -> Result<Vec<Automation>, String> {
    app_handle
        .db(|db| automation_repository::get_automation_history(db))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_automation_execution_history(app_handle: AppHandle, limit: i32) -> Result<Vec<ExecutionRunWithSteps>, String> {
    app_handle
        .db(|db| automation_execution_repository::get_recent_execution_runs_with_steps(db, limit))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_automation_script(app_handle: AppHandle, automation_id: i64) -> Result<AutomationScript, String> {
    app_handle
        .db(|db| automation_repository::get_automation_script(db, automation_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Automation not found".to_string())
}

#[tauri::command]
fn start_recording_automation(app_handle: AppHandle, objective: String, name: String) -> Result<(), String> {
    info!("🟢 start_recording_automation called with name: {}, objective: {}", name, objective);

    // First, validate that we have a valid API key configured
    // Get API choice setting
    let api_choice = app_handle
        .db(|db| get_setting(db, "api_choice"))
        .map_err(|e| format!("Failed to get API choice: {}", e))?
        .setting_value;

    // Check the appropriate API key based on the choice
    let api_key_status = match api_choice.as_str() {
        "proxy" => {
            // Check if user is authenticated for proxy
            let user_id = app_handle
                .db(|db| match get_setting(db, "user_id") {
                    Ok(setting) => Some(setting.setting_value),
                    Err(_) => None,
                })
                .unwrap_or_default();

            if user_id.is_empty() {
                return Err("User not authenticated. Please login first to use the proxy API.".to_string());
            }

            // Check for valid auth token
            let has_token = app_handle
                .db(|db| crate::repository::user_auth_repository::get_valid_auth_token(db, &user_id))
                .map_err(|e| format!("Failed to check auth token: {}", e))?
                .is_some();

            if !has_token {
                return Err("Authentication expired. Please login again to use the proxy API.".to_string());
            }

            Ok(())
        },
        "openai" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_open_ai"))
                .map_err(|e| format!("Failed to get OpenAI API key setting: {}", e))?
                .setting_value;

            if key.is_empty() {
                Err("OpenAI API key is not configured. Please add your OpenAI API key in settings.".to_string())
            } else {
                Ok(())
            }
        },
        "grok" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_grok"))
                .map_err(|e| format!("Failed to get Grok API key setting: {}", e))?
                .setting_value;

            if key.is_empty() {
                Err("Grok API key is not configured. Please add your Grok API key in settings.".to_string())
            } else {
                Ok(())
            }
        },
        "deepseek" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_deepseek"))
                .map_err(|e| format!("Failed to get DeepSeek API key setting: {}", e))?
                .setting_value;

            if key.is_empty() {
                Err("DeepSeek API key is not configured. Please add your DeepSeek API key in settings.".to_string())
            } else {
                Ok(())
            }
        },
        "claude" | _ => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_claude"))
                .map_err(|e| format!("Failed to get Claude API key setting: {}", e))?
                .setting_value;

            if key.is_empty() {
                Err("Claude API key is not configured. Please add your Claude API key in settings.".to_string())
            } else {
                Ok(())
            }
        }
    };

    // Return early if API key validation failed
    api_key_status?;

    info!("✅ API key validation successful for provider: {}", api_choice);

    // Store a placeholder for the automation we're recording
    // The raw script will be populated when recording stops
    let automation_id = app_handle
        .db(|db| {
            automation_repository::save_automation(
                db,
                &name,
                &objective,
                "", // Empty raw script for now
                "", // Empty generalized script for now
            )
        })
        .map_err(|e| e.to_string())?;
    
    #[cfg(target_os = "macos")]
    {
        // Set the current automation ID in the accessibility engine
        crate::window_details_collector::macos::macos_accessibility_engine::set_current_automation_id(automation_id);
        
        // Enable recording to start collecting events
        crate::window_details_collector::macos::macos_accessibility_engine::set_recording_enabled(true);
    }
    
    // Emit an event to notify frontend recording has started
    // IMPORTANT: Explicitly set isProcessing to false to clear any lingering state
    app_handle
        .emit("recording_status", serde_json::json!({
            "isRecording": true,
            "isProcessing": false,  // Clear any lingering processing state
            "automationId": automation_id
        }))
        .map_err(|e| e.to_string())?;
    
    info!("✅ start_recording_automation completed successfully - recording is now active");
    
    Ok(())
}

#[tauri::command]
async fn stop_recording_automation(app_handle: AppHandle) -> Result<i64, String> {
    info!("stop_recording_automation called - beginning to process automation");
    
    #[cfg(target_os = "macos")]
    {
        // CRITICAL: Ensure recording is stopped immediately with multiple approaches
        // Disable recording using the primary method
        crate::window_details_collector::macos::macos_accessibility_engine::set_recording_enabled(false);
        info!("🛑 First attempt to stop recording complete");
        
        // Wait a tiny bit to ensure it takes effect
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        // Second explicit call to make absolutely sure recording is disabled
        crate::window_details_collector::macos::macos_accessibility_engine::set_recording_enabled(false);
        info!("🛑 Second attempt to stop recording complete");
        
        // Third attempt - really make sure it's off
        crate::window_details_collector::macos::macos_accessibility_engine::set_recording_enabled(false);
        info!("🛑 Third attempt to stop recording complete - recording should now be definitively stopped");
    }
    
    // Get the automation that was being recorded
    #[cfg(target_os = "macos")]
    let automation_id_to_process = {
        // Get the automation ID that was set during start_recording_automation
        match crate::window_details_collector::macos::macos_accessibility_engine::get_current_automation_id() {
            Some(id) => {
                info!("Using stored automation ID from recording session: {}", id);
                id
            },
            None => {
                // Fallback to the most recently created automation
                warn!("No current automation ID found, falling back to most recently created automation");
                app_handle
                    .db(|db| -> Result<i64, rusqlite::Error> {
                        // Get automations ordered by ID DESC (most recently created first)
                        let mut stmt = db.prepare("SELECT id FROM automations ORDER BY id DESC LIMIT 1")?;
                        let id: i64 = stmt.query_row([], |row| row.get(0))?;
                        Ok(id)
                    })
                    .map_err(|e| format!("Failed to get most recent automation: {}", e))?
            }
        }
    };

    #[cfg(not(target_os = "macos"))]
    let automation_id_to_process = {
        // For non-macOS platforms, use the most recently created automation
        app_handle
            .db(|db| -> Result<i64, rusqlite::Error> {
                let mut stmt = db.prepare("SELECT id FROM automations ORDER BY id DESC LIMIT 1")?;
                let id: i64 = stmt.query_row([], |row| row.get(0))?;
                Ok(id)
            })
            .map_err(|e| format!("Failed to get most recent automation: {}", e))?
    };

    // Get the automation details using the correct ID
    let latest_automation = app_handle
        .db(|db| automation_repository::get_automation_by_id(db, automation_id_to_process))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Automation with ID {} not found", automation_id_to_process))?;

    info!("Processing automation ID: {}, Name: {}", latest_automation.id, latest_automation.name);
    
    // Emit a status update that processing has started
    // IMPORTANT: Also explicitly set isRecording to false to ensure UI state is correct
    app_handle
        .emit("recording_status", serde_json::json!({
            "isRecording": false,
            "isProcessing": true,
            "automationId": latest_automation.id,
            "status": "Processing automation events with AI..."
        }))
        .map_err(|e| e.to_string())?;
    
    info!("Emitted processing started event for automation ID: {}", latest_automation.id);
    
    // Step 1: Process raw logs into a structured, filtered generalized_script
    match crate::engine::automation_summary_engine::process_automation_with_llm(&app_handle, latest_automation.id).await {
        Ok((filtered_json, questions)) => {
            info!("Successfully processed automation {} with LLM filtering, JSON size: {} bytes, {} questions generated", 
                  latest_automation.id, filtered_json.len(), questions.len());
            
            // Step 2: Generate human-readable description using the filtered data
            match crate::engine::automation_summary_engine::generate_automation_summary_with_llm(&app_handle, latest_automation.id).await {
                Ok(human_summary) => {
                    info!("Successfully generated human-readable summary for automation {}", latest_automation.id);
                    
                    // Store the human summary in the automation's nl_description field
                    app_handle
                        .db_mut(|db| {
                            db.execute(
                                "UPDATE automations SET nl_description = ?, updated_at = datetime('now') WHERE id = ?",
                                [&human_summary, &latest_automation.id.to_string()]
                            )
                            .map(|_| ())
                            .map_err(|e| format!("Failed to update automation: {}", e))
                        })
                        .map_err(|e| format!("Database error: {}", e))?;
                },
                Err(e) => {
                    error!("Failed to generate human-readable summary: {}", e);
                    // Continue despite error - we still have the filtered script
                }
            }
            
            // Emit an event to notify frontend processing is complete
            app_handle
                .emit("recording_status", serde_json::json!({
                    "isRecording": false,
                    "isProcessing": false,
                    "automationId": latest_automation.id,
                    "status": "Automation processed successfully"
                }))
                .map_err(|e| e.to_string())?;
            
            info!("Emitted processing complete event for automation ID: {}", latest_automation.id);
            
            // Use the questions generated during script analysis
            if !questions.is_empty() {
                info!("Using {} intake questions generated during script analysis", questions.len());
                
                // Get the latest automation data including the summary
                let updated_automation = app_handle
                    .db(|db| automation_repository::get_automation_by_id(db, latest_automation.id))
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "Failed to retrieve updated automation".to_string())?;
                
                // Emit event to open intake wizard
                app_handle
                    .emit("intake_wizard_open", serde_json::json!({
                        "questions": questions,
                        "objective": updated_automation.objective,
                        "name": updated_automation.name,
                        "automationId": latest_automation.id
                    }))
                    .map_err(|e| format!("Failed to emit intake wizard event: {}", e))?;
            } else {
                info!("No intake questions needed for automation {}", latest_automation.id);
            }
            
            Ok(latest_automation.id)
        },
        Err(e) => {
            error!("Failed to process automation with LLM: {}", e);
            
            // Handle error but still return success to the user
            // We'll use a simplified format instead
            let placeholder_json = format!(
                r#"{{
                    "automation_id": {},
                    "name": "{}",
                    "steps": [
                        {{
                            "step_number": 1,
                            "timestamp": "{}",
                            "app_name": "System",
                            "action": "Started automation recording",
                            "element_info": null,
                            "screen_context": "Automation interface",
                            "raw_action": null
                        }}
                    ]
                }}"#, 
                latest_automation.id,
                latest_automation.name,
                chrono::Local::now().to_rfc3339()
            );
            
            // Update with the placeholder
            app_handle
                .db_mut(|db| {
                    db.execute(
                        "UPDATE automations SET generalized_script = ?, nl_description = ?, updated_at = datetime('now') WHERE id = ?",
                        rusqlite::params![placeholder_json, "Error processing automation", latest_automation.id]
                    )
                    .map(|_| ())
                    .map_err(|db_err| format!("Failed to update automation: {}", db_err))
                })
                .map_err(|db_err| format!("Database error: {}", db_err))?;
            
            info!("Updated with placeholder for failed automation ID: {}", latest_automation.id);
            
            // Emit event that processing failed
            app_handle
                .emit("recording_status", serde_json::json!({
                    "isRecording": false,
                    "isProcessing": false,
                    "automationId": latest_automation.id,
                    "status": "Automation processed with errors",
                    "error": format!("LLM processing failed: {}", e)
                }))
                .map_err(|emit_err| emit_err.to_string())?;
            
            info!("Emitted processing error event for automation ID: {}", latest_automation.id);
            
            // Still return success to allow the user to continue
            // Final safety check: ensure recording is completely disabled
            #[cfg(target_os = "macos")]
            crate::window_details_collector::macos::macos_accessibility_engine::set_recording_enabled(false);
            info!("✅ stop_recording_automation completed - final recording state check done");
            
            Ok(latest_automation.id)
        }
    }
}

#[tauri::command]
fn play_automation(app_handle: AppHandle, automation_id: i64, additional_instructions: Option<String>, agent_mode: Option<bool>) -> Result<(), String> {
    // Get the automation
    let _automation = app_handle
        .db(|db| automation_repository::get_automation_by_id(db, automation_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Automation not found".to_string())?;
    
    // Log additional instructions if provided
    if let Some(instructions) = &additional_instructions {
        if !instructions.is_empty() {
            info!("Additional instructions for automation {}: {}", automation_id, instructions);
        }
    }
    
    let use_agent_mode = agent_mode.unwrap_or(false);

    // Emit an event to notify frontend playback has started
    app_handle
        .emit("playback_status", serde_json::json!({
            "isPlaying": true,
            "progress": 0,
            "agentMode": use_agent_mode,
            "statusMessage": if use_agent_mode { "Starting agent mode..." } else { "Starting automation..." }
        }))
        .map_err(|e| format!("Failed to emit playback status: {}", e))?;

    // Run the automation in a background thread
    let handle_clone = app_handle.clone();
    let additional_instructions_clone = additional_instructions.clone();

    // Spawn a new async task for execution
    tauri::async_runtime::spawn(async move {
        let result = if use_agent_mode {
            crate::engine::automation_agent_engine::execute_automation_agent_mode(
                &handle_clone,
                automation_id,
                additional_instructions_clone
            ).await
        } else {
            crate::engine::automation_agent_engine::execute_automation(
                &handle_clone,
                automation_id,
                additional_instructions_clone
            ).await
        };
        match result {
            Ok(_) => {
                info!("Automation execution completed successfully");
                let _ = handle_clone.emit("playback_status", serde_json::json!({
                    "isPlaying": false,
                    "progress": 100,
                    "statusMessage": "Automation completed successfully"
                }));
            },
            Err(e) => {
                error!("Automation execution failed: {}", e);
                let _ = handle_clone.emit("playback_status", serde_json::json!({
                    "isPlaying": false,
                    "progress": 0,
                    "statusMessage": format!("Automation failed: {}", e)
                }));
                // Emit automation_stopped as safety net so frontend clears isPlaying
                let _ = handle_clone.emit("automation_stopped", serde_json::json!({}));
                // Emit automation_failed event for usage tracking
                let _ = handle_clone.emit("automation_failed", ());
            }
        }
    });
    
    Ok(())
}

#[tauri::command]
fn stop_automation(app_handle: AppHandle) -> Result<(), String> {
    use crate::engine::automation_agent_engine::{stop_execution, finalize_execution_run};

    // Stop the agent execution
    stop_execution()?;

    // Finalize the execution run in the database (mark as stopped)
    // No error message since this is an intentional user action, not an error
    finalize_execution_run(&app_handle, "stopped", None);

    // Clear recording state (to prevent UI showing both recording and playing)
    #[cfg(target_os = "macos")]
    crate::window_details_collector::macos::macos_accessibility_engine::set_recording_enabled(false);

    // Emit an event to notify frontend playback has stopped
    app_handle
        .emit("playback_status", serde_json::json!({
            "isPlaying": false,
            "progress": 0.0,
            "statusMessage": "Automation stopped"
        }))
        .map_err(|e| e.to_string())?;

    // Emit automation_stopped event for usage tracking
    app_handle
        .emit("automation_stopped", ())
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn continue_automation_task(
    app_handle: AppHandle,
    automation_id: i64,
    execution_run_id: i64,
    continuation_prompt: String,
    previous_objective: String,
    previous_completion_message: String,
    recent_steps: Vec<String>,
) -> Result<(), String> {
    use crate::engine::script_generator::ScriptGenerator;
    
    info!("Continuing automation {} (run {}) with prompt: {}", automation_id, execution_run_id, continuation_prompt);
    
    // Emit planning status
    app_handle
        .emit("playback_status", serde_json::json!({
            "isPlaying": true,
            "progress": 0,
            "statusMessage": "Planning continuation..."
        }))
        .map_err(|e| format!("Failed to emit playback status: {}", e))?;
    
    // Get API settings for the planner
    let api_choice = app_handle
        .db(|db| get_setting(db, "api_choice"))
        .map(|s| s.setting_value)
        .unwrap_or_else(|_| "Claude".to_string());
    
    let api_key_setting = match api_choice.as_str() {
        "OpenAI" => "api_key_open_ai",
        "Gemini" => "api_key_gemini",
        "Grok" => "api_key_grok",
        _ => "api_key_claude",
    };
    
    let api_key = app_handle
        .db(|db| get_setting(db, api_key_setting))
        .map(|s| s.setting_value)
        .unwrap_or_default();
    
    // Generate continuation plan using the dedicated planner
    let script_generator = ScriptGenerator::new(api_key, api_choice);
    
    let (continuation_plan, continuation_name) = script_generator
        .generate_continuation_plan(
            &previous_objective,
            &previous_completion_message,
            &recent_steps,
            &continuation_prompt,
        )
        .await
        .map_err(|e| format!("Failed to generate continuation plan: {}", e))?;
    
    info!("Generated continuation plan '{}' with {} steps", continuation_name, continuation_plan.lines().count());
    
    // Build synthesized objective: truncated completion message + new goal
    // The completion message already summarizes what was accomplished — no need for a separate context blob
    let truncated_completion = if previous_completion_message.len() > 300 {
        format!("{}...", &previous_completion_message[..300])
    } else if previous_completion_message.is_empty() {
        "(task was stopped)".to_string()
    } else {
        previous_completion_message.clone()
    };
    let synthesized_objective = format!("Previously: {} New goal: {}", truncated_completion, continuation_prompt);
    info!("Synthesized objective: {}", &synthesized_objective[..synthesized_objective.len().min(100)]);
    
    // Re-open the execution run for continuation
    app_handle
        .db(|db| automation_execution_repository::reopen_execution_run(db, execution_run_id))
        .map_err(|e| format!("Failed to reopen execution run: {}", e))?;
    
    // Get current step count to store the user prompt at the right position
    let current_step_count = app_handle
        .db(|db| automation_execution_repository::get_step_count(db, execution_run_id))
        .map_err(|e| format!("Failed to get step count: {}", e))?;
    
    // Store the continuation prompt as a user_prompt step
    let continuation_prompt_clone = continuation_prompt.clone();
    app_handle
        .db(move |db| automation_execution_repository::create_execution_step_with_type(
            db,
            execution_run_id,
            current_step_count, // This will be the next step number
            Some(continuation_prompt_clone),
            None,
            "completed",
            "user_prompt"
        ))
        .map_err(|e| format!("Failed to store continuation prompt: {}", e))?;
    
    // Emit status update
    app_handle
        .emit("playback_status", serde_json::json!({
            "isPlaying": true,
            "progress": 5,
            "statusMessage": format!("Executing: {}", continuation_name)
        }))
        .map_err(|e| format!("Failed to emit playback status: {}", e))?;
    
    // Run the automation continuation in a background thread
    let handle_clone = app_handle.clone();
    
    // Spawn a new async task for execution - continue existing run
    tauri::async_runtime::spawn(async move {
        match crate::engine::automation_agent_engine::execute_automation_continuation(
            &handle_clone, 
            automation_id,
            execution_run_id,
            continuation_plan,
            continuation_name.clone(),
            synthesized_objective,
            continuation_prompt, // Raw prompt for completion message generation
        ).await {
            Ok(_) => {
                info!("Automation continuation completed successfully");
                let _ = handle_clone.emit("playback_status", serde_json::json!({
                    "isPlaying": false,
                    "progress": 100,
                    "statusMessage": "Continuation completed successfully"
                }));
            },
            Err(e) => {
                error!("Automation continuation failed: {}", e);
                let _ = handle_clone.emit("playback_status", serde_json::json!({
                    "isPlaying": false,
                    "progress": 0,
                    "statusMessage": format!("Continuation failed: {}", e)
                }));
                let _ = handle_clone.emit("automation_failed", ());
            }
        }
    });
    
    Ok(())
}

#[tauri::command]
fn complete_takeover(app_handle: AppHandle) -> Result<(), String> {
    use crate::engine::automation_agent_engine::complete_takeover;
    
    info!("Frontend signaled takeover completion");
    
    // Complete the takeover
    complete_takeover()?;
    
    // Emit an event to confirm takeover was completed
    app_handle
        .emit("takeover_completed", serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn submit_clarification(app_handle: AppHandle, response: String) -> Result<(), String> {
    use crate::engine::automation_agent_engine::submit_clarification;
    
    info!("Frontend submitted clarification: {}", response);
    
    // Submit the clarification
    submit_clarification(response)?;
    
    // Emit an event to confirm clarification was received
    app_handle
        .emit("clarification_received", serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn send_user_message(message: String) -> Result<(), String> {
    use crate::engine::automation_agent_engine::send_user_message;
    info!("Frontend sent user message during execution: {}", message);
    send_user_message(message)?;
    Ok(())
}

#[tauri::command]
fn update_automation_script(app_handle: AppHandle, automation_id: i64, script: AutomationScript) -> Result<(), String> {
    app_handle
        .db(|db| automation_repository::update_automation_script(db, automation_id, &script))
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn delete_automation(app_handle: AppHandle, automation_id: i64) -> Result<(), String> {
    app_handle
        .db(|db| automation_repository::delete_automation(db, automation_id))
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn delete_execution_run(app_handle: AppHandle, run_id: i64) -> Result<(), String> {
    app_handle
        .db(|db| crate::repository::automation_execution_repository::delete_execution_run(db, run_id))
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

// ============================================================
// SKILLS COMMANDS
// ============================================================

#[tauri::command]
fn get_all_skills(app_handle: AppHandle) -> Result<Vec<Skill>, String> {
    app_handle
        .db(|db| skill_repository::get_all_skills(db))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn create_skill(
    app_handle: AppHandle,
    name: String,
    skill_type: String,
    domains: Vec<String>,
    triggers: Vec<String>,
    description: String,
    content: String,
) -> Result<i64, String> {
    let input = SkillInput {
        name,
        skill_type: SkillType::from(skill_type.as_str()),
        domains,
        triggers,
        description,
        content,
        is_active: true,
    };
    
    let result = app_handle
        .db(|db| skill_repository::create_skill(db, &input))
        .map_err(|e| e.to_string())?;
    
    // Emit event to refresh skills list in UI
    let _ = app_handle.emit("skill_changed", serde_json::json!({"action": "created", "id": result}));
    
    Ok(result)
}

#[tauri::command]
fn update_skill(
    app_handle: AppHandle,
    skill_id: i64,
    name: String,
    skill_type: String,
    domains: Vec<String>,
    triggers: Vec<String>,
    description: String,
    content: String,
    is_active: bool,
) -> Result<(), String> {
    let input = SkillInput {
        name,
        skill_type: SkillType::from(skill_type.as_str()),
        domains,
        triggers,
        description,
        content,
        is_active,
    };
    
    app_handle
        .db(|db| skill_repository::update_skill(db, skill_id, &input))
        .map_err(|e| e.to_string())?;
    
    // Emit event to refresh skills list in UI
    let _ = app_handle.emit("skill_changed", serde_json::json!({"action": "updated", "id": skill_id}));
    
    Ok(())
}

#[tauri::command]
fn delete_skill(app_handle: AppHandle, skill_id: i64) -> Result<bool, String> {
    let result = app_handle
        .db(|db| skill_repository::delete_skill(db, skill_id))
        .map_err(|e| e.to_string())?;
    
    // Emit event to refresh skills list in UI
    let _ = app_handle.emit("skill_changed", serde_json::json!({"action": "deleted", "id": skill_id}));
    
    Ok(result)
}

#[tauri::command]
fn toggle_skill_active(app_handle: AppHandle, skill_id: i64, is_active: bool) -> Result<(), String> {
    app_handle
        .db(|db| skill_repository::toggle_skill_active(db, skill_id, is_active))
        .map_err(|e| e.to_string())?;
    
    // Emit event to refresh skills list in UI
    let _ = app_handle.emit("skill_changed", serde_json::json!({"action": "toggled", "id": skill_id}));
    
    Ok(())
}

// ============================================================
// Schedule Commands
// ============================================================

#[tauri::command]
fn get_all_schedules(app_handle: AppHandle) -> Result<Vec<ScheduleWithName>, String> {
    app_handle
        .db(|db| schedule_repository::get_all_schedules_with_names(db))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_schedules_for_automation(app_handle: AppHandle, automation_id: i64) -> Result<serde_json::Value, String> {
    let schedules = app_handle
        .db(|db| schedule_repository::get_schedules_for_automation(db, automation_id))
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(schedules).unwrap_or_default())
}

#[tauri::command]
fn create_schedule(
    app_handle: AppHandle,
    automation_id: i64,
    recurrence_type: String,
    recurrence_days: Option<Vec<i32>>,
    execution_hour: i32,
    execution_minute: Option<i32>,
    timezone: String,
    persistent_run_id: Option<i64>,
    continuation_prompt: Option<String>,
) -> Result<i64, String> {
    let minute = execution_minute.unwrap_or(0);
    let id = app_handle
        .db(|db| schedule_repository::create_schedule(
            db, automation_id, &recurrence_type, recurrence_days, execution_hour, minute, &timezone,
            persistent_run_id, continuation_prompt.as_deref(),
        ))
        .map_err(|e| e.to_string())?;

    let _ = app_handle.emit("schedule_changed", serde_json::json!({"action": "created", "id": id}));
    Ok(id)
}

#[tauri::command]
fn update_schedule(
    app_handle: AppHandle,
    schedule_id: i64,
    is_active: Option<bool>,
    recurrence_type: Option<String>,
    recurrence_days: Option<Vec<i32>>,
    execution_hour: Option<i32>,
    execution_minute: Option<i32>,
) -> Result<(), String> {
    app_handle
        .db(|db| schedule_repository::update_schedule(
            db, schedule_id, is_active,
            recurrence_type.as_deref(), recurrence_days,
            execution_hour, execution_minute,
        ))
        .map_err(|e| e.to_string())?;

    let _ = app_handle.emit("schedule_changed", serde_json::json!({"action": "updated", "id": schedule_id}));
    Ok(())
}

#[tauri::command]
fn delete_schedule(app_handle: AppHandle, schedule_id: i64) -> Result<(), String> {
    app_handle
        .db(|db| schedule_repository::delete_schedule(db, schedule_id))
        .map_err(|e| e.to_string())?;

    let _ = app_handle.emit("schedule_changed", serde_json::json!({"action": "deleted", "id": schedule_id}));
    Ok(())
}

// Helper functions for automation system
#[allow(dead_code)]
fn get_recorded_actions() -> String {
    // In a real implementation, this would return the actions recorded
    // during the recording session. For now, we'll return a dummy script.
    r#"[
        {"action": "click", "target": "button.login", "description": "Click login button"},
        {"action": "type", "target": "input[name='username']", "description": "Type username", "params": {"text": "user123"}},
        {"action": "type", "target": "input[name='password']", "description": "Type password", "params": {"text": "password123"}},
        {"action": "click", "target": "button[type='submit']", "description": "Click submit button"}
    ]"#.to_string()
}

#[allow(dead_code)]
fn generate_generalized_script(_objective: &str, raw_script: &str) -> Result<String, String> {
    // In a real implementation, this would send the raw script to an LLM
    // to generate a generalized script. For now, we'll return the raw script.
    Ok(raw_script.to_string())
}

/// Enhance the automation's natural language description with clarification answers
async fn enhance_nl_description_with_clarifications(
    app_handle: &AppHandle,
    current_description: &str,
    objective: &str,
    qa_pairs: &[(String, String)]
) -> Result<String, String> {
    use crate::configuration::state::ServiceAccess;
    use crate::repository::settings_repository::get_setting;
    
    // Get API choice setting
    let api_choice = app_handle
        .db(|db| get_setting(db, "api_choice").expect("Failed to get API choice"))
        .setting_value;
    
    // Get the appropriate API key based on the choice
    let (api_key, provider_name) = match api_choice.as_str() {
        "proxy" => {
            // Get JWT token for proxy authentication
            let user_id = app_handle
                .db(|db| match get_setting(db, "user_id") {
                    Ok(setting) => Some(setting.setting_value),
                    Err(_) => None,
                })
                .unwrap_or_default();
            
            if user_id.is_empty() {
                return Err("User not authenticated - please login first".to_string());
            }
            
            // Get valid auth token for the user
            let jwt_token = app_handle
                .db(|db| crate::repository::user_auth_repository::get_valid_auth_token(db, &user_id))
                .map_err(|e| format!("Failed to get auth token: {}", e))?
                .ok_or_else(|| "Authentication expired - please login again".to_string())?;
                
            (jwt_token, "Heelix Cloud")
        },
        "openai" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_open_ai").expect("Failed to get OpenAI API key"))
                .setting_value;
            if key.is_empty() {
                return Err("OpenAI API key is not configured".to_string());
            }
            (key, "OpenAI")
        },
        "grok" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_grok").expect("Failed to get Grok API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Grok API key is not configured".to_string());
            }
            (key, "Grok")
        },
        "deepseek" => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_deepseek").expect("Failed to get DeepSeek API key"))
                .setting_value;
            if key.is_empty() {
                return Err("DeepSeek API key is not configured".to_string());
            }
            (key, "DeepSeek")
        },
        "claude" | _ => {
            let key = app_handle
                .db(|db| get_setting(db, "api_key_claude").expect("Failed to get Claude API key"))
                .setting_value;
            if key.is_empty() {
                return Err("Claude API key is not configured".to_string());
            }
            (key, "Claude")
        }
    };
    
    // Create prompt for enhancing the description
    let mut prompt = format!(
        "## Automation Enhancement Task\n\n\
        I need to enhance an automation's description based on clarification answers from the user.\n\n\
        **Objective**: {}\n\n\
        **Current Description**:\n{}\n\n\
        **Clarification Q&A**:\n",
        objective, current_description
    );
    
    // Add Q&A pairs
    for (i, (question, answer)) in qa_pairs.iter().enumerate() {
        prompt.push_str(&format!("{}. Q: {}\n   A: {}\n\n", i + 1, question, answer));
    }
    
    prompt.push_str("\n**Task**: Create an enhanced version of the automation description that:\n");
    prompt.push_str("1. Incorporates the clarification answers naturally into the step-by-step instructions\n");
    prompt.push_str("2. Makes the automation more specific and precise based on the user's answers\n");
    prompt.push_str("3. CRITICAL: Replace ALL generic placeholders (like [email address], [search term], [website URL]) with the EXACT values provided in the answers\n");
    prompt.push_str("4. Maintains the same clear, numbered step format\n");
    prompt.push_str("5. Ensures the automation would be less likely to need clarification in future runs\n");
    prompt.push_str("6. Be as specific as possible - use exact button names, URLs, email addresses, etc. from the answers\n\n");
    prompt.push_str("Examples of replacements:\n");
    prompt.push_str("- \"Type [recipient email]\" → \"Type john@example.com\"\n");
    prompt.push_str("- \"Navigate to [website URL]\" → \"Navigate to https://example.com\"\n");
    prompt.push_str("- \"Search for [search term]\" → \"Search for quarterly report\"\n");
    prompt.push_str("- \"Click on [headline]\" → \"Click on the first headline in the news section\"\n\n");
    prompt.push_str("Return ONLY the enhanced description in the same numbered step format. Do not include any explanations or commentary.");
    
    let system_prompt = "You are an expert at refining automation instructions. Your task is to enhance automation descriptions by incorporating user clarifications to make them more precise and self-contained.";
    
    info!("Enhancing nl_description with {} clarifications using {}", qa_pairs.len(), provider_name);
    
    // Call the appropriate LLM provider
    let enhanced_description = match api_choice.as_str() {
        "proxy" => {
            let (response_text, _, _) = 
                crate::engine::llm_providers::proxy::call_llm_api(
                    &api_key, prompt, &system_prompt, 1000
                ).await?;
            response_text
        },
        "openai" => {
            let (response_text, _, _) = 
                crate::engine::llm_providers::openai::call_llm_api(
                    &api_key, prompt, system_prompt, 1000
                ).await?;
            response_text
        },
        "grok" => {
            let (response_text, _, _) = 
                crate::engine::llm_providers::grok::call_llm_api(
                    &api_key, prompt, system_prompt, 1000
                ).await?;
            response_text
        },
        "deepseek" => {
            let (response_text, _, _) = 
                crate::engine::llm_providers::deepseek::call_llm_api(
                    &api_key, prompt, system_prompt, 1000
                ).await?;
            response_text
        },
        "claude" | _ => {
            let (response_text, _, _) = 
                crate::engine::llm_providers::claude::call_llm_api(
                    &api_key, prompt, system_prompt, 1000
                ).await?;
            response_text
        }
    };
    
    Ok(enhanced_description.trim().to_string())
}

#[tauri::command]
fn stop_recording(_app_handle: AppHandle) -> Result<(), ()> {
    // Disable recording - this stops data collection
    #[cfg(target_os = "macos")]
    crate::window_details_collector::macos::macos_accessibility_engine::set_recording_enabled(false);
    
    info!("Recording stopped - data collection is now paused");
    
    Ok(())
}

#[tauri::command]
fn get_activity_history(
    app_handle: AppHandle,
    offset: usize,
    limit: usize,
) -> Result<Vec<(i64, String, String)>, String> {
    app_handle
        .db(|db| {
            activity_log_repository::get_activity_history(db, offset, limit)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_activity(app_handle: AppHandle, id: i64) -> Result<bool, String> {
    app_handle
        .db(|db| activity_log_repository::delete_activity(db, id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_activity_full_text_by_id(
    app_handle: tauri::AppHandle,
    id: i64,
) -> Result<Option<(String, String)>, String> {
    app_handle
        .db(|db| activity_log_repository::get_activity_full_text_by_id(db, id, None))
        .map_err(|e| e.to_string())
}

// Add a new command to get activity logs by automation ID
#[tauri::command]
fn get_activity_logs_by_automation(app_handle: AppHandle, automation_id: i64) -> Result<Vec<ActivityItem>, String> {
    app_handle
        .db(|db| activity_log_repository::get_activity_logs_by_automation_id(db, automation_id))
        .map_err(|e| e.to_string())
}

// Add these new commands to the invoke_handler section
#[tauri::command]
fn process_automation_logs(app_handle: AppHandle, automation_id: i64) -> Result<usize, String> {
    app_handle
        .db_mut(|db| automation_log_repository::process_activity_logs_for_automation(db, automation_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_automation_logs(app_handle: AppHandle, automation_id: i64) -> Result<Vec<AutomationLog>, String> {
    app_handle
        .db(|db| automation_log_repository::get_automation_logs(db, automation_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_automation_summary(app_handle: AppHandle, automation_id: i64) -> Result<AutomationSummary, String> {
    app_handle
        .db_mut(|db| automation_log_repository::generate_automation_summary(db, automation_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_llm_script(app_handle: AppHandle, automation_id: i64) -> Result<String, String> {
    app_handle
        .db_mut(|db| automation_log_repository::generate_llm_script(db, automation_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn generate_automation_summary_with_llm(app_handle: AppHandle, automation_id: i64) -> Result<String, String> {
    crate::engine::automation_summary_engine::generate_automation_summary_with_llm(&app_handle, automation_id).await
}

#[tauri::command]
async fn update_all_automations_with_summaries(app_handle: AppHandle) -> Result<usize, String> {
    crate::engine::automation_summary_engine::update_all_automations_with_summaries(&app_handle).await
}

#[tauri::command]
async fn process_automation_events_with_llm(app_handle: AppHandle, automation_id: i64) -> Result<String, String> {
    // Call the processing function and extract just the script part
    let (script, _questions) = crate::engine::automation_summary_engine::process_automation_with_llm(&app_handle, automation_id).await?;
    Ok(script)
}

#[tauri::command]
fn get_automation_by_id(app_handle: AppHandle, automation_id: i64) -> Result<Automation, String> {
    app_handle
        .db(|db| automation_repository::get_automation_by_id(db, automation_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Automation with ID {} not found", automation_id))
}

#[tauri::command]
fn test_keyboard_monitoring() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::window_details_collector::macos::keyboard_monitor;
        
        if keyboard_monitor::is_keyboard_monitoring_enabled() {
            // Test recording a few sample keyboard shortcuts
            keyboard_monitor::record_keyboard_shortcut("Cmd+C (Copy)");
            std::thread::sleep(std::time::Duration::from_millis(100));
            keyboard_monitor::record_keyboard_shortcut("Cmd+V (Paste)");
            std::thread::sleep(std::time::Duration::from_millis(100));
            keyboard_monitor::record_keyboard_shortcut("Escape");
            std::thread::sleep(std::time::Duration::from_millis(100));
            keyboard_monitor::record_keyboard_shortcut("F5");
            
            Ok("✅ Smart keyboard monitoring test completed. Check logs for recorded shortcuts.\n\n🎯 What you should see:\n- Only meaningful shortcuts (Cmd+C, Cmd+V, Escape, F5)\n- NO noise from regular typing\n- Real-time modifier detection\n\n🚫 What you WON'T see:\n- Individual letters like 'a', 'b', 'c'\n- Numbers during typing\n- Regular backspace/space".to_string())
        } else {
            Err("❌ Keyboard monitoring is not enabled. Please start the app first.".to_string())
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        Err("❌ Keyboard monitoring is only available on macOS currently.".to_string())
    }
}

#[tauri::command]
async fn generate_intake_questions(app_handle: AppHandle, objective: String) -> Result<Vec<String>, String> {
    let questions = crate::engine::intake_engine::generate_intake_questions(&app_handle, &objective).await?;
    Ok(questions)
}

#[tauri::command]
async fn save_intake_answers(app_handle: AppHandle, automation_id: Option<i64>, qa_pairs: Vec<(String, String)>) -> Result<(), String> {
    use crate::repository::clarification_repository as repo;
    
    // First save the raw Q&A pairs for audit purposes
    app_handle.db_mut(|db| {
        repo::init_table(db);
        for (question, answer) in &qa_pairs {
            let id = repo::insert_question(db, automation_id, question)?;
            repo::update_answer(db, id, answer)?;
        }
        Ok(())
    }).map_err(|e: rusqlite::Error| e.to_string())?;
    
    // If we have an automation_id, enhance its nl_description with the clarifications
    if let Some(auto_id) = automation_id {
        // Get the current automation
        let automation = app_handle
            .db(|db| automation_repository::get_automation_by_id(db, auto_id))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Automation not found".to_string())?;
        
        // Enhance the nl_description with clarifications
        let enhanced_description = enhance_nl_description_with_clarifications(
            &app_handle,
            &automation.nl_description.as_deref().unwrap_or(""),
            &automation.objective,
            &qa_pairs
        ).await?;
        
        // Update the automation with the enhanced description
        app_handle
            .db_mut(|db| {
                db.execute(
                    "UPDATE automations SET nl_description = ?, updated_at = datetime('now') WHERE id = ?",
                    [&enhanced_description, &auto_id.to_string()]
                )
                .map(|_| ())
                .map_err(|e| format!("Failed to update automation: {}", e))
            })
            .map_err(|e| format!("Database error: {}", e))?;
        
        info!("Enhanced nl_description for automation {} with {} clarifications", auto_id, qa_pairs.len());
    }
    
    Ok(())
}

#[tauri::command]
fn get_automation_clarifications(app_handle: AppHandle, automation_id: i64) -> Result<Vec<crate::repository::clarification_repository::ClarificationEntry>, String> {
    use crate::repository::clarification_repository as repo;
    let entries = app_handle.db(|db| {
        repo::init_table(db);
        repo::get_by_automation(db, automation_id)
    }).map_err(|e: rusqlite::Error| e.to_string())?;
    Ok(entries)
}

#[tauri::command]
async fn authenticate_user(
    app_handle: AppHandle,
    email: String,
    password: String,
) -> Result<serde_json::Value, String> {
    // The 'password' parameter contains the JWT token from the proxy server
    // We'll use it as the auth_token
    
    // Generate a unique user_id based on the email
    let user_id = format!("user_{}", email.replace("@", "_").replace(".", "_"));
    
    // Use the JWT token passed from frontend (in the password field)
    let auth_token = password; // This is actually the JWT token from proxy
    
    // Debug: Log token info (first 20 chars only for security)
    let token_preview = if auth_token.len() > 20 {
        format!("{}...", &auth_token[..20])
    } else {
        auth_token.clone()
    };
    info!("Storing JWT token for user {}: {}", email, token_preview);
    
    // Set expiration to 30 days from now (matching the proxy server's JWT expiry)
    let expires_at = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(30))
        .unwrap()
        .to_rfc3339();
    
    // Store auth in database
    let auth = app_handle
        .db(|db| {
            user_auth_repository::create_or_update_user_auth(
                db,
                crate::entity::user_auth::NewUserAuth {
                    user_id: user_id.clone(),
                    email: Some(email.clone()),
                    auth_token: auth_token.clone(),
                    token_expires_at: expires_at.clone(),
                }
            )
        })
        .map_err(|e| format!("Failed to save auth: {}", e))?;
    
    // Store user_id in settings for other parts of the app
    app_handle
        .db(|db| insert_or_update_setting(db, Setting {
            setting_key: "user_id".to_string(),
            setting_value: user_id.clone(),
        }))
        .map_err(|e| format!("Failed to save user_id: {}", e))?;
    
    Ok(serde_json::json!({
        "userId": auth.user_id,
        "email": auth.email,
        "authToken": auth.auth_token,
        "expiresAt": auth.token_expires_at,
    }))
}

#[tauri::command]
fn logout_user(app_handle: AppHandle) -> Result<(), String> {
    // Get current user
    let user_id = app_handle
        .db(|db| match get_setting(db, "user_id") {
            Ok(setting) => Some(setting.setting_value),
            Err(_) => None,
        })
        .unwrap_or_default();
    
    if !user_id.is_empty() {
        // Clear auth token
        app_handle
            .db(|db| user_auth_repository::clear_user_auth(db, &user_id))
            .map_err(|e| format!("Failed to clear auth: {}", e))?;
        
        // Clear user_id from settings
        app_handle
            .db(|db| insert_or_update_setting(db, Setting {
                setting_key: "user_id".to_string(),
                setting_value: "".to_string(),
            }))
            .map_err(|e| format!("Failed to clear user_id: {}", e))?;
    }
    
    Ok(())
}

#[tauri::command]
fn get_current_user(app_handle: AppHandle) -> Result<Option<serde_json::Value>, String> {
    let user_id = app_handle
        .db(|db| match get_setting(db, "user_id") {
            Ok(setting) => Some(setting.setting_value),
            Err(_) => None,
        })
        .unwrap_or_default();
    
    if user_id.is_empty() {
        return Ok(None);
    }
    
    // Get auth info
    let auth = app_handle
        .db(|db| user_auth_repository::get_user_auth_by_user_id(db, &user_id))
        .map_err(|e| format!("Failed to get auth: {}", e))?;
    
    match auth {
        Some(auth) => Ok(Some(serde_json::json!({
            "userId": auth.user_id,
            "email": auth.email,
            "authToken": auth.auth_token,
            "expiresAt": auth.token_expires_at,
        }))),
        None => Ok(None),
    }
}

#[tauri::command]
fn get_usage_stats(app_handle: AppHandle) -> Result<serde_json::Value, String> {
    let user_id = app_handle
        .db(|db| match get_setting(db, "user_id") {
            Ok(setting) => Some(setting.setting_value),
            Err(_) => None,
        })
        .unwrap_or_else(|| "default_user".to_string());
    
    let stats = app_handle
        .db(|db| usage_session_repository::get_user_usage_stats(db, &user_id))
        .map_err(|e| format!("Failed to get usage stats: {}", e))?;
    
    Ok(serde_json::json!({
        "totalMinutes": stats.total_minutes,
        "sessionsCount": stats.sessions_count,
        "currentMonthMinutes": stats.current_month_minutes,
    }))
}

#[tauri::command]
fn get_usage_history(app_handle: AppHandle, limit: i32) -> Result<Vec<serde_json::Value>, String> {
    let user_id = app_handle
        .db(|db| match get_setting(db, "user_id") {
            Ok(setting) => Some(setting.setting_value),
            Err(_) => None,
        })
        .unwrap_or_else(|| "default_user".to_string());
    
    let sessions = app_handle
        .db(|db| usage_session_repository::get_user_sessions(db, &user_id, limit))
        .map_err(|e| format!("Failed to get usage history: {}", e))?;
    
    let history: Vec<serde_json::Value> = sessions
        .into_iter()
        .map(|s| {
            // Get automation name
            let automation_name = app_handle
                .db(|db| automation_repository::get_automation_by_id(db, s.automation_id))
                .ok()
                .flatten()
                .map(|a| a.name)
                .unwrap_or_else(|| "Unknown".to_string());
            
            serde_json::json!({
                "id": s.id,
                "automationId": s.automation_id,
                "automationName": automation_name,
                "startedAt": s.started_at,
                "endedAt": s.ended_at,
                "totalSeconds": s.total_seconds,
                "billedMinutes": s.billed_minutes,
                "status": s.status,
            })
        })
        .collect();
    
    Ok(history)
}

#[tauri::command]
async fn save_automation(
    app_handle: AppHandle,
    name: String,
    objective: String,
    nl_description: Option<String>,
    steps: Vec<serde_json::Value>,
) -> Result<i64, String> {
    use crate::repository::automation_repository::save_automation as save_automation_to_db;

    info!("📥 Saving imported automation: {}", name);
    info!("  - objective: {}", objective);
    info!("  - nl_description: {:?}", nl_description);
    info!("  - steps count: {}", steps.len());

    // Create the script structure from the steps
    let script = serde_json::json!({
        "steps": steps
    });

    let raw_script_str = serde_json::to_string_pretty(&script)
        .map_err(|e| format!("Failed to serialize script: {}", e))?;

    // For imported automations, we use the same script for both raw and generalized
    let generalized_script_str = raw_script_str.clone();

    // Save the automation to the database
    let automation_id = app_handle
        .db(|db| -> Result<i64, rusqlite::Error> {
            let id = save_automation_to_db(
                db,
                &name,
                &objective,
                &raw_script_str,
                &generalized_script_str,
            )?;

            // If we have an nl_description, update it
            if let Some(nl_desc) = &nl_description {
                db.execute(
                    "UPDATE automations SET nl_description = ? WHERE id = ?",
                    rusqlite::params![nl_desc, id]
                )?;
            }

            Ok(id)
        })
        .map_err(|e| format!("Failed to save automation: {}", e))?;

    info!("✅ Successfully saved imported automation with ID: {}", automation_id);

    Ok(automation_id)
}

#[tauri::command]
async fn generate_synthetic_automation(
    app_handle: AppHandle,
    prompt: String,
    name: String,
    api_key: Option<String>,
    api_choice: Option<String>
) -> Result<crate::engine::synthetic_automation::SyntheticGenerationResponse, String> {
    use crate::engine::synthetic_automation::{SyntheticGenerationRequest, generate_synthetic_automation as gen_synthetic};
    
    info!("🔍 Received generate_synthetic_automation command:");
    info!("  - prompt: {}", prompt);
    info!("  - name: {}", name);
    info!("  - api_key: {} (length: {})", 
        if api_key.is_some() { "provided" } else { "not provided" }, 
        api_key.as_ref().map(|k| k.len()).unwrap_or(0)
    );
    info!("  - api_choice: {:?}", api_choice);
    
    let request = SyntheticGenerationRequest {
        prompt,
        name,
        api_key,
        api_choice,
    };
    
    gen_synthetic(&app_handle, request).await
}

#[tauri::command]
async fn create_agent_mode_automation(
    app_handle: AppHandle,
    objective: String,
    name: String,
) -> Result<crate::engine::synthetic_automation::SyntheticGenerationResponse, String> {
    use crate::engine::synthetic_automation::create_agent_mode_automation as create_agent_auto;

    info!("Received create_agent_mode_automation command:");
    info!("  - objective: {}", objective);
    info!("  - name: {}", name);

    create_agent_auto(&app_handle, objective, name).await
}

#[tauri::command]
fn get_detected_cli_tools() -> Vec<String> {
    crate::engine::cli_probe::get_cached_tools().unwrap_or_default()
}

#[tauri::command]
async fn refresh_cli_probe() -> Vec<String> {
    crate::engine::cli_probe::refresh().await;
    crate::engine::cli_probe::get_cached_tools().unwrap_or_default()
}

fn check_single_instance() -> Result<PathBuf, String> {
    let temp_dir = std::env::temp_dir();
    let lock_file = temp_dir.join("heelix_notes.lock");
    
    // Check if lock file exists and contains a valid PID
    if lock_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&lock_file) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                    // On Unix-like systems, check if process exists
                    if std::process::Command::new("ps")
                        .arg("-p")
                        .arg(pid.to_string())
                        .output()
                        .map(|output| output.status.success())
                        .unwrap_or(false)
                    {
                        return Err("Another instance is already running".to_string());
                }
            }
        }
        // If we can't read the file or PID is invalid, remove the stale lock file
        let _ = remove_file(&lock_file);
    }
    
    // Create new lock file with current PID
    let current_pid = std::process::id();
    std::fs::write(&lock_file, current_pid.to_string())
        .map_err(|e| format!("Failed to create lock file: {}", e))?;
    
    Ok(lock_file)
}

fn cleanup_lock_file() {
    if let Ok(mut path) = LOCK_FILE_PATH.lock() {
        if let Some(lock_path) = path.take() {
            let _ = remove_file(lock_path);
        }
    }
}
