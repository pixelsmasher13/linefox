#![cfg(any(target_os = "macos"))]

use log::{error, info, warn};
use std::time::Duration;
use tokio::process::Command;
use accessibility::{AXUIElement, AXAttribute};

/// Result of performing an action
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub success: bool,
    pub message: String,
}

impl ActionResult {
    /// Create a successful result
    pub fn success(message: &str) -> Self {
        ActionResult {
            success: true,
            message: message.to_string(),
        }
    }
    
    /// Create a failure result
    pub fn failure(message: &str) -> Self {
        ActionResult {
            success: false,
            message: message.to_string(),
        }
    }
}

/// Launch an application by name and wait until it's ready
pub async fn launch_app_and_wait(app_name: &str) -> ActionResult {
    info!("Launching application: {}", app_name);
    
    // Launch the app using the existing launch_app function
    #[cfg(target_os = "macos")]
    let result = crate::chrome_automation::launch_app(app_name);
    if !result.success {
        return ActionResult {
            success: false,
            message: format!("Failed to launch app: {}", result.message),
        };
    }
    
    info!("App launch initiated, waiting for app to be ready...");
    
    // Get the PID of the app
    let pid = match get_app_pid(app_name).await {
        Ok(pid) => pid,
        Err(e) => {
            warn!("Failed to get PID after app launch: {}", e);
            // Wait a moment and try again
            tokio::time::sleep(Duration::from_secs(1)).await;
            
            match get_app_pid(app_name).await {
                Ok(pid) => pid,
                Err(e) => return ActionResult {
                    success: false, 
                    message: format!("Failed to get PID after retry: {}", e)
                }
            }
        }
    };
    
    // Start observing the app
    crate::window_details_collector::macos::macos_accessibility_engine::observe_by_pid(&pid);
    
    // Wait for elements to become available, with timeout
    let mut attempts = 0;
    let max_attempts = 2;
    let mut has_elements = false;
    
    while attempts < max_attempts {
        // Check if we can get elements from the app
        let (elements, _, _) = crate::window_details_collector::macos::macos_action_detector_engine::get_actionable_elements_by_pid(&pid);
        
        if !elements.is_empty() {
            info!("App ready with {} accessible elements", elements.len());
            has_elements = true;
            break;
        }
        
        // Wait before next check
        info!("App not ready yet (attempt {}/{}), waiting...", attempts + 1, max_attempts);
        tokio::time::sleep(Duration::from_millis(100)).await;
        attempts += 1;
    }
    
    if !has_elements {
        warn!("App launched but no elements found after {} attempts. Proceeding anyway.", max_attempts);
    }
    
    ActionResult {
        success: true,
        message: format!("Application {} launched successfully", app_name),
    }
}

/// Find and click an element in an application
pub async fn find_and_click_element(app_name: &str, element_path: &str) -> ActionResult {
    info!("Clicking element with path '{}' in '{}'", element_path, app_name);
    
    // Get the PID of the app
    let pid = match get_app_pid(app_name).await {
        Ok(pid) => pid,
        Err(e) => return ActionResult {
            success: false,
            message: format!("Failed to get PID: {}", e),
        },
    };
    
    // Ensure accessibility observation is turned on for this app
    // Observer setup now handled efficiently with PID tracking in observe_by_pid
    
    // Give a moment for the accessibility engine to collect information
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Use click_element_by_id with the element path
    let click_result = crate::window_details_collector::macos::macos_action_detector_engine::click_element_by_id(
        &pid, 
        element_path
    );
    
    if click_result.success {
        info!("Successfully clicked element");
        return ActionResult {
            success: true,
            message: "Successfully clicked element".to_string(),
        };
    }
    
    // If we get here, we couldn't find or interact with the element
    ActionResult {
        success: false,
        message: format!("Could not find or interact with element '{}'", element_path),
    }
}

/// Type text in an application
pub async fn type_text_in_app(app_name: &str, text: &str, element_path: Option<&str>) -> ActionResult {
    info!("Typing text in app {}: {} (element: {:?})", app_name, text, element_path);

    // ONLY use Word AppleScript for the synthetic word_document_content element
    // For other elements in Word (like text fields in dialogs), use the standard accessibility API
    let is_word_document_element = element_path.map_or(false, |path| path.contains("word_document_content"));

    info!("🔍 WORD DETECTION: app_name='{}', is_word_document_element={}",
          app_name, is_word_document_element);

    if is_word_document_element {
        info!("✅ WORD DOCUMENT ELEMENT DETECTED: Using AppleScript insertion method for document content");
        return insert_text_in_word(text).await;
    } else {
        info!("❌ NOT WORD DOCUMENT ELEMENT: Using standard accessibility API method (element_path: {:?})", element_path);
    }

    // Get the PID of the app
    let pid = match get_app_pid(app_name).await {
        Ok(pid) => pid,
        Err(e) => return ActionResult {
            success: false,
            message: format!("Failed to get PID: {}", e),
        },
    };

    // Ensure the app is being observed
    // Observer setup now handled efficiently with PID tracking in observe_by_pid

    // Wait a bit for the app to be ready and focused
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Use the direct type-in-element function when we have an element path
    if let Some(path) = element_path {
        // Try the new direct approach to type into element by path
        let result = crate::window_details_collector::macos::macos_action_detector_engine::type_text_in_element_by_id(
            &pid, 
            path,
            text
        );
        
        // If successful, return right away, converting between ActionResult types
        if result.success {
            return ActionResult {
                success: result.success,
                message: result.message,
            };
        }
        
        // If direct typing failed, fall back to click-then-type approach
        info!("Direct typing failed: {}, falling back to click-then-type approach", result.message);
        
        // Try to click/focus the element first
        let click_result = crate::window_details_collector::macos::macos_action_detector_engine::click_element_by_id(
            &pid, 
            path
        );
        
        if !click_result.success {
            return ActionResult {
                success: false,
                message: format!("Failed to focus element before typing: {}", click_result.message),
            };
        }
        
        // Give a moment for the element to be focused
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    // Approach for focused element: Try to use the accessibility API to set text in focused element
    let result = crate::window_details_collector::macos::macos_action_detector_engine::type_text_in_focused_element(&pid, text);
    
    if result.success {
        info!("Successfully typed text using accessibility API: {}", text);
        return ActionResult {
            success: true,
            message: format!("Successfully typed '{}' using accessibility API", text),
        };
    }
    
    // Second approach: If accessibility API fails, use AppleScript as fallback
    info!("Accessibility API approach failed: {}, falling back to AppleScript", result.message);
    
    // Use AppleScript as fallback
    let script = format!(r#"
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            if name of frontApp is "{}" then
                keystroke "{}"
            end if
        end tell
    "#, app_name, text.replace(r#"""#, r#"\""#));
    
    // Execute the AppleScript
    match execute_applescript(&script).await {
        Ok(_) => {
            info!("Successfully typed text using AppleScript fallback: {}", text);
            ActionResult {
                success: true,
                message: format!("Successfully typed '{}' using keyboard input", text),
            }
        },
        Err(e) => {
            ActionResult {
                success: false,
                message: format!("Failed to type text: {}", e),
            }
        }
    }
}

/// Press a key in an application
pub async fn press_key_in_app(app_name: &str, key: &str) -> ActionResult {
    info!("Pressing key in app {}: {}", app_name, key);
    
    // Get the PID of the app
    let pid = match get_app_pid(app_name).await {
        Ok(pid) => pid,
        Err(e) => return ActionResult {
            success: false,
            message: format!("Failed to get PID: {}", e),
        },
    };
    
    // Ensure the app is being observed
    crate::window_details_collector::macos::macos_accessibility_engine::observe_by_pid(&pid);
    
    // Wait a bit for the app to be ready and focused
    tokio::time::sleep(Duration::from_millis(300)).await;
    
    // Check if this is a keyboard shortcut with modifiers
    let script = if key.contains("+") {
        // Handle keyboard shortcuts like cmd+a, ctrl+c, cmd+shift+p, etc.
        let parts: Vec<&str> = key.split('+').collect();
        
        // The last part is the key, everything before is modifiers
        let key_char = parts.last().unwrap().to_lowercase();
        let modifiers: Vec<&str> = parts[..parts.len()-1].iter().map(|s| *s).collect();
        
        // Map each modifier to AppleScript syntax
        let mut modifier_names: Vec<&str> = Vec::new();
        for modifier in &modifiers {
            let modifier_name = match modifier.to_lowercase().as_str() {
                "cmd" | "command" => "command down",
                "ctrl" | "control" => "control down",
                "opt" | "option" | "alt" => "option down",
                "shift" => "shift down",
                _ => return ActionResult {
                    success: false,
                    message: format!("Unknown modifier: {}", modifier),
                },
            };
            modifier_names.push(modifier_name);
        }
        
        // Build the modifier clause
        // For single modifier: "using command down"
        // For multiple modifiers: "using {command down, shift down}"
        let modifier_clause = if modifier_names.len() == 1 {
            format!("using {}", modifier_names[0])
        } else {
            format!("using {{{}}}", modifier_names.join(", "))
        };
        
        // Check if the key is a special key that needs key code
        let key_script = match key_char.as_str() {
            "f1" => "key code 122".to_string(),
            "f2" => "key code 120".to_string(),
            "f3" => "key code 99".to_string(),
            "f4" => "key code 118".to_string(),
            "f5" => "key code 96".to_string(),
            "f6" => "key code 97".to_string(),
            "f7" => "key code 98".to_string(),
            "f8" => "key code 100".to_string(),
            "f9" => "key code 101".to_string(),
            "f10" => "key code 109".to_string(),
            "f11" => "key code 103".to_string(),
            "f12" => "key code 111".to_string(),
            "enter" | "return" => "key code 36".to_string(),
            "tab" => "key code 48".to_string(),
            "escape" | "esc" => "key code 53".to_string(),
            "space" => "key code 49".to_string(),
            "backspace" | "delete" => "key code 51".to_string(),
            "up" | "uparrow" => "key code 126".to_string(),
            "down" | "downarrow" => "key code 125".to_string(),
            "left" | "leftarrow" => "key code 123".to_string(),
            "right" | "rightarrow" => "key code 124".to_string(),
            "[" => "key code 33".to_string(),
            "]" => "key code 30".to_string(),
            "/" => "key code 44".to_string(),
            "\\" => "key code 42".to_string(),
            // For regular character keys, use keystroke
            _ => format!("keystroke \"{}\"", key_char),
        };
        
        // Build the final script - send keystroke to the frontmost app directly
        // We don't check the app name because:
        // 1. We've already ensured the app is frontmost via get_app_pid
        // 2. Process names can vary (e.g., "Code" vs "Electron" for VS Code)
        // 3. The conditional would silently fail if names don't match exactly
        format!(r#"
            tell application "System Events"
                {} {}
            end tell
        "#, key_script, modifier_clause)
    } else {
        // Single key press
        // Map key name to AppleScript key code
        let key_code = match key.to_lowercase().as_str() {
            "enter" | "return" => "return",
            "tab" => "tab",
            "escape" | "esc" => "escape",
            "space" => "space",
            "backspace" | "delete" => "delete",
            "up" | "uparrow" => "up arrow",
            "down" | "downarrow" => "down arrow",
            "left" | "leftarrow" => "left arrow",
            "right" | "rightarrow" => "right arrow",
            _ => key, // Use as-is for other keys
        };
        
        // For special keys like Enter and Escape, use key code for reliability
        // For regular characters, use keystroke
        if matches!(key_code, "return" | "escape" | "tab" | "space" | "delete" | "up arrow" | "down arrow" | "left arrow" | "right arrow") {
            // Use key code for special keys - more reliable across apps
            let key_code_num = match key_code {
                "return" => 36,
                "escape" => 53,
                "tab" => 48,
                "space" => 49,
                "delete" => 51,
                "up arrow" => 126,
                "down arrow" => 125,
                "left arrow" => 123,
                "right arrow" => 124,
                _ => 36, // fallback to return
            };
            format!(r#"
                tell application "System Events"
                    key code {}
                end tell
            "#, key_code_num)
        } else {
            format!(r#"
                tell application "System Events"
                    keystroke "{}"
                end tell
            "#, key_code)
        }
    };
    
    // Execute the AppleScript
    match execute_applescript(&script).await {
        Ok(_) => {
            info!("Successfully pressed key: {}", key);
            ActionResult {
                success: true,
                message: format!("Successfully pressed key: {}", key),
            }
        },
        Err(e) => {
            ActionResult {
                success: false,
                message: format!("Failed to press key: {}", e),
            }
        }
    }
}

/// Get the PID of a running application
/// Map common app name aliases to their actual macOS process names
/// This is needed because System Events uses the actual process name
fn get_macos_process_name(app_name: &str) -> &str {
    match app_name.to_lowercase().as_str() {
        // Development Tools - VSCode shows as "Code" in UI but process is "Code"
        // However, System Events knows it as "Visual Studio Code" or just "Code" depending on context
        "code" | "vscode" | "vs code" | "visual studio code" => "Code",
        "cursor" | "cursor editor" => "Cursor",
        
        // Browsers
        "chrome" | "google chrome" => "Google Chrome",
        "firefox" | "mozilla firefox" => "Firefox",
        "edge" | "microsoft edge" => "Microsoft Edge",
        "safari" => "Safari",
        "brave" | "brave browser" => "Brave Browser",
        "arc" => "Arc",
        
        // Communication
        "slack" => "Slack",
        "discord" => "Discord",
        "zoom" | "zoom.us" => "zoom.us",
        "teams" | "microsoft teams" => "Microsoft Teams",
        "messages" | "imessage" => "Messages",
        
        // Productivity
        "notes" | "apple notes" => "Notes",
        "notion" => "Notion",
        "obsidian" => "Obsidian",
        
        // System
        "finder" => "Finder",
        "terminal" => "Terminal",
        "iterm" | "iterm2" => "iTerm2",
        
        // Default: use as-is
        _ => app_name,
    }
}

pub async fn get_app_pid(app_name: &str) -> Result<String, String> {
    let process_name = get_macos_process_name(app_name);
    info!("Looking for PID of '{}' (mapped from '{}')", process_name, app_name);
    
    // Try to get PID by process name first
    let pid_script = format!(r#"
        tell application "System Events"
            set appProcess to first process whose name is "{}"
            return (unix id of appProcess) as text
        end tell
    "#, process_name);
    
    if let Ok(pid) = execute_applescript(&pid_script).await {
        info!("Found PID for {} by name: {}", process_name, pid);
        return Ok(pid);
    }
    
    // Fallback: try to find by frontmost app (if app was just launched/focused)
    let frontmost_script = r#"
        tell application "System Events"
            set frontApp to first process whose frontmost is true
            return (unix id of frontApp) as text
        end tell
    "#;
    
    if let Ok(pid) = execute_applescript(frontmost_script).await {
        info!("Using frontmost app PID as fallback: {}", pid);
        return Ok(pid);
    }
    
    // Final fallback: use pgrep
    let pgrep_output = tokio::process::Command::new("pgrep")
        .arg("-x")
        .arg(process_name)
        .output()
        .await;
    
    if let Ok(output) = pgrep_output {
        if output.status.success() {
            let pid = String::from_utf8_lossy(&output.stdout).trim().lines().next().unwrap_or("").to_string();
            if !pid.is_empty() {
                info!("Found PID for {} via pgrep: {}", process_name, pid);
                return Ok(pid);
            }
        }
    }
    
    Err(format!("Failed to get PID for {} (tried name lookup, frontmost, and pgrep)", process_name))
}

/// Execute AppleScript
pub async fn execute_applescript(script: &str) -> Result<String, String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .await
        .map_err(|e| format!("Failed to execute AppleScript: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Enter data into multiple Excel cells efficiently using AppleScript
/// cells_and_values: Vector of tuples containing (cell_reference, value)
/// Example: vec![("A1", "Product Name"), ("B1", "Price"), ("C1", "Quantity")]
pub async fn enter_excel_data(cells_and_values: Vec<(&str, &str)>) -> ActionResult {
    if cells_and_values.is_empty() {
        return ActionResult::failure("No cell data provided");
    }
    
    // Build the AppleScript
    let mut script = String::from("tell application \"Microsoft Excel\"\n");
    script.push_str("    activate\n");
    script.push_str("    delay 0.5\n"); // Small delay to ensure Excel is ready
    
    // Add each cell assignment
    for (cell, value) in cells_and_values.iter() {
        // Check if this is a formula (starts with =)
        if value.starts_with("=") {
            // For formulas, set the formula property directly without quotes
            script.push_str(&format!("    set formula of range \"{}\" to \"{}\"\n", cell, value));
        } else {
            // For regular values, escape quotes and backslashes
            let escaped_value = value
                .replace("\\", "\\\\")
                .replace("\"", "\\\"");
            script.push_str(&format!("    set value of range \"{}\" to \"{}\"\n", cell, escaped_value));
        }
    }
    
    script.push_str("end tell");
    
    info!("Executing AppleScript to enter data into {} Excel cells", cells_and_values.len());
    info!("AppleScript: {}", script);
    
    // Execute the AppleScript
    match execute_applescript(&script).await {
        Ok(_) => {
            info!("Successfully entered data into {} Excel cells", cells_and_values.len());
            ActionResult::success(&format!("Entered data into {} cells", cells_and_values.len()))
        },
        Err(e) => {
            error!("Failed to execute Excel AppleScript: {}", e);
            ActionResult::failure(&format!("Failed to enter Excel data: {}", e))
        }
    }
}

/// Insert text into Microsoft Word using AppleScript
/// This approach is more reliable than the accessibility API for Word on macOS
pub async fn insert_text_in_word(text: &str) -> ActionResult {
    info!("🔵 WORD APPLESCRIPT: Starting insertion for text: '{}'", text);

    // Escape quotes and backslashes for AppleScript
    let escaped_text = text
        .replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("\n", "\\n");

    info!("🔵 WORD APPLESCRIPT: Escaped text: '{}'", escaped_text);

    // Build the AppleScript to insert text using Word's native commands
    let script = format!(r#"
        tell application "Microsoft Word"
            activate
            delay 0.3
            try
                -- Insert text at the current cursor position
                insert text "{}" at selection
                return "success"
            on error errMsg
                -- Try alternative syntax if first fails
                try
                    set content of selection to "{}"
                    return "success (alternative method)"
                on error errMsg2
                    return "error: " & errMsg & " (also tried: " & errMsg2 & ")"
                end try
            end try
        end tell
    "#, escaped_text, escaped_text);

    info!("🔵 WORD APPLESCRIPT: Full script:\n{}", script);
    info!("🔵 WORD APPLESCRIPT: Executing AppleScript now...");

    // Execute the AppleScript
    match execute_applescript(&script).await {
        Ok(result) => {
            info!("🔵 WORD APPLESCRIPT: AppleScript execution completed with result: '{}'", result);
            if result.contains("error") {
                error!("🔴 WORD APPLESCRIPT: Error returned from AppleScript: {}", result);
                ActionResult::failure(&format!("Failed to insert text in Word: {}", result))
            } else {
                info!("🟢 WORD APPLESCRIPT: Successfully inserted text into Word");
                ActionResult::success(&format!("Successfully inserted '{}' into Word", text))
            }
        },
        Err(e) => {
            error!("🔴 WORD APPLESCRIPT: Failed to execute AppleScript command: {}", e);
            ActionResult::failure(&format!("Failed to insert text in Word: {}", e))
        }
    }
}

/// Helper function to get an element's path in the accessibility tree
pub fn get_element_path(element: &AXUIElement) -> Option<String> {
    // Create simplified path parts
    let mut path_parts = Vec::new();
    
    // Try to get role - this is the most important part
    let role_attr = AXAttribute::role();
    let role = match element.attribute(&role_attr) {
        Ok(role) => format!("{:?}", role),
        Err(_) => "unknown_role".to_string(),
    };
    
    // Try to get title
    let title_attr = AXAttribute::title();
    let title = match element.attribute(&title_attr) {
        Ok(title) => format!("{:?}", title),
        Err(_) => String::new(),
    };
    
    // Try to get identifier
    let id_attr = AXAttribute::identifier();
    let identifier = match element.attribute(&id_attr) {
        Ok(id) => format!("{:?}", id),
        Err(_) => String::new(),
    };
    
    // Create a simplified path
    if !identifier.is_empty() && identifier != "null" {
        path_parts.push(identifier);
    }
    
    if !title.is_empty() && title != "null" {
        path_parts.push(title);
    }
    
    path_parts.push(role);
    
    Some(path_parts.join("/"))
} 