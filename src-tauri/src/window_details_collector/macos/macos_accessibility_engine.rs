#![cfg(any(target_os = "macos"))]

use std::collections::{VecDeque, HashMap, HashSet};
use std::ptr;
use std::sync::Mutex;
use std::ffi::c_void;
use chrono::Local;
use log::{info, debug, error, warn};
use lazy_static::lazy_static;
use crate::configuration::state::ServiceAccess;

// Only use entity which is exported in lib.rs
use crate::entity::macos_element_details::ElementDetails;
use tauri::AppHandle;
use accessibility::AXAttribute;
use accessibility::AXUIElement;
use accessibility_sys::{
    kAXAnnouncementRequestedNotification, kAXCreatedNotification, kAXFocusedApplicationAttribute,
    kAXFocusedUIElementChangedNotification, kAXTrustedCheckOptionPrompt,
    kAXUIElementDestroyedNotification, kAXValueChangedNotification, AXIsProcessTrustedWithOptions,
    AXObserverAddNotification, AXObserverCreateWithInfoCallback, AXObserverGetRunLoopSource,
    AXObserverRef, AXUIElementCopyAttributeValue, AXUIElementCreateApplication, AXUIElementRef,
    AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout,
};
use core_foundation::array::CFArray;
use core_foundation::base::{Boolean, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::runloop::{
    kCFRunLoopDefaultMode, CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRunInMode,
};
use core_foundation::string::{CFString, CFStringRef};
use active_win_pos_rs;
use std::sync::atomic::{AtomicBool, Ordering};
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl, class};

/// Safe wrapper around `active_win_pos_rs::get_active_window()`.
///
/// The underlying crate can abort (non-unwinding panic) when
/// `NSWorkspace.sharedWorkspace().frontmostApplication()` returns nil -- which happens
/// during app transitions, screen-saver activation, or when a process just terminated.
/// `catch_unwind` cannot save us from an abort, so we pre-check with a raw Objective-C
/// message send to verify that the pointer is non-null before handing control to the crate.
fn safe_get_active_window() -> Result<active_win_pos_rs::ActiveWindow, ()> {
    // 1. Pre-check: is frontmostApplication non-nil?
    let has_frontmost = unsafe {
        let cls = objc::runtime::Class::get("NSWorkspace")
            .expect("NSWorkspace class must exist on macOS");
        let workspace: *mut objc::runtime::Object = objc::msg_send![cls, sharedWorkspace];
        if workspace.is_null() { return Err(()); }
        let app: *mut objc::runtime::Object = objc::msg_send![workspace, frontmostApplication];
        !app.is_null()
    };

    if !has_frontmost {
        info!("⚠️ safe_get_active_window: frontmostApplication is nil, skipping");
        return Err(());
    }

    // 2. The pointer is valid — delegate to the crate (still catch_unwind for other panics)
    match std::panic::catch_unwind(|| active_win_pos_rs::get_active_window()) {
        Ok(result) => result.map_err(|_| ()),
        Err(_) => {
            warn!("⚠️ safe_get_active_window: caught panic from get_active_window");
            Err(())
        }
    }
}

// Global atomic flag to track when UI tree needs refresh
lazy_static! {
    pub static ref UI_TREE_DIRTY: AtomicBool = AtomicBool::new(false);
    /// When true, observer_callback silently discards all AX notifications.
    /// Used by clipboard-based FULL_TEXT extraction to prevent notification floods.
    pub static ref OBSERVER_PAUSED: AtomicBool = AtomicBool::new(false);
}

// A global variable to track recording state
lazy_static! {
    static ref IS_RECORDING_ENABLED: Mutex<bool> = Mutex::new(false);
    // Store pending typing actions
    static ref PENDING_ACTION: Mutex<Option<ActionInfo>> = Mutex::new(None);
    // Track the last non-typing action to avoid duplicates
    static ref LAST_ACTION: Mutex<ActionInfo> = Mutex::new(ActionInfo::default());
    // Store the current automation ID during recording
    pub static ref CURRENT_AUTOMATION_ID: Mutex<Option<i64>> = Mutex::new(None);
}

// Track which PIDs already have observers set up
lazy_static::lazy_static! {
    static ref OBSERVED_PIDS: Mutex<HashSet<i32>> = Mutex::new(HashSet::new());
}

// Struct to store action information for comparison
#[derive(Clone, Default, PartialEq)]
struct ActionInfo {
    timestamp: String,
    notification: String,
    role: String,
    value: String,
    description: String, 
    identifier: String,
    title: String,
    subrole: String,
    window_title: String,
    window_app_name: String,
    action_entry: String,
}

// Function to check recording state
pub fn is_recording_enabled() -> bool {
    if let Ok(recording_state) = IS_RECORDING_ENABLED.lock() {
        return *recording_state;
    }
    false
}

// Function to set recording state
pub fn set_recording_enabled(enabled: bool) {
    info!("🚨 CRITICAL: set_recording_enabled({}) called!", enabled);
    
    // Flag to track if we need to flush after releasing lock
    let should_flush = {
        if let Ok(mut recording_state) = IS_RECORDING_ENABLED.lock() {
            // If we're turning off recording, flush any pending actions
            if *recording_state && !enabled {
                info!("🚨 STOPPING RECORDING: Disabling recording that was previously enabled!");
                
                // FIRST: Set recording state to false to prevent any new actions from being saved
                *recording_state = false;
                info!("🎬 Recording state set to: DISABLED ❌");
                
                // Return true to indicate we should flush after releasing the lock
                true
            } else if !*recording_state && enabled {
                info!("🚨 STARTING RECORDING: Enabling recording that was previously disabled!");
                *recording_state = true;
                info!("🎬 Recording state changed to: ENABLED ✅");
                false
            } else {
                info!("🚨 Recording state unchanged: was {} and setting to {}", *recording_state, enabled);
                // Force the state anyway to ensure it takes effect
                *recording_state = enabled;
                info!("🎬 Recording state forced to: {}", if enabled { "ENABLED ✅" } else { "DISABLED ❌" });
                false
            }
        } else {
            error!("🚨 FAILED to lock IS_RECORDING_ENABLED mutex!");
            false
        }
    }; // Lock is released here
    
    // Now flush pending actions if needed (after releasing the lock)
    if should_flush {
        flush_pending_actions();
        
        // Also reset the current automation ID when recording stops
        if let Ok(mut automation_id) = CURRENT_AUTOMATION_ID.lock() {
            let previous_id = automation_id.take();
            info!("🎬 Reset automation ID after recording stopped. Previous ID: {:?}", previous_id);
        }
        
        info!("🎬 Recording stop sequence completed");
    }
}

// New function to set the current automation ID
pub fn set_current_automation_id(automation_id: i64) {
    if let Ok(mut current_id) = CURRENT_AUTOMATION_ID.lock() {
        *current_id = Some(automation_id);
        info!("🎬 Current automation ID set to: {}", automation_id);
    }
}

// Function to get the current automation ID
pub fn get_current_automation_id() -> Option<i64> {
    match CURRENT_AUTOMATION_ID.lock() {
        Ok(id) => id.clone(),
        Err(_) => {
            warn!("Failed to get current automation ID lock");
            None
        }
    }
}

// Store detected actions
lazy_static! {
    pub static ref DETECTED_ACTIONS: Mutex<String> = Mutex::new(String::new());
    // Track last seen values to avoid duplicate reports
    static ref LAST_VALUES: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
    // App handle for DB access
    pub static ref APP_HANDLE: Mutex<Option<AppHandle>> = Mutex::new(None);
    // Store last element tree state
    static ref LAST_ELEMENT_TREE: Mutex<String> = Mutex::new(String::new());
    static ref LAST_ELEMENT_TREE_TIME: Mutex<std::time::Instant> = Mutex::new(std::time::Instant::now());
    static ref LAST_WINDOW_NAME: Mutex<String> = Mutex::new(String::new());
}

// Set global app handle
pub fn set_app_handle(handle: AppHandle) {
    if let Ok(mut app_handle) = APP_HANDLE.lock() {
        *app_handle = Some(handle);
        info!("✅ App handle set for DB access");
    }
}

pub fn initialize_continuous_monitoring() {
    // Don't automatically request permissions - let the UI handle it
    
    // Get the current active window information directly
    let active_window = match safe_get_active_window() {
        Ok(window) => window,
        _ => {
            info!("Could not get active window for initial monitoring");
            return;
        }
    };
    
    let own_pid = std::process::id() as u64;
    let pid = active_window.process_id.to_string();
    info!("🔍 STARTUP: Monitoring initial app: {} (PID: {})", active_window.app_name, active_window.process_id);
    
    // Only observe external apps — never ourselves
    if active_window.process_id != own_pid {
        observe_by_pid(&pid);
    } else {
        info!("🔍 STARTUP: Skipping observer for own process (PID: {})", own_pid);
    }
    
    // Start a background thread to monitor app switching
    std::thread::spawn(move || {
        info!("🔄 Starting app switching monitor thread");
        let mut last_pid = 0;
        
        loop {
            // Check for app switches every 200ms
            if let Ok(current_window) = safe_get_active_window() {
                let current_pid = current_window.process_id;
                
                // Only log when we detect a change
                if current_pid != last_pid {
                    // Never set up observers on our own process — it causes AX callbacks
                    // to fire on every keystroke/paste into Linefox's own UI.
                    if current_pid == own_pid {
                        info!("🔀 APP SWITCH: Linefox focused — skipping self-observation");
                    } else {
                        info!("🔀 APP SWITCH: Now active: {} (PID: {})", 
                              current_window.app_name, current_pid);
                        observe_by_pid(&current_pid.to_string());
                    }
                    last_pid = current_pid;
                }
            }
            
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    });
}

pub fn get_element_tree_by_pid(pid: &str) -> (String, String) {
    let pid = pid.parse::<i32>().unwrap();
    let application = AXUIElement::application(pid);
    let all_window_element_trees = scan_frontmost_window(&application);
    
    // Get the detected actions
    let actions = if let Ok(mut actions_lock) = DETECTED_ACTIONS.lock() {
        let actions_copy = actions_lock.clone();
        // Clear for next round
        *actions_lock = String::new();
        actions_copy
    } else {
        String::new()
    };
    
    (all_window_element_trees, actions)
}

pub fn by_pid(pid: &str) -> String {
    let (element_tree, _) = get_element_tree_by_pid(pid);
    return element_tree;
}

pub fn observe_by_pid(pid: &str) -> () {
    debug!("Setting up observers for PID: {}", pid);
    let pid = pid.parse::<i32>().unwrap_or_else(|e| {
        info!("❌ Failed to parse PID: {} - Error: {}", pid, e);
        return 0;
    });

    // Skip if PID is invalid
    if pid <= 0 {
        info!("⚠️ Skipping invalid PID: {}", pid);
        return;
    }

    match setup_notifications(pid) {
        Ok(()) => info!("✅ Successfully setup notifications for PID: {}", pid),
        Err(e) => info!("❌ Error setting up notifications for PID {}: {}", pid, e),
    }

    // Run the loop for longer to ensure events register
    debug!("Running event loop for PID: {}", pid);
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.5, Boolean::from(false));
    }
    debug!("Finished initial event loop for PID: {}", pid);
}

fn scan_windows(application: &AXUIElement) -> String {
    let mut contents = String::new();

    let windows: CFArray<AXUIElement> = application
        .attribute(&AXAttribute::windows())
        .unwrap_or(CFArray::from_CFTypes(&[]));

    for window in windows.iter() {
        let elem_contents = walk_elements(&window, 1); // Limit depth to 1 level for maximum performance
        contents.push_str(&elem_contents);
    }

    contents
}

// New focused window scanning function
fn scan_frontmost_window(application: &AXUIElement) -> String {
    // Try to get the focused window (the one in front)
    // Create a CFString for "AXFocusedWindow"
    let focused_window_attr = CFString::new("AXFocusedWindow");
    let attr = AXAttribute::new(&focused_window_attr);
    
    // Get the focused window
    match application.attribute(&attr) {
        Ok(cf_type) => {
            // We need to convert the CFType to AXUIElement
            if let Some(window) = cf_type.downcast::<AXUIElement>() {
                // Process only this window
                let window_contents = walk_elements(&window, 1); // Limit depth to 1 level for maximum performance
                info!("🔍 Scanning focused window only");
                return window_contents;
            } else {
                info!("⚠️ Could not cast focused window to AXUIElement, falling back to all windows");
                return scan_windows(application);
            }
        },
        Err(_) => {
            info!("⚠️ Could not get focused window, falling back to all windows");
            // Fall back to scanning all windows if we can't get the focused one
            return scan_windows(application);
        }
    }
}

fn walk_elements(element: &AXUIElement, max_depth: usize) -> String {
    let mut contents = String::new();
    let mut stack = VecDeque::new();
    // Store elements with their depth
    stack.push_back((element.clone(), 0));

    while let Some((current_element, current_depth)) = stack.pop_front() {
        // Stop traversal if we've reached max depth
        if current_depth > max_depth {
            continue;
        }

        // Check if element is visible before processing
        // Use a custom implementation since there's no direct hidden attribute
        let is_visible = !is_element_hidden(&current_element);
            
        if !is_visible {
            continue; // Skip hidden elements
        }
        
        // Get role to check relevance
        let role = current_element
            .attribute(&AXAttribute::role())
            .map(|r| format!("{:?}", r))
            .unwrap_or_else(|_| "Unknown".to_string());
            
        // Skip some less relevant element types that tend to create noise
        // Adjust this list based on your specific needs
        if role.contains("AXGroup") && current_depth > 1 {
            // Skip deeply nested groups
            continue;
        }
        
        // Skip elements that are likely not visible or interactable
        if role.contains("Unknown") || role.contains("AXLayoutArea") || 
           (role.contains("AXStaticText") && current_depth > 1) {
            continue;
        }

        // Only get value attribute for performance
        if let Ok(val) = current_element.attribute(&AXAttribute::value()) {
            let value_str = format!("{:?}", val);
            let value = if value_str.contains("Private") || value_str.contains("Incognito") {
                "Private Browsing".to_string()
            } else if value_str.contains("contents = ") {
                if let Some(start_idx) = value_str.find("contents = ") {
                    let start = start_idx + 12;
                    let end = value_str.len().saturating_sub(3);
                    if start < end {
                        value_str[start..end].replace("\\\"", "")
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            
            if !value.is_empty() {
                contents.push_str(&value);
                contents.push(' ');
            }
        }

        // Only get children if we haven't reached max depth
        if current_depth < max_depth {
            let children: CFArray<AXUIElement> = current_element.attribute(&AXAttribute::children())
                .unwrap_or_else(|_| CFArray::from_CFTypes(&[]));
                
            // If we have too many siblings (like in a list view), just sample a few
            let child_limit = if children.len() > 10 && current_depth > 3 {
                // For deep elements with many children (like email lists),
                // just sample the first few
                3_usize
            } else {
                let len = children.len();
                if len >= 0 {
                    len as usize
                } else {
                    0
                }
            };

            for (i, child) in children.iter().enumerate() {
                if i < child_limit {
                    stack.push_back((child.clone(), current_depth + 1));
                }
            }
        }
    }
    contents
}

// Helper function to check if an element is hidden
fn is_element_hidden(element: &AXUIElement) -> bool {
    // Try to get the role first - if we can't get a role, consider it hidden
    let role = match element.attribute(&AXAttribute::role()) {
        Ok(r) => format!("{:?}", r),
        Err(_) => return true, // No role usually means not visible
    };
    
    // Some elements are never shown to users directly
    if role.contains("AXUnknown") || 
       role.contains("AXLayoutArea") || 
       role.contains("AXHelpTag") {
        return true;
    }
    
    // Check parent for visibility if we need more robust checking
    // This would require tracking parents which is complex
    
    false // Default to visible
}

#[allow(dead_code)]
fn get_element_properties(element: &AXUIElement) -> ElementDetails {
    let mut value = String::new();

    if let Ok(val) = element.attribute(&AXAttribute::value()) {
        let value_str = format!("{:?}", val);
        if value_str.contains("Private") || value_str.contains("Incognito") {
            value = "Private Browsing".to_string();
        } else if value_str.contains("contents = ") {
            let start_index = value_str.find("contents = ").unwrap() + 12;
            let end_index = value_str.len() - 3;
            value = value_str[start_index..end_index].to_string();
            value = value.replace("\\\"", "");
        }
    }

    // Get role
    let role = element
        .attribute(&AXAttribute::role())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| "".to_string());

    // Get subrole
    let subrole = element
        .attribute(&AXAttribute::subrole())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| "".to_string());

    // Get description
    let description = element
        .attribute(&AXAttribute::description())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| "".to_string());

    // Get identifier
    let identifier = element
        .attribute(&AXAttribute::identifier())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| "".to_string());

    // Get title
    let title = element
        .attribute(&AXAttribute::title())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| "".to_string());

    // Get help
    let help = element
        .attribute(&AXAttribute::help())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| "".to_string());

    // Position and size are not available as direct attributes in this version of accessibility
    let position = String::new(); 
    let size = String::new();

    ElementDetails { 
        value, 
        role, 
        subrole, 
        description, 
        identifier, 
        title, 
        help, 
        position, 
        size 
    }
}

fn setup_notifications(pid: i32) -> Result<(), String> {
    debug!("Setting up notifications for PID: {}", pid);
    unsafe {
        // Create new observer
        let mut observer_ref: AXObserverRef = ptr::null_mut();
        let result = AXObserverCreateWithInfoCallback(pid, observer_callback, &mut observer_ref);
        if result != 0 {
            info!("❌ Failed to create observer for PID {}: error code {}", pid, result);
            return Err(format!("Failed to create observer: {result}"));
        }
        debug!("Observer created for PID: {}", pid);

        let application = AXUIElementCreateApplication(pid);

        configure_application(application);

        add_notification_types_to_observer(observer_ref, application);

        let run_loop_source = AXObserverGetRunLoopSource(observer_ref);
        if run_loop_source.is_null() {
            info!("❌ Failed to get run loop source for PID: {}", pid);
            return Err("Failed to get run loop source".to_string());
        }

        // Add to run loop
        CFRunLoopAddSource(
            CFRunLoopGetCurrent(),
            run_loop_source,
            kCFRunLoopDefaultMode,
        );
    }
    Ok(())
}

unsafe fn configure_application(application: AXUIElementRef) {
    AXUIElementSetMessagingTimeout(application, 5.0);
    AXUIElementCopyAttributeValue(
        application,
        CFString::new(kAXFocusedApplicationAttribute).as_concrete_TypeRef(),
        ptr::null_mut(),
    );
    let _ = AXUIElementSetAttributeValue(
        application,
        CFString::new("AXInspectorEnabled").as_concrete_TypeRef(),
        CFBoolean::true_value().as_CFTypeRef(),
    );
    let _ = AXUIElementSetAttributeValue(
        application,
        CFString::new("AXEnhancedUserInterface").as_concrete_TypeRef(),
        CFBoolean::true_value().as_CFTypeRef(),
    );
    let _ = AXUIElementSetAttributeValue(
        application,
        CFString::new("AXManualAccessibility").as_concrete_TypeRef(),
        CFBoolean::true_value().as_CFTypeRef(),
    );
}

unsafe fn add_notification_types_to_observer(
    observer_ref: AXObserverRef,
    application: AXUIElementRef,
) {
    let element_did_announce = CFString::new(kAXAnnouncementRequestedNotification);
    let element_did_disappear = CFString::new(kAXUIElementDestroyedNotification);
    let element_did_get_focus = CFString::new(kAXFocusedUIElementChangedNotification);
    let element_did_appear = CFString::new(kAXCreatedNotification);
    let value_changed_notification = CFString::new(kAXValueChangedNotification);
    
    // Add structural change notifications
    let children_changed = CFString::new("AXChildrenChanged");
    let focused_window_changed = CFString::new("AXFocusedWindowChanged");
    let layout_changed = CFString::new("AXLayoutChanged");

    let _ = AXObserverAddNotification(
        observer_ref,
        application,
        element_did_disappear.as_concrete_TypeRef(),
        ptr::null_mut(),
    );
    let _ = AXObserverAddNotification(
        observer_ref,
        application,
        element_did_get_focus.as_concrete_TypeRef(),
        ptr::null_mut(),
    );
    let _ = AXObserverAddNotification(
        observer_ref,
        application,
        element_did_announce.as_concrete_TypeRef(),
        ptr::null_mut(),
    );
    let _ = AXObserverAddNotification(
        observer_ref,
        application,
        element_did_appear.as_concrete_TypeRef(),
        ptr::null_mut(),
    );
    let _ = AXObserverAddNotification(
        observer_ref,
        application,
        value_changed_notification.as_concrete_TypeRef(),
        ptr::null_mut(),
    );
    
    // Register the new structural change notifications
    let _ = AXObserverAddNotification(
        observer_ref,
        application,
        children_changed.as_concrete_TypeRef(),
        ptr::null_mut(),
    );
    let _ = AXObserverAddNotification(
        observer_ref,
        application,
        focused_window_changed.as_concrete_TypeRef(),
        ptr::null_mut(),
    );
    let _ = AXObserverAddNotification(
        observer_ref,
        application,
        layout_changed.as_concrete_TypeRef(),
        ptr::null_mut(),
    );
    
    debug!("Registered all notifications including structural change events");
}

// Helper function to walk an element and its immediate children to a limited depth
#[allow(dead_code)]
fn walk_element_with_children(element: &AXUIElement, max_depth: usize) -> String {
    let mut context = String::new();
    
    // Get this element's details
    let element_details = get_element_properties(element);
    
    // Format element details for readability
    let details = format!(
        "[{}{}{}{}{}]", 
        if !element_details.role.is_empty() { &element_details.role } else { "Unknown" },
        if !element_details.title.is_empty() { format!(" title:{}", element_details.title) } else { "".to_string() },
        if !element_details.identifier.is_empty() { format!(" id:{}", element_details.identifier) } else { "".to_string() },
        if !element_details.description.is_empty() { format!(" desc:{}", element_details.description) } else { "".to_string() },
        if !element_details.value.is_empty() { format!(" val:\"{}\"", element_details.value) } else { "".to_string() }
    );
    
    context.push_str(&details);
    
    // Only explore children if we haven't reached max depth
    if max_depth > 0 {
        let children: CFArray<AXUIElement> = element
            .attribute(&AXAttribute::children())
            .unwrap_or_else(|_| CFArray::from_CFTypes(&[]));
            
        if children.len() > 0 {
            context.push_str("\n  Children: ");
            
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    context.push_str("\n  ");
                }
                
                // Create a reference to the child element
                let child_ref = &child; // Properly borrow the element
                
                // Get child element details and append to context
                let child_context = walk_element_with_children(child_ref, max_depth - 1);
                context.push_str(&child_context);
            }
        }
    }
    
    context
}

extern "C" fn observer_callback(
    _observer: AXObserverRef,
    element_ref: AXUIElementRef,
    notification_ref: CFStringRef,
    _info_ref: CFDictionaryRef,
    _refcon: *mut c_void,
) {
    unsafe {
        // Standard setup
        AXUIElementSetMessagingTimeout(element_ref, 5.0);

        // Skip all notifications while clipboard extraction is in progress
        if OBSERVER_PAUSED.load(Ordering::SeqCst) {
            return;
        }

        // Get notification type
        let notification = CFString::wrap_under_get_rule(notification_ref).to_string();
        
        // Handle structural change notifications - mark tree as dirty and return early
        if notification == "AXChildrenChanged" || 
           notification == "AXFocusedWindowChanged" ||
           notification == "AXLayoutChanged" {
            // Mark that the UI structure has changed and needs refresh
            UI_TREE_DIRTY.store(true, Ordering::SeqCst);
            debug!("UI structural change detected: {} - marking tree as dirty", notification);
            return; // Don't process these as user actions
        }
        
        if notification.contains("AXUIElementDestroyed") || 
           notification.contains("AXAnnouncementRequested") ||
           notification.contains("AXCreated") {
            return; // Skip element destruction, announcement, and creation events
        }
        
        
        // Get basic element info
        let element = AXUIElement::wrap_under_get_rule(element_ref);
        let role = element
            .attribute(&AXAttribute::role())
            .map(|r| format!("{:?}", r))
            .unwrap_or_else(|_| "Unknown".to_string());
            
        // Skip unimportant elements
        if role == "Unknown" || role.contains("AXHelpTag") {
            return;
        }
        
        // Get app and window info directly from the element's ancestry
        let (window_title, window_app_name, app_pid) = get_app_info_from_element(&element);

        // Never observe our own UI — pasting into Linefox's text fields would
        // trigger a full accessibility tree crawl on every keystroke/paste.
        if window_app_name.contains("Linefox") || window_app_name.contains("linefox") {
            return;
        }
        
        // Get value from the element
        let mut value = String::new();
        if let Ok(val) = element.attribute(&AXAttribute::value()) {
            value = extract_text_content(&format!("{:?}", val));
            
            // Skip CFNumber values - they don't provide useful information
            if value.contains("CFNumber") || value.contains("0x") {
                return;
            }
        }
        
        // Get helpful description - MOVED UP before it's used
        let description = match element.attribute(&AXAttribute::description()) {
            Ok(desc) => format!("{:?}", desc),
            Err(_) => String::new(),
        };
        
        // Get additional properties for better context
        let identifier = match element.attribute(&AXAttribute::identifier()) {
            Ok(id) => format!("{:?}", id),
            Err(_) => String::new(),
        };
        
        let title = match element.attribute(&AXAttribute::title()) {
            Ok(t) => format!("{:?}", t),
            Err(_) => String::new(),
        };
        
        let subrole = match element.attribute(&AXAttribute::subrole()) {
            Ok(s) => format!("{:?}", s),
            Err(_) => String::new(),
        };
        
        // Log the timestamp
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S.%3f").to_string();
        
        // Format the action entry - removed timestamp from the format
        let action_entry = format!(
            "📝 Action: {} on {} {}{}{}{}{} (Window: {}, App: {})\n",
            notification,
            role,
            if !value.is_empty() { format!("Value: \"{}\"", value) } else { String::new() },
            if !description.is_empty() { format!(" Description: \"{}\"", description) } else { String::new() },
            if !identifier.is_empty() { format!(" ID: \"{}\"", identifier) } else { String::new() },
            if !title.is_empty() { format!(" Title: \"{}\"", title) } else { String::new() },
            if !subrole.is_empty() { format!(" Subrole: \"{}\"", subrole) } else { String::new() },
            window_title,
            window_app_name
        );
        
        // Create current action info
        let current_action = ActionInfo {
            timestamp: timestamp.clone(),
            notification: notification.clone(),
            role: role.clone(),
            value: value.clone(),
            description: description.clone(),
            identifier: identifier.clone(),
            title: title.clone(),
            subrole: subrole.clone(),
            window_title: window_title.clone(),
            window_app_name: window_app_name.clone(),
            action_entry: action_entry.clone(),
        };
        
        // Determine if this is a typing event (focusing on text input)
        // Fixed to handle quoted format like "AXTextField" or "AXTextArea"
        let is_typing_event = notification.contains("AXValueChanged") &&
                             (role.contains("TextArea") || role.contains("TextField"));

        // Process the action based on its type and context
        {
            let mut pending_action = PENDING_ACTION.lock().unwrap();
            let mut last_action = LAST_ACTION.lock().unwrap();
            
            // For all events (typing or not), check for exact duplicates first
            // This will catch rapid identical events like in the example
            if *last_action != ActionInfo::default() && 
               current_action.notification == last_action.notification &&
               current_action.role == last_action.role &&
               current_action.value == last_action.value &&
               current_action.window_title == last_action.window_title &&
               current_action.description == last_action.description {
                // Skip exact duplicates regardless of type
                return;
            }
            
            // Check if we need to flush a pending action
            // If the current action is different from our pending action's context
            let should_flush_pending = match &*pending_action {
                Some(pa) => {
                    // Flush if this is a different control or different notification type
                    pa.role != current_action.role ||
                    pa.notification != current_action.notification ||
                    pa.window_title != current_action.window_title ||
                    pa.window_app_name != current_action.window_app_name
                },
                None => false
            };
            
            // Flush pending action if needed
            if should_flush_pending {
                if let Some(pa) = pending_action.take() {
                    // Store the final state of the last typing sequence
                    record_action(&pa.action_entry, &pa.window_title, &pa.window_app_name, 0); // Use default pid for now
                    // Update last action
                    *last_action = pa;
                }
            }
            
            // If this is a typing event, we want to accumulate it
            if is_typing_event {
                // Update or create pending action
                *pending_action = Some(current_action);
                return; // Don't record yet - wait for typing sequence to complete
            }
            
            // Record this non-typing event
            record_action(&action_entry, &window_title, &window_app_name, app_pid);
            // Update last action
            *last_action = current_action;
        }
    }
}

// Update the log output to show timestamp separately
fn record_action(action_entry: &str, window_title: &str, window_app_name: &str, _pid: i32) {
    // Store the action for retrieval
    if let Ok(mut actions) = DETECTED_ACTIONS.lock() {
        actions.push_str(action_entry);
    }
    
    // Log the action with timestamp
    let _timestamp = Local::now().format("%Y-%m-%d %H:%M:%S.%3f").to_string();
 // info!("👤 User action: [{}] {}", _timestamp, action_entry.trim());
    
    // Save to DB if recording is enabled
    if is_recording_enabled() {
        save_action_to_db(action_entry.to_string(), window_title.to_string(), window_app_name.to_string());
    }
}

// Helper function to flush any pending actions
// Call this when recording stops or application changes
pub fn flush_pending_actions() {
    info!("🚿 Flushing pending actions...");
    if let Ok(mut pending_action) = PENDING_ACTION.lock() {
        if let Some(pa) = pending_action.take() {
            info!("🚿 Found pending action to flush: {}", pa.action_entry);
            // Only record if we're still recording (avoid deadlock during stop)
            if is_recording_enabled() {
                // Record the final action state
                record_action(&pa.action_entry, &pa.window_title, &pa.window_app_name, 0); // Use default pid for now
            } else {
                info!("🚿 Skipping record_action as recording is disabled");
            }
            
            // Also update last action
            if let Ok(mut last_action) = LAST_ACTION.lock() {
                *last_action = pa;
            }
        } else {
            info!("🚿 No pending actions to flush");
        }
    } else {
        error!("🚿 Failed to acquire PENDING_ACTION lock");
    }
    info!("🚿 Flush complete");
}

// Helper function to extract text content from accessibility values
fn extract_text_content(value_str: &str) -> String {
    if value_str.contains("contents = ") {
        if let Some(start_idx) = value_str.find("contents = ") {
            let start = start_idx + 11;
            let end = value_str.len().saturating_sub(3);
            if start < end {
                let mut text = value_str[start..end].to_string();
                text = text.replace("\\\"", "\"").replace("\\\\", "\\");
                return text;
            }
        }
    } else if value_str.contains("AXValue = ") {
        if let Some(start_idx) = value_str.find("AXValue = ") {
            let start = start_idx + 10;
            let end = value_str.len().saturating_sub(3);
            if start < end {
                let mut text = value_str[start..end].to_string();
                text = text.replace("\\\"", "\"").replace("\\\\", "\\");
                return text;
            }
        }
    } else if value_str.starts_with("\"") && value_str.ends_with("\"") && value_str.len() > 2 {
        // Simple quoted string
        let mut text = value_str[1..value_str.len()-1].to_string();
        text = text.replace("\\\"", "\"").replace("\\\\", "\\");
        return text;
    }
    
    // Default: clean up the value string
    value_str.trim_matches(|c| c == '"' || c == '(' || c == ')').to_string()
}

pub unsafe fn prompt_for_accessibility_permissions() {
    let options = CFDictionary::from_CFType_pairs(&[(
        cf_string_ref_to_cf_string_safe(kAXTrustedCheckOptionPrompt),
        CFBoolean::true_value(),
    )]);

    let trusted = unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };

    if !trusted {
        info!("User did not grant accessibility permissions.");
    } else {
        info!("Accessibility permissions granted.");
    }
}

pub fn check_accessibility_permissions() -> bool {
    let options = CFDictionary::from_CFType_pairs(&[(
        unsafe { cf_string_ref_to_cf_string_safe(kAXTrustedCheckOptionPrompt) },
        CFBoolean::false_value(),
    )]);

    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

fn cf_string_ref_to_cf_string_safe(cf_string_ref: CFStringRef) -> CFString {
    if cf_string_ref.is_null() {
        CFString::new("")
    } else {
        unsafe { CFString::wrap_under_create_rule(cf_string_ref) }
    }
}

// Get current element tree or "Same" based on window change or time interval
#[allow(dead_code)]
fn get_current_element_tree(window_title: &str, window_app_name: String) -> (String, bool) {
    let mut window_changed = false;
    let mut time_threshold_reached = false;
    let mut app_changed = false;
    
    // Check if window title changed
    {
        let mut last_title = LAST_WINDOW_NAME.lock().unwrap();
        if *last_title != *window_title {
            *last_title = window_title.to_string();
            window_changed = true;
            info!("🔄 Window title changed to: {}", window_title);
        }
    }

    // Check if app changed - stronger check for app switching 
    {
        static APP_NAME: Mutex<String> = Mutex::new(String::new());
        
        let mut last_app = APP_NAME.lock().unwrap();
        if *last_app != window_app_name {
            *last_app = window_app_name.clone();
            app_changed = true;
            info!("🔄 Application changed to: {}", window_app_name);
        }
    }
    
    // Check if 5 seconds elapsed since last capture
    {
        let mut last_capture_time = LAST_ELEMENT_TREE_TIME.lock().unwrap();
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(*last_capture_time);
        
        if elapsed >= std::time::Duration::from_secs(5) {
            *last_capture_time = now;
            time_threshold_reached = true;
            info!("⏱️ Time threshold reached for element tree capture");
        }
    }
    
    // Only capture a new tree if 5 seconds have passed, regardless of app/window changes
    // This prevents excessive memory usage from frequent captures
    if time_threshold_reached {
        info!("🌳 Capturing element tree - 5 second threshold reached");
        if app_changed {
            info!("   Note: App changed to {} but waiting for time threshold", window_app_name);
        }
        if window_changed {
            info!("   Note: Window changed to {} but waiting for time threshold", window_title);
        }
        // Get active window PID (safe_get_active_window pre-checks for nil frontmostApplication)
        let pid = match safe_get_active_window() {
            Ok(window) => window.process_id.to_string(),
            _ => "0".to_string(),
        };
        
        // Get new element tree using the same function as automation_agent_engine
        let (element_tree, _) = crate::window_details_collector::window_details_collector::get_element_tree_by_window_app_name(&pid);
        
        // Update the stored element tree
        {
            let mut last_tree = LAST_ELEMENT_TREE.lock().unwrap();
            
            // Only update the stored element tree if it's different or the app changed
            if *last_tree != element_tree || app_changed {
                info!("📝 Captured new element tree ({})", 
                     if app_changed { "app changed" } 
                     else if window_changed { "window changed" } 
                     else { "time threshold" });
                *last_tree = element_tree.clone();
                return (element_tree, true);
            } else {
                // Content is the same despite the trigger - still use "Same" for efficiency
                info!("📝 Element tree unchanged despite trigger - marking as Same");
                return ("Same".to_string(), false);
            }
        }
    } else {
        // No change triggers - return "Same" for efficiency
        if app_changed || window_changed {
            info!("⏳ Skipping element tree capture - waiting for 5s threshold (app/window changed but timer not reached)");
        }
        return ("Same".to_string(), false);
    }
}

// New function to save actions directly to DB
fn save_action_to_db(action_entry: String, window_title: String, window_app_name: String) {
    // Check if recording is enabled
    if !is_recording_enabled() {
        return;
    }

    // Get the app handle
    let app_handle = match APP_HANDLE.lock() {
        Ok(handle) => {
            if let Some(h) = handle.clone() {
                h
            } else {
                info!("❌ Cannot save action - app handle not set");
                return;
            }
        },
        Err(_) => {
            info!("❌ Cannot save action - failed to acquire app handle lock");
            return;
        }
    };
    
    // Get the current element tree or "Same" if no change
    // COMMENTED OUT: Temporarily disabled element tree capture
    // let (element_tree, _) = get_current_element_tree(&window_title, window_app_name.clone());
    let element_tree = String::new(); // Keep blank for now
    
    // Create a timestamp
    let timestamp = Local::now().to_rfc3339();
    
    // Get the current automation ID if we're recording for an automation
    let automation_id = match CURRENT_AUTOMATION_ID.lock() {
        Ok(id) => id.clone(),
        Err(_) => {
            info!("⚠️ Failed to get current automation ID lock, defaulting to None");
            None
        }
    };
    
    // Create the activity item with required fields for direct saving
    let activity_item = crate::entity::activity_item::ActivityItem {
        timestamp,
        ocr_text: String::new(),
        full_activity_text: String::new(),
        editing_mode: String::new(),
        original_ocr_text: String::new(),
        window_title: window_title.clone(),
        window_app_name: window_app_name.clone(),
        user_id: "default".to_string(),
        os_details: format!("macOS"),
        similarity_percentage_to_previous_ocr_text: "0.0".to_string(),
        interval_length: 0,
        keypress_count: 0,
        element_tree_dump: element_tree.clone(),
        detected_actions: action_entry,
        automation_id,
    };
    
    // Save to database directly
    if let Err(e) = app_handle.db(|db| {
        crate::repository::activity_log_repository::save_activity_item(&activity_item, db)
    }) {
        info!("❌ Failed to save activity item directly: {:?}", e);
        return;
    }
    
    info!("✅ Saved activity directly to database for {}{}", 
        window_app_name,
        if automation_id.is_some() { 
            format!(" (automation ID: {})", automation_id.unwrap()) 
        } else { 
            String::new() 
        }
    );
}

// Helper function to get app and window info from an element
fn get_app_info_from_element(element: &AXUIElement) -> (String, String, i32) {
    // Start with defaults in case we can't get proper information
    let mut window_title = "Unknown".to_string();
    let mut app_name = "Unknown".to_string();
    let mut pid = 0;
    
    // Get application info from active window
    if let Ok(active_window) = safe_get_active_window() {
        pid = active_window.process_id as i32; // Convert u64 to i32
        app_name = active_window.app_name.clone();
    }
    
    // Try to find window title by walking up hierarchy
    let current_element = element.clone();
    let mut found_window = false;
    let _max_depth = 10; // Limit how far up we'll search to prevent infinite loops
    
    // Try to get role of current element
    if let Ok(role) = current_element.attribute(&AXAttribute::role()) {
        let role_str = format!("{:?}", role);
        
        // Check if this is a window element
        if role_str.contains("Window") {
            // Get window title
            if let Ok(title) = current_element.attribute(&AXAttribute::title()) {
                window_title = extract_text_content(&format!("{:?}", title));
                found_window = true;
            }
        }
    }
    
    // Note: We've simplified the parent element walk-up since
    // the accessibility API doesn't provide a clear way to walk
    // up the element hierarchy without more complex code
    
    // If we still don't have window title, fall back to active window
    if !found_window {
        if let Ok(active_window) = safe_get_active_window() {
            window_title = if active_window.title.trim().is_empty() { 
                active_window.app_name.clone() 
            } else { 
                active_window.title 
            };
            
            // Log that we had to fall back
         //   info!("⚠️ Had to fall back to active window for title");
        }
    }
    
    (window_title, app_name, pid)
}

/// Clear the observed PIDs (useful for cleanup when apps are closed)
pub fn clear_observed_pids() {
    let mut observed_pids = OBSERVED_PIDS.lock().unwrap();
    let count = observed_pids.len();
    observed_pids.clear();
    info!("🧹 Cleared {} observed PIDs from tracking", count);
}

/// Clear a specific PID from observed tracking (useful when an app is closed)
pub fn clear_observed_pid(pid: &str) {
    if let Ok(pid_num) = pid.parse::<i32>() {
        let mut observed_pids = OBSERVED_PIDS.lock().unwrap();
        if observed_pids.remove(&pid_num) {
            info!("🧹 Removed PID {} from observed tracking", pid_num);
        }
    }
}

/// Check if the UI tree needs refresh due to structural changes
/// Returns true if refresh is needed and resets the flag
pub fn check_and_reset_ui_tree_dirty() -> bool {
    let was_dirty = UI_TREE_DIRTY.swap(false, Ordering::SeqCst);
    if was_dirty {
        info!("🔄 UI tree was marked dirty - structural changes detected");
    }
    was_dirty
}

