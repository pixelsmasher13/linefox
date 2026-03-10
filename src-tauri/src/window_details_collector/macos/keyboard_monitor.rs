#![cfg(target_os = "macos")]

use std::sync::Mutex;
use log::{info, error, warn};
use lazy_static::lazy_static;
use chrono::Local;
use active_win_pos_rs;
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};
use crate::configuration::state::ServiceAccess;
use std::collections::HashSet;

// Global state for keyboard monitoring
lazy_static! {
    static ref KEYBOARD_MONITORING_ENABLED: Mutex<bool> = Mutex::new(false);
    static ref LAST_KEYBOARD_ACTION: Mutex<String> = Mutex::new(String::new());
    // Track currently pressed modifier keys
    static ref PRESSED_MODIFIERS: Mutex<HashSet<rdev::Key>> = Mutex::new(HashSet::new());
}

pub fn start_keyboard_monitoring() -> Result<(), String> {
    // Starting keyboard monitoring for shortcuts

    // Check if already enabled
    {
        let mut enabled = KEYBOARD_MONITORING_ENABLED.lock().unwrap();
        if *enabled {
            // Keyboard monitoring already enabled (silently return)
            return Ok(());
        }
        *enabled = true;
    }
    
    // Start real keyboard event monitoring using rdev
    std::thread::spawn(|| {
        // Keyboard monitoring thread started

        // Set up the keyboard event listener
        if let Err(error) = rdev::listen(keyboard_event_callback) {
            error!("❌ Error in keyboard monitoring: {:?}", error);
        }

        // Keyboard monitoring thread stopped
    });

    // Keyboard monitoring started successfully
    Ok(())
}

pub fn stop_keyboard_monitoring() {
    info!("🛑 Stopping keyboard monitoring");
    let mut enabled = KEYBOARD_MONITORING_ENABLED.lock().unwrap();
    *enabled = false;
}

pub fn is_keyboard_monitoring_enabled() -> bool {
    KEYBOARD_MONITORING_ENABLED.lock().unwrap().clone()
}

// Function to manually record a keyboard shortcut (can be called from other parts of the code)
pub fn record_keyboard_shortcut(shortcut_description: &str) {
    if !is_keyboard_monitoring_enabled() {
        return;
    }
    
    record_keyboard_action(shortcut_description);
}

// Real keyboard event callback using rdev
fn keyboard_event_callback(event: rdev::Event) {
    // Only process if monitoring is enabled
    if !is_keyboard_monitoring_enabled() {
        return;
    }
    
    match event.event_type {
        rdev::EventType::KeyPress(key) => {
            // Track modifier keys
            if is_modifier_key(&key) {
                let mut modifiers = PRESSED_MODIFIERS.lock().unwrap();
                modifiers.insert(key);
                return; // Don't log modifier keys themselves
            }
            
            // Only process non-modifier keys if we have modifiers pressed
            // OR if it's a special standalone key (Escape, F-keys, etc.)
            if let Some(shortcut_description) = detect_keyboard_shortcut(key, &event) {
                record_keyboard_action(&shortcut_description);
            }
        },
        rdev::EventType::KeyRelease(key) => {
            // Remove released modifier keys
            if is_modifier_key(&key) {
                let mut modifiers = PRESSED_MODIFIERS.lock().unwrap();
                modifiers.remove(&key);
            }
        },
        _ => {} // Ignore other event types
    }
}

// Check if a key is a modifier key
fn is_modifier_key(key: &rdev::Key) -> bool {
    matches!(key, 
        rdev::Key::ShiftLeft | rdev::Key::ShiftRight |
        rdev::Key::ControlLeft | rdev::Key::ControlRight |
        rdev::Key::Alt | // Alt key (covers both left and right)
        rdev::Key::MetaLeft | rdev::Key::MetaRight | // Cmd key on macOS
        rdev::Key::Function
    )
}

// Check if any modifier keys are currently pressed
fn has_modifiers_pressed() -> bool {
    let modifiers = PRESSED_MODIFIERS.lock().unwrap();
    !modifiers.is_empty()
}

// Get string representation of currently pressed modifiers
fn get_modifier_string() -> String {
    let modifiers = PRESSED_MODIFIERS.lock().unwrap();
    let mut modifier_parts = Vec::new();
    
    // Check for Cmd (Meta) key - most common on macOS
    if modifiers.contains(&rdev::Key::MetaLeft) || modifiers.contains(&rdev::Key::MetaRight) {
        modifier_parts.push("Cmd");
    }
    
    // Check for Ctrl key
    if modifiers.contains(&rdev::Key::ControlLeft) || modifiers.contains(&rdev::Key::ControlRight) {
        modifier_parts.push("Ctrl");
    }
    
    // Check for Alt key
    if modifiers.contains(&rdev::Key::Alt) {
        modifier_parts.push("Alt");
    }
    
    // Check for Shift key
    if modifiers.contains(&rdev::Key::ShiftLeft) || modifiers.contains(&rdev::Key::ShiftRight) {
        modifier_parts.push("Shift");
    }
    
    // Check for Function key
    if modifiers.contains(&rdev::Key::Function) {
        modifier_parts.push("Fn");
    }
    
    modifier_parts.join("+")
}

// Detect keyboard shortcuts - ONLY with modifiers or special standalone keys
fn detect_keyboard_shortcut(key: rdev::Key, _event: &rdev::Event) -> Option<String> {
    let has_modifiers = has_modifiers_pressed();
    let modifier_string = get_modifier_string();
    
    // For letter/number keys, ONLY capture if modifiers are pressed
    match key {
        // Common shortcut keys - ONLY with modifiers
        rdev::Key::KeyA if has_modifiers => Some(format!("{}+A (Select All)", modifier_string)),
        rdev::Key::KeyC if has_modifiers => Some(format!("{}+C (Copy)", modifier_string)),
        rdev::Key::KeyV if has_modifiers => Some(format!("{}+V (Paste)", modifier_string)),
        rdev::Key::KeyX if has_modifiers => Some(format!("{}+X (Cut)", modifier_string)),
        rdev::Key::KeyZ if has_modifiers => Some(format!("{}+Z (Undo)", modifier_string)),
        rdev::Key::KeyY if has_modifiers => Some(format!("{}+Y (Redo)", modifier_string)),
        rdev::Key::KeyS if has_modifiers => Some(format!("{}+S (Save)", modifier_string)),
        rdev::Key::KeyO if has_modifiers => Some(format!("{}+O (Open)", modifier_string)),
        rdev::Key::KeyN if has_modifiers => Some(format!("{}+N (New)", modifier_string)),
        rdev::Key::KeyF if has_modifiers => Some(format!("{}+F (Find)", modifier_string)),
        rdev::Key::KeyR if has_modifiers => Some(format!("{}+R (Refresh)", modifier_string)),
        rdev::Key::KeyT if has_modifiers => Some(format!("{}+T (New Tab)", modifier_string)),
        rdev::Key::KeyW if has_modifiers => Some(format!("{}+W (Close Tab)", modifier_string)),
        rdev::Key::KeyL if has_modifiers => Some(format!("{}+L (Lock/Location)", modifier_string)),
        rdev::Key::KeyP if has_modifiers => Some(format!("{}+P (Print)", modifier_string)),
        rdev::Key::KeyQ if has_modifiers => Some(format!("{}+Q (Quit)", modifier_string)),
        rdev::Key::KeyD if has_modifiers => Some(format!("{}+D (Duplicate/Bookmark)", modifier_string)),
        rdev::Key::KeyG if has_modifiers => Some(format!("{}+G (Find Next)", modifier_string)),
        rdev::Key::KeyH if has_modifiers => Some(format!("{}+H (Hide)", modifier_string)),
        rdev::Key::KeyI if has_modifiers => Some(format!("{}+I (Info/Italic)", modifier_string)),
        rdev::Key::KeyJ if has_modifiers => Some(format!("{}+J (Downloads)", modifier_string)),
        rdev::Key::KeyK if has_modifiers => Some(format!("{}+K (Link)", modifier_string)),
        rdev::Key::KeyM if has_modifiers => Some(format!("{}+M (Minimize)", modifier_string)),
        rdev::Key::KeyU if has_modifiers => Some(format!("{}+U (Underline)", modifier_string)),
        rdev::Key::KeyB if has_modifiers => Some(format!("{}+B (Bold)", modifier_string)),
        rdev::Key::KeyE if has_modifiers => Some(format!("{}+E (Eject/Center)", modifier_string)),
        
        // Number keys with modifiers (for tab switching, etc.)
        rdev::Key::Num1 if has_modifiers => Some(format!("{}+1", modifier_string)),
        rdev::Key::Num2 if has_modifiers => Some(format!("{}+2", modifier_string)),
        rdev::Key::Num3 if has_modifiers => Some(format!("{}+3", modifier_string)),
        rdev::Key::Num4 if has_modifiers => Some(format!("{}+4", modifier_string)),
        rdev::Key::Num5 if has_modifiers => Some(format!("{}+5", modifier_string)),
        rdev::Key::Num6 if has_modifiers => Some(format!("{}+6", modifier_string)),
        rdev::Key::Num7 if has_modifiers => Some(format!("{}+7", modifier_string)),
        rdev::Key::Num8 if has_modifiers => Some(format!("{}+8", modifier_string)),
        rdev::Key::Num9 if has_modifiers => Some(format!("{}+9", modifier_string)),
        rdev::Key::Num0 if has_modifiers => Some(format!("{}+0", modifier_string)),
        
        // Special keys that are meaningful even without modifiers
        rdev::Key::Escape => Some("Escape".to_string()),
        rdev::Key::Return => Some("Enter".to_string()),
        rdev::Key::Tab if has_modifiers => Some(format!("{}+Tab", modifier_string)),
        rdev::Key::Tab => Some("Tab".to_string()),
        rdev::Key::Space if has_modifiers => Some(format!("{}+Space", modifier_string)),
        rdev::Key::Backspace if has_modifiers => Some(format!("{}+Backspace", modifier_string)),
        rdev::Key::Delete if has_modifiers => Some(format!("{}+Delete", modifier_string)),
        
        // Arrow keys - capture both with and without modifiers as they're often shortcuts
        rdev::Key::UpArrow if has_modifiers => Some(format!("{}+Up Arrow", modifier_string)),
        rdev::Key::DownArrow if has_modifiers => Some(format!("{}+Down Arrow", modifier_string)),
        rdev::Key::LeftArrow if has_modifiers => Some(format!("{}+Left Arrow", modifier_string)),
        rdev::Key::RightArrow if has_modifiers => Some(format!("{}+Right Arrow", modifier_string)),
        rdev::Key::UpArrow => Some("Up Arrow".to_string()),
        rdev::Key::DownArrow => Some("Down Arrow".to_string()),
        rdev::Key::LeftArrow => Some("Left Arrow".to_string()),
        rdev::Key::RightArrow => Some("Right Arrow".to_string()),
        
        // Function keys - always capture as they're typically shortcuts
        rdev::Key::F1 => Some(if has_modifiers { format!("{}+F1", modifier_string) } else { "F1".to_string() }),
        rdev::Key::F2 => Some(if has_modifiers { format!("{}+F2", modifier_string) } else { "F2".to_string() }),
        rdev::Key::F3 => Some(if has_modifiers { format!("{}+F3", modifier_string) } else { "F3".to_string() }),
        rdev::Key::F4 => Some(if has_modifiers { format!("{}+F4", modifier_string) } else { "F4".to_string() }),
        rdev::Key::F5 => Some(if has_modifiers { format!("{}+F5", modifier_string) } else { "F5".to_string() }),
        rdev::Key::F6 => Some(if has_modifiers { format!("{}+F6", modifier_string) } else { "F6".to_string() }),
        rdev::Key::F7 => Some(if has_modifiers { format!("{}+F7", modifier_string) } else { "F7".to_string() }),
        rdev::Key::F8 => Some(if has_modifiers { format!("{}+F8", modifier_string) } else { "F8".to_string() }),
        rdev::Key::F9 => Some(if has_modifiers { format!("{}+F9", modifier_string) } else { "F9".to_string() }),
        rdev::Key::F10 => Some(if has_modifiers { format!("{}+F10", modifier_string) } else { "F10".to_string() }),
        rdev::Key::F11 => Some(if has_modifiers { format!("{}+F11", modifier_string) } else { "F11".to_string() }),
        rdev::Key::F12 => Some(if has_modifiers { format!("{}+F12", modifier_string) } else { "F12".to_string() }),
        
        // Page navigation keys
        rdev::Key::PageUp if has_modifiers => Some(format!("{}+Page Up", modifier_string)),
        rdev::Key::PageDown if has_modifiers => Some(format!("{}+Page Down", modifier_string)),
        rdev::Key::Home if has_modifiers => Some(format!("{}+Home", modifier_string)),
        rdev::Key::End if has_modifiers => Some(format!("{}+End", modifier_string)),
        rdev::Key::PageUp => Some("Page Up".to_string()),
        rdev::Key::PageDown => Some("Page Down".to_string()),
        rdev::Key::Home => Some("Home".to_string()),
        rdev::Key::End => Some("End".to_string()),
        
        // Ignore all other keys (regular typing)
        _ => None,
    }
}

fn record_keyboard_action(shortcut_description: &str) {
    // Avoid duplicate logging of the same shortcut
    {
        let mut last_action = LAST_KEYBOARD_ACTION.lock().unwrap();
        if *last_action == *shortcut_description {
            return; // Skip duplicate
        }
        *last_action = shortcut_description.to_string();
    }
    
    // Get current window info
    let (window_title, app_name) = get_current_window_info();
    
    let action_entry = format!(
        "⌨️ Keyboard Shortcut: {} (Window: {}, App: {})",
        shortcut_description,
        window_title,
        app_name
    );
    
    // Use the same record_action pattern as accessibility events
    record_action(&action_entry, &window_title, &app_name, 0);
}

// Helper function that follows the exact same pattern as the accessibility engine
fn record_action(action_entry: &str, window_title: &str, window_app_name: &str, _pid: i32) {
    // Store the action for retrieval (same as accessibility engine)
    use crate::window_details_collector::macos::macos_accessibility_engine::DETECTED_ACTIONS;
    if let Ok(mut actions) = DETECTED_ACTIONS.lock() {
        actions.push_str(action_entry);
        actions.push('\n'); // Add newline for separation
    }
    
    // Log the action with timestamp (same format as accessibility engine)
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S.%3f").to_string();
    info!("👤 User action: [{}] {}", timestamp, action_entry.trim());
    
    // Save to DB if recording is enabled (same as accessibility engine)
    if crate::window_details_collector::macos::macos_accessibility_engine::is_recording_enabled() {
        save_keyboard_action_to_db(action_entry.to_string(), window_title.to_string(), window_app_name.to_string());
    }
}

fn get_current_window_info() -> (String, String) {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        // Pre-check: skip if frontmostApplication is nil (avoids abort in active-win-pos-rs)
        let frontmost_ok = unsafe {
            let cls = match objc::runtime::Class::get("NSWorkspace") {
                Some(c) => c,
                None => { let _ = tx.send(("Unknown".to_string(), "Unknown".to_string())); return; },
            };
            let workspace: *mut objc::runtime::Object = msg_send![cls, sharedWorkspace];
            if workspace.is_null() { let _ = tx.send(("Unknown".to_string(), "Unknown".to_string())); return; }
            let app: *mut objc::runtime::Object = msg_send![workspace, frontmostApplication];
            !app.is_null()
        };

        let window_info = if !frontmost_ok {
            ("Unknown".to_string(), "Unknown".to_string())
        } else {
            match std::panic::catch_unwind(|| active_win_pos_rs::get_active_window()) {
                Ok(Ok(window)) => {
                    (
                        if window.title.trim().is_empty() {
                            window.app_name.clone()
                        } else {
                            window.title
                        },
                        window.app_name
                    )
                },
                _ => ("Unknown".to_string(), "Unknown".to_string())
            }
        };

        let _ = tx.send(window_info);
    });

    match rx.recv_timeout(Duration::from_millis(100)) {
        Ok(info) => info,
        Err(_) => {
            warn!("Timeout or error getting window info");
            ("Unknown".to_string(), "Unknown".to_string())
        }
    }
}

fn save_keyboard_action_to_db(action_entry: String, window_title: String, window_app_name: String) {
    // Get the app handle from the accessibility engine
    use crate::window_details_collector::macos::macos_accessibility_engine::APP_HANDLE;
    
    let app_handle = match APP_HANDLE.lock() {
        Ok(handle) => {
            if let Some(h) = handle.clone() {
                h
            } else {
                info!("❌ Cannot save keyboard action - app handle not set");
                return;
            }
        },
        Err(_) => {
            info!("❌ Cannot save keyboard action - failed to acquire app handle lock");
            return;
        }
    };
    
    // Create a timestamp
    let timestamp = Local::now().to_rfc3339();
    
    // Get the current automation ID if we're recording for an automation
    use crate::window_details_collector::macos::macos_accessibility_engine::CURRENT_AUTOMATION_ID;
    let automation_id = match CURRENT_AUTOMATION_ID.lock() {
        Ok(id) => id.clone(),
        Err(_) => {
            info!("⚠️ Failed to get current automation ID lock for keyboard action, defaulting to None");
            None
        }
    };
    
    // Create the activity item
    let activity_item = crate::entity::activity_item::ActivityItem {
        timestamp,
        ocr_text: String::new(),
        full_activity_text: String::new(),
        editing_mode: String::new(),
        original_ocr_text: String::new(),
        window_title: window_title.clone(),
        window_app_name: window_app_name.clone(),
        user_id: "default".to_string(),
        os_details: "macOS".to_string(),
        similarity_percentage_to_previous_ocr_text: "0.0".to_string(),
        interval_length: 0,
        keypress_count: 0,
        element_tree_dump: "Same".to_string(), // Don't capture full tree for keyboard shortcuts
        detected_actions: action_entry,
        automation_id,
    };
    
    // Save to database
    if let Err(e) = app_handle.db(|db| {
        crate::repository::activity_log_repository::save_activity_item(&activity_item, db)
    }) {
        info!("❌ Failed to save keyboard action to database: {:?}", e);
        return;
    }
    
    info!("✅ Saved keyboard action to database for {}{}", 
        window_app_name,
        if automation_id.is_some() { 
            format!(" (automation ID: {})", automation_id.unwrap()) 
        } else { 
            String::new() 
        }
    );
} 