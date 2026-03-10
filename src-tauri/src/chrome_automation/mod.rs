// App automation module for macOS
use std::process::Command;
use log::{info, error};
use serde::{Serialize, Deserialize};

// Import the macOS accessibility functionality
#[cfg(target_os = "macos")]
use crate::window_details_collector::macos::macos_accessibility_engine;
#[cfg(target_os = "macos")]
use crate::window_details_collector::macos::macos_action_detector_engine;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppResult {
    pub success: bool,
    pub message: String,
}

/// Launch any application
#[tauri::command]
pub fn launch_app(app_name: &str) -> AppResult {
    info!("Attempting to launch application: {}", app_name);
    
    #[cfg(target_os = "windows")]
    {
        // Map common app names to Windows executables
        let windows_app = match app_name.to_lowercase().as_str() {
            // Web Browsers
            "google chrome" | "chrome" => "chrome.exe",
            "microsoft edge" | "edge" | "ms edge" => "msedge.exe",
            "firefox" | "mozilla firefox" => "firefox.exe",
            "opera" | "opera browser" => "opera.exe",
            "brave" | "brave browser" => "brave.exe",
            "vivaldi" | "vivaldi browser" => "vivaldi.exe",
            "tor browser" | "tor" => "firefox.exe",  // Tor uses modified Firefox

            // Microsoft Office Suite
            "microsoft word" | "word" | "ms word" => "winword.exe",
            "microsoft excel" | "excel" | "ms excel" => "excel.exe",
            "microsoft powerpoint" | "powerpoint" | "ms powerpoint" => "powerpnt.exe",
            "microsoft outlook" | "outlook" | "ms outlook" => "outlook.exe",
            "microsoft onenote" | "onenote" => "onenote.exe",
            "microsoft access" | "access" | "ms access" => "msaccess.exe",
            "microsoft publisher" | "publisher" | "ms publisher" => "mspub.exe",
            "microsoft visio" | "visio" | "ms visio" => "visio.exe",
            "microsoft project" | "project" | "ms project" => "winproj.exe",

            // Communication & Collaboration
            "microsoft teams" | "teams" | "ms teams" => "teams.exe",
            "slack" => "slack.exe",
            "discord" => "discord.exe",
            "skype" | "skype for business" => "skype.exe",
            "zoom" | "zoom meetings" => "zoom.exe",
            "telegram" | "telegram desktop" => "telegram.exe",
            "whatsapp" | "whatsapp desktop" => "whatsapp.exe",
            "signal" | "signal desktop" => "signal.exe",

            // Development Tools
            // VSCode/Cursor: These are script-based launchers (code.cmd, cursor.cmd)
            // They exit quickly after spawning the actual Electron app
            "visual studio code" | "vscode" | "vs code" | "code" => "code",
            "cursor" | "cursor editor" => "cursor",
            "visual studio" | "vs" => "devenv.exe",
            "notepad++" | "notepad plus plus" => "notepad++.exe",
            "sublime text" | "sublime" => "sublime_text.exe",
            "atom" => "atom.exe",
            "intellij idea" | "intellij" => "idea64.exe",
            "pycharm" => "pycharm64.exe",
            "webstorm" => "webstorm64.exe",
            "eclipse" => "eclipse.exe",
            "android studio" => "studio64.exe",
            "git bash" | "git" => "git-bash.exe",

            // Adobe Creative Suite
            "adobe photoshop" | "photoshop" => "photoshop.exe",
            "adobe illustrator" | "illustrator" => "illustrator.exe",
            "adobe premiere pro" | "premiere" => "adobe premiere pro.exe",
            "adobe after effects" | "after effects" => "afterfx.exe",
            "adobe acrobat" | "acrobat" | "adobe reader" => "acrobat.exe",
            "adobe indesign" | "indesign" => "indesign.exe",
            "adobe lightroom" | "lightroom" => "lightroom.exe",
            "adobe xd" => "xd.exe",

            // Media Players & Editors
            "vlc" | "vlc media player" => "vlc.exe",
            "windows media player" | "media player" => "wmplayer.exe",
            "spotify" => "spotify.exe",
            "itunes" | "apple music" => "itunes.exe",
            "audacity" => "audacity.exe",
            "obs studio" | "obs" => "obs64.exe",
            "davinci resolve" | "resolve" => "resolve.exe",

            // Productivity & Utilities
            "notion" => "notion.exe",
            "obsidian" => "obsidian.exe",
            "evernote" => "evernote.exe",
            "todoist" => "todoist.exe",
            "7-zip" | "7zip" => "7zfm.exe",
            "winrar" => "winrar.exe",
            "winzip" => "winzip32.exe",

            // Gaming & Entertainment
            "steam" => "steam.exe",
            "epic games" | "epic games launcher" => "epicgameslauncher.exe",
            "origin" | "ea app" => "origin.exe",
            "battle.net" | "blizzard" => "battle.net.exe",
            "minecraft" | "minecraft launcher" => "minecraftlauncher.exe",

            // System & Built-in Windows
            "notepad" => "notepad.exe",
            "calculator" | "calc" => "calc.exe",
            "paint" | "mspaint" => "mspaint.exe",
            "snipping tool" | "snip & sketch" => "snippingtool.exe",
            "explorer" | "file explorer" | "windows explorer" => "explorer.exe",
            "command prompt" | "cmd" => "cmd.exe",
            "powershell" | "windows powershell" => "powershell.exe",
            "windows terminal" | "terminal" => "wt.exe",
            "task manager" | "taskmgr" => "taskmgr.exe",
            "control panel" => "control.exe",
            "settings" | "windows settings" => "ms-settings:",  // Special URI
            "registry editor" | "regedit" => "regedit.exe",

            // Default: return as-is if no mapping found
            _ => app_name,
        };
        
        // Check if this is VS Code or Cursor - these support reusing existing windows
        let is_vscode = windows_app == "code.cmd";
        let is_cursor = windows_app == "cursor.cmd";
        
        // For VS Code and Cursor, check if already running and focus existing window
        if is_vscode || is_cursor {
            let process_name = if is_vscode { "Code.exe" } else { "Cursor.exe" };
            
            // Check if the process is already running using tasklist
            let check_result = Command::new("tasklist")
                .args(&["/FI", &format!("IMAGENAME eq {}", process_name), "/NH"])
                .output();
            
            if let Ok(output) = check_result {
                let output_str = String::from_utf8_lossy(&output.stdout);
                
                if output_str.contains(process_name) {
                    // Process is already running - focus the existing window using PowerShell
                    info!("{} is already running, focusing existing window", process_name);
                    
                    // Use PowerShell to find and activate the window
                    // This brings the existing VS Code/Cursor window to the foreground
                    let ps_script = format!(
                        r#"Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class Win32 {{ [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd); [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow); }}'; $proc = Get-Process -Name '{}' -ErrorAction SilentlyContinue | Select-Object -First 1; if ($proc) {{ [Win32]::ShowWindow($proc.MainWindowHandle, 9); [Win32]::SetForegroundWindow($proc.MainWindowHandle) }}"#,
                        if is_vscode { "Code" } else { "Cursor" }
                    );
                    
                    let focus_result = Command::new("powershell")
                        .args(&["-NoProfile", "-Command", &ps_script])
                        .spawn();
                    
                    match focus_result {
                        Ok(_) => {
                            info!("Successfully focused existing {} window", app_name);
                            return AppResult {
                                success: true,
                                message: format!("Focused existing {} window", app_name),
                            };
                        }
                        Err(e) => {
                            // If focus fails, fall through to launch new instance
                            info!("Failed to focus existing window, will launch new instance: {}", e);
                        }
                    }
                }
            }
            
            // Not running or focus failed - launch new instance
            info!("{} not running, launching new instance", app_name);
        }
        
        // Check if this is a script-based launcher (.cmd, .bat)
        // These launchers exit quickly after spawning the actual application
        let is_script_launcher = windows_app.ends_with(".cmd") || windows_app.ends_with(".bat");
        
        info!("Launching Windows app: {} (script_launcher={})", windows_app, is_script_launcher);
        
        // For script-based launchers, use 'start' command which handles .cmd files properly
        // and returns immediately (fire-and-forget)
        if is_script_launcher {
            // Use 'start' command with /B flag to run without creating a new window for the cmd process
            // The script will spawn the actual app and exit quickly
            let result = Command::new("cmd")
                .args(&["/C", "start", "", windows_app])
                .spawn();
            
            match result {
                Ok(_) => {
                    // Don't wait for the launcher script - it will spawn the actual app and exit
                    info!("Successfully started script launcher {} (fire-and-forget)", windows_app);
                    return AppResult {
                        success: true,
                        message: format!("Successfully launched {} (script launcher)", app_name),
                    };
                }
                Err(e) => {
                    error!("Failed to start script launcher {}: {}", windows_app, e);
                    return AppResult {
                        success: false,
                        message: format!("Failed to launch {}: {}", app_name, e),
                    };
                }
            }
        }
        
        // For regular executables, try direct launch first
        let result = Command::new(windows_app).spawn();
        
        match result {
            Ok(mut child) => {
                // Wait a moment to see if the process starts successfully
                std::thread::sleep(std::time::Duration::from_millis(300));
                
                // Check if process is still running
                match child.try_wait() {
                    Ok(Some(status)) => {
                        // Process exited - check if it succeeded or failed
                        if !status.success() {
                            // Try fallback with start command (non-blocking spawn)
                            let fallback_result = Command::new("cmd")
                                .args(&["/C", "start", "", windows_app])
                                .spawn();
                            
                            match fallback_result {
                                Ok(_) => {
                                    info!("Successfully launched {} via start command (fallback)", app_name);
                                    return AppResult {
                                        success: true,
                                        message: format!("Successfully launched {}", app_name),
                                    };
                                }
                                Err(e) => {
                                    error!("Failed to launch {}: {}", app_name, e);
                                    return AppResult {
                                        success: false,
                                        message: format!("Failed to launch {}: {}", app_name, e),
                                    };
                                }
                            }
                        } else {
                            // Process started and exited successfully (might be a launcher that spawns another process)
                            info!("Successfully launched {} (process completed quickly)", app_name);
                            return AppResult {
                                success: true,
                                message: format!("Successfully launched {}", app_name),
                            };
                        }
                    }
                    Ok(None) => {
                        // Process is still running - success
                        info!("Successfully spawned {} (process running)", windows_app);
                        return AppResult {
                            success: true,
                            message: format!("Successfully launched {}", app_name),
                        };
                    }
                    Err(e) => {
                        error!("Error checking process status: {}", e);
                        return AppResult {
                            success: false,
                            message: format!("Error launching {}: {}", app_name, e),
                        };
                    }
                }
            }
            Err(_) => {
                // Direct launch failed, try using start command (non-blocking spawn)
                let fallback_result = Command::new("cmd")
                    .args(&["/C", "start", "", windows_app])
                    .spawn();
                    
                match fallback_result {
                    Ok(_) => {
                        info!("Successfully launched {} via start command", app_name);
                        return AppResult {
                            success: true,
                            message: format!("Successfully launched {}", app_name),
                        };
                    }
                    Err(e) => {
                        error!("Failed to launch {}: {}", app_name, e);
                        return AppResult {
                            success: false,
                            message: format!("Failed to launch {}: {}", app_name, e),
                        };
                    }
                }
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // Map common app names/aliases to macOS application names
        let macos_app = match app_name.to_lowercase().as_str() {
            // Development Tools - VSCode shows as "Code" in window title
            "code" | "vscode" | "vs code" => "Visual Studio Code",
            "cursor" | "cursor editor" => "Cursor",
            
            // Browsers
            "chrome" => "Google Chrome",
            "firefox" => "Firefox",
            "edge" | "microsoft edge" => "Microsoft Edge",
            "safari" => "Safari",
            "brave" => "Brave Browser",
            "arc" => "Arc",
            
            // Communication
            "slack" => "Slack",
            "discord" => "Discord",
            "zoom" => "zoom.us",
            "teams" | "microsoft teams" => "Microsoft Teams",
            "messages" | "imessage" => "Messages",
            "facetime" => "FaceTime",
            
            // Productivity
            "notes" | "apple notes" => "Notes",
            "reminders" => "Reminders",
            "calendar" => "Calendar",
            "mail" => "Mail",
            "notion" => "Notion",
            "obsidian" => "Obsidian",
            
            // Media
            "spotify" => "Spotify",
            "music" | "apple music" => "Music",
            "vlc" => "VLC",
            "quicktime" => "QuickTime Player",
            
            // System
            "finder" => "Finder",
            "terminal" => "Terminal",
            "iterm" | "iterm2" => "iTerm",
            "activity monitor" => "Activity Monitor",
            "system preferences" | "system settings" => "System Settings",
            
            // Default: use as-is
            _ => app_name,
        };
        
        let output = Command::new("open")
            .arg("-a")
            .arg(macos_app)
            .output();
        
        match output {
            Ok(output) => {
                if output.status.success() {
                    info!("Successfully launched {} (as '{}')", app_name, macos_app);
                    AppResult {
                        success: true,
                        message: format!("Successfully launched {}", app_name),
                    }
                } else {
                    let error_message = String::from_utf8_lossy(&output.stderr).to_string();
                    error!("Failed to launch {} (tried '{}'): {}", app_name, macos_app, error_message);
                    AppResult {
                        success: false,
                        message: format!("Failed to launch {}: {}", app_name, error_message),
                    }
                }
            },
            Err(e) => {
                error!("Error launching {}: {}", app_name, e);
                AppResult {
                    success: false,
                    message: format!("Error launching {}: {}", app_name, e),
                }
            }
        }
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let output = Command::new("xdg-open")
            .arg(app_name)
            .output();
        
        match output {
            Ok(output) => {
                if output.status.success() {
                    info!("Successfully launched {}", app_name);
                    AppResult {
                        success: true,
                        message: format!("Successfully launched {}", app_name),
                    }
                } else {
                    let error_message = String::from_utf8_lossy(&output.stderr).to_string();
                    error!("Failed to launch {}: {}", app_name, error_message);
                    AppResult {
                        success: false,
                        message: format!("Failed to launch {}: {}", app_name, error_message),
                    }
                }
            },
            Err(e) => {
                error!("Error launching {}: {}", app_name, e);
                AppResult {
                    success: false,
                    message: format!("Error launching {}: {}", app_name, e),
                }
            }
        }
    }
}

/// Execute AppleScript to control applications (legacy implementation)
#[cfg(target_os = "macos")]
fn execute_applescript(script: &str) -> Result<String, String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to execute AppleScript: {}", e))?;
    
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Get a list of UI elements from a running application by its PID
#[tauri::command]
#[allow(dead_code)]
pub fn get_app_ui_elements_by_pid(pid: &str) -> AppResult {
    info!("Getting UI elements from application with PID: {}", pid);
    
    #[cfg(target_os = "macos")]
    {
        let element_tree = macos_accessibility_engine::by_pid(pid);
        if !element_tree.is_empty() {
            return AppResult {
                success: true,
                message: element_tree,
            };
        } else {
            return AppResult {
                success: false,
                message: "Failed to get UI elements from application".to_string(),
            };
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        return AppResult {
            success: false,
            message: "This feature is only available on macOS".to_string(),
        };
    }
}

/// Get a list of all actionable UI elements in any application
#[tauri::command]
pub fn get_app_actionable_elements(app_name: &str) -> AppResult {
    info!("Attempting to get actionable UI elements from {}", app_name);
    
    // First, launch the application
    let launch_result = launch_app(app_name);
    if !launch_result.success {
        return launch_result; // Return early if app couldn't be launched
    }
    
    // Add a delay to ensure the app has time to launch fully
    // Use shorter delay for browsers
    let is_browser = app_name.contains("Chrome") || app_name.contains("Safari") || 
                     app_name.contains("Firefox") || app_name.contains("Edge");
    let launch_delay = if is_browser { 750 } else { 1500 };
    std::thread::sleep(std::time::Duration::from_millis(launch_delay));
    
    #[cfg(target_os = "macos")]
    {
        // Use AppleScript to get the PID of the app
        let pid_script = format!(r#"
            tell application "System Events"
                set appProcess to the first process whose name is "{}"
                return (unix id of appProcess) as text
            end tell
        "#, app_name);
        
        match execute_applescript(&pid_script) {
            Ok(pid) => {
                // First, start observing this application to ensure all elements are accessible
                macos_accessibility_engine::observe_by_pid(&pid);
                
                // Use the new specialized action detector
                let (actionable_elements, _, _) = macos_action_detector_engine::get_actionable_elements_by_pid(&pid);
                
                if !actionable_elements.is_empty() {
                    // Format elements into a structured, readable form
                    let formatted_result = macos_action_detector_engine::format_actionable_elements(&actionable_elements);
                    
                    return AppResult {
                        success: true,
                        message: formatted_result,
                    };
                } else {
                    return AppResult {
                        success: false,
                        message: "Failed to get actionable elements - no UI elements found".to_string(),
                    };
                }
            },
            Err(e) => {
                error!("Failed to get PID for {}: {}", app_name, e);
                return AppResult {
                    success: false,
                    message: format!("Failed to get PID for {}: {}", app_name, e),
                };
            }
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        return AppResult {
            success: false,
            message: "This feature is only available on macOS".to_string(),
        };
    }
}

/// Perform a random action on an element in the app
#[tauri::command]
pub fn perform_random_app_action(app_name: &str) -> AppResult {
    info!("Attempting to perform a random action in {}", app_name);
    
    // First, launch the application
    let launch_result = launch_app(app_name);
    if !launch_result.success {
        return launch_result; // Return early if app couldn't be launched
    }
    
    // Add a delay to ensure the app has time to launch fully
    // Use shorter delay for browsers
    let is_browser = app_name.contains("Chrome") || app_name.contains("Safari") || 
                     app_name.contains("Firefox") || app_name.contains("Edge");
    let launch_delay = if is_browser { 750 } else { 1500 };
    std::thread::sleep(std::time::Duration::from_millis(launch_delay));
    
    #[cfg(target_os = "macos")]
    {
        // Use AppleScript to get the PID of the app
        let pid_script = format!(r#"
            tell application "System Events"
                set appProcess to the first process whose name is "{}"
                return (unix id of appProcess) as text
            end tell
        "#, app_name);
        
        match execute_applescript(&pid_script) {
            Ok(pid) => {
                // Start observing this application to ensure all elements are accessible
                macos_accessibility_engine::observe_by_pid(&pid);
                
                // Get actionable elements for the application
                let (actionable_elements, _, _) = macos_action_detector_engine::get_actionable_elements_by_pid(&pid);
                let can_find_elements = !actionable_elements.is_empty();
                
                // Try to perform a random action
                let action_result = macos_action_detector_engine::pick_random_element_and_act(&pid);
                
                if !action_result.success && can_find_elements {
                    // If the action failed but we found elements in the list, provide a more helpful message
                    info!("Found elements with get_actionable_elements but couldn't act on them");
                    
                    return AppResult {
                        success: false,
                        message: format!("{}\n\nNote: Found {} actionable elements with 'Get Actionable Elements' but couldn't perform actions on them. This could be because elements are visible in the accessibility tree but not actually interactive, which can happen due to application security restrictions.", 
                            action_result.message, 
                            actionable_elements.len())
                    };
                }
                
                return AppResult {
                    success: action_result.success,
                    message: action_result.message,
                };
            },
            Err(e) => {
                error!("Failed to get PID for {}: {}", app_name, e);
                AppResult {
                    success: false,
                    message: format!("Failed to get PID for {}: {}", app_name, e),
                }
            }
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        AppResult {
            success: false,
            message: "This feature is only available on macOS".to_string(),
        }
    }
} 
