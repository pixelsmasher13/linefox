#![cfg(any(target_os = "macos"))]

use serde::{Serialize, Deserialize};
use accessibility::AXAttribute;
use accessibility::AXUIElement;
use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use log::{info, warn, error, debug};
use rand::Rng;
use std::time::{Instant, Duration};
use accessibility_sys::AXUIElementSetAttributeValue;
use accessibility_sys::AXUIElementPerformAction;

/// Represents an actionable UI element in a structured way with hierarchical context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableElement {
    /// The element's role (button, checkbox, etc.)
    pub role: String,
    /// The element's subrole (if any)
    pub subrole: String,
    /// The element's title or label text
    pub title: String,
    /// The element's description
    pub description: String,
    /// The element's identifier (if available)
    pub identifier: String,
    /// The current value of the element
    pub value: String,
    /// The help text for the element
    pub help: String,
    /// Whether the element is enabled
    pub enabled: bool,
    /// The window title that contains this element
    pub window_title: String,
    /// A clear path to target this element
    pub path: String,
    /// Contextual information from parent headers/groups (e.g., "Machine Learning Engineers")
    pub group_context: String,
    /// Static text at same level as this element (sibling context)
    pub sibling_context: String,
    /// Traversal depth (for identifying elements at the same level)
    pub depth: usize,
}

/// Result of performing an action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub success: bool,
    pub message: String,
}

/// Performance tuning constants for faster scans on complex UIs like Gmail
const SCAN_TIME_BUDGET_MS: u64 = 3500;           // Hard cap per scan across all windows
const MAX_ELEMENTS_PER_SCAN: usize = 2500;        // Cap actionable elements collected
const MAX_TEXT_CHARS_DURING_SCAN: usize = 25000;  // Cap text captured during scan
const MAX_TRAVERSAL_DEPTH: usize = 40;           // Avoid very deep trees
const PRIORITY_EXTRA_DEPTH: usize = 30;          // Extra depth allowance under web containers

// Relaxed limits for FULL_TEXT command - when user explicitly requests all text
const FULL_TEXT_SCAN_TIME_BUDGET_MS: u64 = 6000;  // 6 seconds for comprehensive text scan
const FULL_TEXT_MAX_TRAVERSAL_DEPTH: usize = 60;  // Go deeper to find more text

/// Reorder elements to move browser chrome to the top
/// Finds address bar, back button, or new tab button, then moves all elements
/// at ALL RELEVANT DEPTHS (back button level + address bar level) to the top
fn reorder_browser_chrome(elements: Vec<ActionableElement>) -> Vec<ActionableElement> {
    // Find ALL browser chrome depths (back button, address bar, etc. may be at different depths)
    let mut chrome_depths = std::collections::HashSet::new();

    for e in &elements {
        let is_chrome_element =
            // Address bar or search field
            (e.role == "AXTextField" && (
                e.description.to_lowercase().contains("address") ||
                e.description.to_lowercase().contains("search") ||
                e.title.to_lowercase().contains("address") ||
                e.title.to_lowercase().contains("search")
            )) ||
            // Back button
            (e.role == "AXButton" && e.description.to_lowercase().contains("back")) ||
            // Forward button
            (e.role == "AXButton" && e.description.to_lowercase().contains("forward")) ||
            // Reload/Refresh button
            (e.role == "AXButton" && (
                e.description.to_lowercase().contains("reload") ||
                e.description.to_lowercase().contains("refresh")
            )) ||
            // New Tab button
            (e.role == "AXButton" && e.description.to_lowercase().contains("new tab"));

        if is_chrome_element {
            chrome_depths.insert(e.depth);
        }
    }

    if chrome_depths.is_empty() {
        // No browser chrome found
        return elements;
    }

    // Split into chrome elements (at any chrome depth) vs everything else
    let mut chrome_elements: Vec<ActionableElement> = Vec::new();
    let mut other_elements: Vec<ActionableElement> = Vec::new();

    for elem in elements {
        if chrome_depths.contains(&elem.depth) {
            chrome_elements.push(elem);
        } else {
            other_elements.push(elem);
        }
    }

    info!("Found browser chrome at depths {:?}. Moving {} elements from those levels to top",
          chrome_depths, chrome_elements.len());

    // Reassemble: chrome elements first, then everything else
    let mut result = chrome_elements;
    result.extend(other_elements);
    result
}

/// Fast approximate element count - only traverses a few levels deep
/// This is used for detecting significant DOM changes without expensive full scans
pub fn get_quick_element_count(pid: &str) -> usize {
    // Parse the PID to i32
    let pid = match pid.parse::<i32>() {
        Ok(pid) => pid,
        Err(e) => {
            debug!("Failed to parse PID for quick count: {} - Error: {}", pid, e);
            return 0;
        }
    };

    let application = AXUIElement::application(pid);

    // Get all windows from the application
    let windows: CFArray<AXUIElement> = match application.attribute(&AXAttribute::windows()) {
        Ok(windows) => windows,
        Err(_) => return 0,
    };

    let mut total_count = 0;
    let max_depth = 5; // Only go 5 levels deep for quick count

    for window in windows.iter() {
        total_count += count_elements_fast(&window, 0, max_depth);
    }

    total_count
}

/// Recursive helper to count elements quickly with limited depth
fn count_elements_fast(element: &AXUIElement, current_depth: usize, max_depth: usize) -> usize {
    if current_depth >= max_depth {
        return 0;
    }

    let mut count = 1; // Count this element

    // Get children and count them recursively
    if let Ok(children) = element.attribute(&AXAttribute::children()) {
        let children_array: CFArray<AXUIElement> = children;
        for child in children_array.iter() {
            count += count_elements_fast(&child, current_depth + 1, max_depth);
        }
    }

    count
}

/// Main function to get actionable elements from an app by PID
pub fn get_actionable_elements_by_pid(pid: &str) -> (Vec<ActionableElement>, String, String) {
    info!("Getting actionable elements from application with PID: {}", pid);

    // Parse the PID to i32
    let pid = match pid.parse::<i32>() {
        Ok(pid) => pid,
        Err(e) => {
            info!("❌ Failed to parse PID: {} - Error: {}", pid, e);
            return (Vec::new(), String::new(), String::new());
        }
    };

    // Get the application from the PID
    let application = AXUIElement::application(pid);

    // Scan all windows for actionable elements and text content
    scan_windows_for_actionable_elements_and_text(&application)
}

/// Clipboard-based full text extraction: Escape → Cmd+A → Cmd+C → read clipboard → restore.
/// Pauses the AX observer during keystrokes to prevent notification flood from selection changes.
/// Falls back gracefully if clipboard is empty.
pub fn get_full_text_via_clipboard(pid: &str) -> Result<String, String> {
    use core_graphics::event::{CGEvent, CGEventTapLocation, CGKeyCode, CGEventFlags};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use std::process::Command;
    use crate::window_details_collector::macos::macos_accessibility_engine::OBSERVER_PAUSED;
    use std::sync::atomic::Ordering;

    info!("FULL_TEXT clipboard: Starting extraction for PID {}", pid);

    // Step 1: Save current clipboard
    let original_clipboard = Command::new("pbpaste")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    info!("FULL_TEXT clipboard: Saved {} chars of original clipboard", original_clipboard.len());

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create CGEventSource".to_string())?;

    const KEY_A: CGKeyCode = 0x00;
    const KEY_C: CGKeyCode = 0x08;
    const KEY_ESCAPE: CGKeyCode = 0x35;
    const KEY_RIGHT: CGKeyCode = 0x7C;

    // *** PAUSE observer to prevent AX notification flood from selection changes ***
    OBSERVER_PAUSED.store(true, Ordering::SeqCst);

    // Step 2: Escape (defocus address bar / any focused input)
    let key_down = CGEvent::new_keyboard_event(source.clone(), KEY_ESCAPE, true)
        .map_err(|_| "Failed to create Escape event".to_string())?;
    let key_up = CGEvent::new_keyboard_event(source.clone(), KEY_ESCAPE, false)
        .map_err(|_| "Failed to create Escape event".to_string())?;
    key_down.post(CGEventTapLocation::HID);
    std::thread::sleep(Duration::from_millis(30));
    key_up.post(CGEventTapLocation::HID);
    std::thread::sleep(Duration::from_millis(150));

    // Step 3: Cmd+A (Select All)
    let key_down = CGEvent::new_keyboard_event(source.clone(), KEY_A, true)
        .map_err(|_| "Failed to create Cmd+A event".to_string())?;
    let key_up = CGEvent::new_keyboard_event(source.clone(), KEY_A, false)
        .map_err(|_| "Failed to create Cmd+A event".to_string())?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);
    key_down.post(CGEventTapLocation::HID);
    std::thread::sleep(Duration::from_millis(30));
    key_up.post(CGEventTapLocation::HID);
    std::thread::sleep(Duration::from_millis(200));

    // Step 4: Cmd+C (Copy)
    let key_down = CGEvent::new_keyboard_event(source.clone(), KEY_C, true)
        .map_err(|_| "Failed to create Cmd+C event".to_string())?;
    let key_up = CGEvent::new_keyboard_event(source.clone(), KEY_C, false)
        .map_err(|_| "Failed to create Cmd+C event".to_string())?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);
    key_down.post(CGEventTapLocation::HID);
    std::thread::sleep(Duration::from_millis(30));
    key_up.post(CGEventTapLocation::HID);
    std::thread::sleep(Duration::from_millis(300));

    // Step 5: Read clipboard
    let clipboard_text = Command::new("pbpaste")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .map_err(|e| format!("Failed to read clipboard: {}", e))?;

    info!("FULL_TEXT clipboard: Got {} chars from clipboard", clipboard_text.len());

    // Step 6: Restore original clipboard
    let mut restore_cmd = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn pbcopy: {}", e))?;
    if let Some(stdin) = restore_cmd.stdin.as_mut() {
        use std::io::Write;
        let _ = stdin.write_all(original_clipboard.as_bytes());
    }
    let _ = restore_cmd.wait();

    // Step 7: Deselect with right arrow
    if let Ok(key_down) = CGEvent::new_keyboard_event(source.clone(), KEY_RIGHT, true) {
        if let Ok(key_up) = CGEvent::new_keyboard_event(source.clone(), KEY_RIGHT, false) {
            key_down.post(CGEventTapLocation::HID);
            std::thread::sleep(Duration::from_millis(20));
            key_up.post(CGEventTapLocation::HID);
        }
    }

    // Small delay to let any queued notifications drain
    std::thread::sleep(Duration::from_millis(100));

    // *** RESUME observer ***
    OBSERVER_PAUSED.store(false, Ordering::SeqCst);

    if clipboard_text.trim().is_empty() {
        return Err("Clipboard extraction returned empty content".to_string());
    }

    Ok(clipboard_text)
}

/// Special function to get FULL text content with relaxed scan limits
/// This performs a dedicated text-focused scan with:
/// - 6 second time budget (vs 3.5s normal)
/// - 60 depth limit (vs 40 normal)
/// - No element count limit (we only care about text)
/// Use this when executor explicitly requests FULL_TEXT command
pub fn get_full_text_content_by_pid(pid: &str) -> String {
    info!("Starting relaxed FULL_TEXT scan for PID: {}", pid);

    let pid = match pid.parse::<i32>() {
        Ok(pid) => pid,
        Err(e) => {
            info!("❌ Failed to parse PID: {} - Error: {}", pid, e);
            return String::new();
        }
    };

    let application = AXUIElement::application(pid);
    let start_time = Instant::now();

    // Get all windows from the application
    let windows: CFArray<AXUIElement> = match application.attribute(&AXAttribute::windows()) {
        Ok(windows) => windows,
        Err(_) => return String::new(),
    };

    let mut full_text = String::new();

    // Process each window with relaxed limits - text only, no element collection
    for window in windows.iter() {
        if start_time.elapsed() > Duration::from_millis(FULL_TEXT_SCAN_TIME_BUDGET_MS) {
            info!("FULL_TEXT scan: time budget exhausted after processing windows");
            break;
        }

        collect_text_only_relaxed(&window, &mut full_text, 0, start_time);
    }

    info!("FULL_TEXT scan complete: {} chars collected in {}ms",
        full_text.len(), start_time.elapsed().as_millis());

    full_text
}

/// Collect text content only (no elements) with relaxed depth/time limits
/// Used by FULL_TEXT command for comprehensive text extraction
fn collect_text_only_relaxed(
    element: &AXUIElement,
    text_content: &mut String,
    depth: usize,
    start_time: Instant,
) {
    // Check time budget
    if start_time.elapsed() > Duration::from_millis(FULL_TEXT_SCAN_TIME_BUDGET_MS) {
        return;
    }

    // Check depth limit (relaxed)
    if depth > FULL_TEXT_MAX_TRAVERSAL_DEPTH {
        return;
    }

    // Get role
    let role = element
        .attribute(&AXAttribute::role())
        .map(|r| clean_attribute_value(&format!("{:?}", r)))
        .unwrap_or_else(|_| String::new());

    // Extract text from text-bearing elements
    if is_text_bearing_element(&role) || role.contains("AXHeading") {
        let mut text_parts = Vec::new();

        // Try value
        if let Ok(value) = element.attribute(&AXAttribute::value()) {
            let v = extract_cfstring_content(&format!("{:?}", value));
            if !v.is_empty() && v.len() > 2 {
                text_parts.push(v);
            }
        }

        // Try title
        if let Ok(title) = element.attribute(&AXAttribute::title()) {
            let t = extract_cfstring_content(&format!("{:?}", title));
            if !t.is_empty() && t.len() > 2 && !text_parts.contains(&t) {
                text_parts.push(t);
            }
        }

        // Try description
        if let Ok(desc) = element.attribute(&AXAttribute::description()) {
            let d = extract_cfstring_content(&format!("{:?}", desc));
            if !d.is_empty() && d.len() > 2 && !text_parts.contains(&d) {
                text_parts.push(d);
            }
        }

        if !text_parts.is_empty() {
            let combined = text_parts.join(" ");
            if !combined.trim().is_empty() {
                text_content.push_str(&combined);
                text_content.push_str(" | ");
            }
        }
    }

    // Recurse into children
    if let Ok(children) = element.attribute(&AXAttribute::children()) {
        for child in children.iter() {
            collect_text_only_relaxed(&child, text_content, depth + 1, start_time);

            // Check time budget in loop
            if start_time.elapsed() > Duration::from_millis(FULL_TEXT_SCAN_TIME_BUDGET_MS) {
                break;
            }
        }
    }
}

/// Find an element, perform a random action, and return the result
pub fn pick_random_element_and_act(pid: &str) -> ActionResult {
    info!("Finding and acting on a random element in application with PID: {}", pid);
    
    // Parse the PID to i32
    let pid = match pid.parse::<i32>() {
        Ok(pid) => pid,
        Err(e) => {
            let msg = format!("Failed to parse PID: {} - Error: {}", pid, e);
            info!("❌ {}", msg);
            return ActionResult {
                success: false,
                message: msg,
            };
        }
    };
    
    // Get the application from the PID
    let application = AXUIElement::application(pid);
    
    // Get all windows from the application
    let windows: CFArray<AXUIElement> = match application.attribute(&AXAttribute::windows()) {
        Ok(windows) => windows,
        Err(e) => {
            let msg = format!("Failed to get windows: {:?}", e);
            warn!("{}", msg);
            return ActionResult {
                success: false,
                message: msg,
            };
        },
    };
    
    if windows.len() == 0 {
        return ActionResult {
            success: false,
            message: "No windows found in application".to_string(),
        };
    }
    
    // Collect all actionable elements with their AXUIElements
    let mut actionable_elements = Vec::new();
    let mut text_content = String::new();
    let mut full_text_content = String::new();

    let start_time = Instant::now();

    for window in windows.iter() {
        // Compute window title once per window
        let window_title = window
            .attribute(&AXAttribute::title())
            .map(|t| clean_attribute_value(&format!("{:?}", t)))
            .unwrap_or_else(|_| "Untitled Window".to_string());

        // Use context-aware collection for better element selection
        collect_actionable_elements_with_context(
            &window,
            &window_title,
            &mut actionable_elements,
            &mut text_content,
            &mut full_text_content,
            start_time,
        );

        if actionable_elements.len() >= MAX_ELEMENTS_PER_SCAN
            || start_time.elapsed() > Duration::from_millis(SCAN_TIME_BUDGET_MS) {
            break;
        }
    }
    
    // Log the number of elements found before filtering
    let total_elements_found = actionable_elements.len();
    info!("Found {} total potential elements before filtering", total_elements_found);
    
    if total_elements_found == 0 {
        return ActionResult {
            success: false,
            message: "No UI elements found (empty accessibility tree)".to_string(),
        };
    }
    
    // Count and log elements rejected due to being disabled
    let disabled_elements = actionable_elements.iter()
        .filter(|(_, element)| !element.enabled)
        .count();
    
    info!("Found {} disabled elements that will be filtered out", disabled_elements);
    
    // Group elements by role for better diagnostics
    let mut element_roles = std::collections::HashMap::new();
    for (_, element) in &actionable_elements {
        *element_roles.entry(element.role.clone()).or_insert(0) += 1;
    }
    
    // Log the element types found
    for (role, count) in &element_roles {
        info!("Found {} elements with role: {}", count, role);
    }
    
    // If many elements are disabled, we'll try to be more permissive
    let filtered_elements = if disabled_elements > total_elements_found / 2 {
        info!("Most elements are reported as disabled - using permissive mode to select elements");
        // In permissive mode, we'll consider all elements regardless of enabled state
        actionable_elements.clone()
    } else {
        // Normal mode - only consider enabled elements
        actionable_elements.iter()
            .filter(|(_, element)| element.enabled)
            .cloned()
            .collect()
    };
    
    if filtered_elements.is_empty() {
        // Build a detailed diagnostic message
        let mut diagnostic = format!("No actionable UI elements found. Diagnostic info:\n");
        diagnostic.push_str(&format!("- Total elements found: {}\n", total_elements_found));
        diagnostic.push_str(&format!("- Disabled elements: {}\n", disabled_elements));
        diagnostic.push_str("- Element roles found: ");
        for (role, count) in &element_roles {
            diagnostic.push_str(&format!("{} ({}), ", role, count));
        }
        
        return ActionResult {
            success: false,
            message: diagnostic,
        };
    }
    
    // Choose a random element
    let mut rng = rand::thread_rng();
    let random_index = rng.gen_range(0..filtered_elements.len());
    let (element_ref, element_info) = &filtered_elements[random_index];
    
    // Try to interact with the element
    info!("Attempting to interact with element: {} - '{}' (enabled: {})", 
          element_info.role, 
          if !element_info.title.is_empty() { &element_info.title } else { &element_info.description },
          element_info.enabled);
    
    // Try to perform an action based on element type
    if element_info.role.contains("AXButton") || 
        element_info.role.contains("AXMenuItem") || 
        element_info.role.contains("AXCheckBox") ||
        element_info.role.contains("AXRadioButton") {
        
        // For button-like elements, use the enhanced multiple action approach
        let element_name = if !element_info.title.is_empty() { 
            &element_info.title 
        } else if !element_info.description.is_empty() { 
            &element_info.description 
        } else { 
            "unnamed" 
        };
        
        return try_multiple_actions(element_ref, element_name, &element_info.role);
    } else if element_info.role.contains("AXTextField") || element_info.role.contains("AXTextArea") {
        // For text fields, try to set a value
        let sample_text = "Hello, this is a test";
        let focus_action = CFString::new("AXFocus");
        let focus_result = element_ref.perform_action(&focus_action);
        
        if focus_result.is_ok() {
            info!("Successfully focused text field, attempting to type");
            
            // Try to set the text value
            let text_value = CFString::new(sample_text);
            
            unsafe {
                use accessibility_sys::AXUIElementSetAttributeValue;
                
                let attr_name = CFString::new("AXValue");
                
                let result = AXUIElementSetAttributeValue(
                    element_ref.as_concrete_TypeRef(),
                    attr_name.as_concrete_TypeRef(),
                    text_value.as_CFTypeRef(),
                );
                
                if result == 0 {
                    let msg = format!("Successfully typed '{}' in text field", sample_text);
                    info!("{}", msg);
                    return ActionResult {
                        success: true,
                        message: msg,
                    };
                } else {
                    let msg = format!("Failed to type in text field: error code {}", result);
                    warn!("{}", msg);
                    return ActionResult {
                        success: false,
                        message: msg,
                    };
                }
            }
        } else {
            let msg = "Failed to focus text field for typing".to_string();
            warn!("{}", msg);
            return ActionResult {
                success: false,
                message: msg,
            };
        }
    } else {
        // For other elements, use the enhanced multiple action approach
        let element_name = if !element_info.title.is_empty() { 
            &element_info.title 
        } else if !element_info.description.is_empty() { 
            &element_info.description 
        } else { 
            "unnamed" 
        };
        
        return try_multiple_actions(element_ref, element_name, &element_info.role);
    }
}

/// Structure to track contextual information while traversing the tree
#[derive(Clone)]
struct ElementContext {
    /// The most recent header/group label encountered (e.g., "Machine Learning Engineers")
    current_header: String,
    /// Collection of sibling static text at current level
    sibling_texts: Vec<String>,
}

/// Enhanced collection that preserves hierarchical context
fn collect_actionable_elements_with_context(
    element: &AXUIElement,
    window_title: &str,
    elements: &mut Vec<(AXUIElement, ActionableElement)>,
    text_content: &mut String,
    full_text_content: &mut String,
    start_time: Instant,
) {
    // Use a stack for DFS instead of queue for BFS
    let mut stack = Vec::new();
    // (element, depth, in_priority_branch, context)
    let initial_context = ElementContext {
        current_header: String::new(),
        sibling_texts: Vec::new(),
    };
stack.push((element.clone(), 0usize, false, initial_context.clone()));

    // Deferred queue for AXWebArea elements - process these AFTER toolbar/chrome elements
    // This ensures browser chrome (Back, Forward, Address bar) gets collected first
    let mut deferred_web_areas: Vec<(AXUIElement, usize, ElementContext)> = Vec::new();

    // Track counters for each role to ensure uniqueness when building identifiers
    let mut role_counters: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    while let Some((current, depth, in_priority_branch, context)) = stack.pop() {

        // Stop early if we exhausted our time/size budgets
        if start_time.elapsed() > Duration::from_millis(SCAN_TIME_BUDGET_MS)
            || elements.len() >= MAX_ELEMENTS_PER_SCAN {
            break;
        }

        let allowed_depth = if in_priority_branch {
            MAX_TRAVERSAL_DEPTH + PRIORITY_EXTRA_DEPTH
        } else {
            MAX_TRAVERSAL_DEPTH
        };

        if depth > allowed_depth {
            continue;
        }

        // Get role and check element type
        let role = current
            .attribute(&AXAttribute::role())
            .map(|r| clean_attribute_value(&format!("{:?}", r)))
            .unwrap_or_else(|_| String::new());

        if role.is_empty() {
            // Still traverse children with same context
            if let Ok(children) = current.attribute(&AXAttribute::children()) {
                // Collect children into a vector to reverse them (for left-to-right DFS order)
                let children_vec: Vec<AXUIElement> = children.iter().map(|c| c.clone()).collect();
                for child in children_vec.into_iter().rev() {
                    stack.push((child, depth + 1, in_priority_branch, context.clone()));
                }
            }
            continue;
        }

        let is_actionable = is_truly_actionable_element(&role);
        let is_header = role.contains("AXHeading");
        let is_static_text = role.contains("AXStaticText");
        let is_table_element = role.contains("AXCell") ||
                               role.contains("AXColumnHeader") ||
                               role.contains("AXRowHeader") ||
                               role.contains("AXGridCell") ||
                               role.contains("AXTable") ||
                               role.contains("AXRow");
        let role_is_priority = is_priority_container(&role);

        // Create new context for children based on current element
        let mut child_context = context.clone();

        // First, process children to analyze what's in this container
        let mut child_elements = Vec::new();
        let mut child_static_texts = Vec::new();
        let mut has_actionable_children = false;

        if let Ok(children) = current.attribute(&AXAttribute::children()) {
            // Scan children to understand the structure
            for child in children.iter() {
                if let Ok(child_role) = child.attribute(&AXAttribute::role()) {
                    let child_role_str = clean_attribute_value(&format!("{:?}", child_role));

                    if is_truly_actionable_element(&child_role_str) {
                        has_actionable_children = true;
                        child_elements.push((child.clone(), child_role_str));
                    } else if child_role_str.contains("AXStaticText") {
                        // Get the text content of static text children
                        if let Ok(value) = child.attribute(&AXAttribute::value()) {
                            let text = extract_cfstring_content(&format!("{:?}", value));
                            if !text.is_empty() && text.len() > 3 {
                                child_static_texts.push(text);
                            }
                        }
                    }
                }
            }
        }

        // Process headers as special elements that should be visible
        if is_header {
            // This is a heading - add it as a special element for display
            if let Some(mut element_info) = create_actionable_element_fast(&current, role.clone(), window_title, String::new(), depth) {
                let header_text = if !element_info.value.is_empty() {
                    element_info.value.clone()
                } else if !element_info.title.is_empty() {
                    element_info.title.clone()
                } else {
                    element_info.description.clone()
                };

                if !header_text.is_empty() && header_text.len() > 3 {
                    // Add COMPLETE header text to full_text_content (only shown when LLM explicitly requests it)
                    // Don't add to text_content since it's already shown in the element list
                    full_text_content.push_str(&header_text);
                    full_text_content.push_str(" | ");

                    // Update the header context for children
                    child_context.current_header = header_text.clone();

                    // Add the heading as a special element with a marker role
                    element_info.role = "HEADING_MARKER".to_string();
                    element_info.group_context = String::new(); // Don't duplicate context
                    element_info.sibling_context = String::new();

                    // Set title/description to the header text for display
                    if element_info.title.is_empty() && element_info.description.is_empty() {
                        element_info.title = header_text.clone();
                    }

                    // Build unique identifier for heading to ensure proper matching
                    let simple_id = make_role_qualified_identifier(&element_info, &mut role_counters);
                    element_info.path = simple_id;

                    // Add to elements list so it will be displayed
                    elements.push((current.clone(), element_info));
                }
            }
        } else if is_static_text {
            // Process all static text elements
            if let Some(mut element_info) = create_actionable_element_fast(&current, role.clone(), window_title, String::new(), depth) {
                let static_text = if !element_info.value.is_empty() {
                    element_info.value.clone()
                } else if !element_info.title.is_empty() {
                    element_info.title.clone()
                } else {
                    element_info.description.clone()
                };

                if !static_text.is_empty() && static_text.len() > 3 {
                    // Add COMPLETE static text to full_text_content (only shown when LLM explicitly requests it)
                    // Don't add to text_content since it's already shown in the element list
                    full_text_content.push_str(&static_text);
                    full_text_content.push_str(" | ");

                    if has_actionable_children {
                        // Static text that precedes actionable elements becomes context
                        child_context.current_header = static_text.clone();
                    } else {
                        // Standalone static text should be visible as a special element (like headers)
                        element_info.role = "STATIC_TEXT_MARKER".to_string();
                        element_info.group_context = context.current_header.clone();
                        element_info.sibling_context = String::new();

                        // Ensure the text is visible in title or description
                        if element_info.title.is_empty() && element_info.description.is_empty() {
                            element_info.title = static_text.clone();
                        }

                        // Build unique identifier for static text to ensure proper matching
                        let simple_id = make_role_qualified_identifier(&element_info, &mut role_counters);
                        element_info.path = simple_id;

                        // Add to elements list so it will be displayed
                        elements.push((current.clone(), element_info));
                    }
                }
            }
        } else if is_table_element && !is_actionable {
            // Process table elements - extract their text content for full_text_content
            // This captures cell values, column/row headers that aren't otherwise visible
            if let Some(element_info) = create_actionable_element_fast(&current, role.clone(), window_title, String::new(), depth) {
                let table_text = if !element_info.value.is_empty() {
                    element_info.value.clone()
                } else if !element_info.title.is_empty() {
                    element_info.title.clone()
                } else {
                    element_info.description.clone()
                };

                if !table_text.is_empty() && table_text.len() > 1 {
                    // Add table content to full_text_content (only shown when LLM explicitly requests it)
                    // Use tab separator for table cells to preserve tabular structure
                    let separator = if role.contains("AXRow") { " | " } else { "\t" };
                    full_text_content.push_str(&table_text);
                    full_text_content.push_str(separator);

                    // If this is a column or row header, also update context for children
                    if role.contains("AXColumnHeader") || role.contains("AXRowHeader") {
                        child_context.current_header = table_text;
                    }
                }
            }
        }

        // Process actionable elements with context
        if is_actionable {
            if let Some(mut element_info) = create_actionable_element_fast(&current, role.clone(), window_title, String::new(), depth) {
                let has_meaningful_content = !element_info.title.is_empty() ||
                                            !element_info.description.is_empty() ||
                                            !element_info.value.is_empty() ||
                                            !element_info.identifier.is_empty();

                if has_meaningful_content || is_high_value_actionable(&role) {
                    let is_empty_group = element_info.role.contains("AXGroup") &&
                                        element_info.title.is_empty() &&
                                        element_info.description.is_empty();

                    if !is_empty_group {
                        // Add group context from parent headers
                        element_info.group_context = context.current_header.clone();

                        // For sibling context, use either:
                        // 1. Static texts passed from parent (siblings at same level)
                        // 2. Static text children of this element
                        let mut sibling_context = if !context.sibling_texts.is_empty() {
                            context.sibling_texts.join(" | ")
                        } else if !child_static_texts.is_empty() {
                            child_static_texts.join(" | ")
                        } else {
                            String::new()
                        };

                        // For checkboxes, append checked state to sibling context
                        if role.contains("AXCheckBox") {
                            let checkbox_state = if element_info.value == "1" {
                                "checked"
                            } else {
                                "unchecked"
                            };

                            if sibling_context.is_empty() {
                                sibling_context = checkbox_state.to_string();
                            } else {
                                sibling_context.push_str(" | ");
                                sibling_context.push_str(checkbox_state);
                            }
                        }

                        element_info.sibling_context = sibling_context;

                        // Build identifier
                        let simple_id = make_role_qualified_identifier(&element_info, &mut role_counters);
                        element_info.path = simple_id;
                        elements.push((current.clone(), element_info));
                    }
                } else if !child_static_texts.is_empty() {
                    // Actionable element has no meaningful content but has static text children
                    // Add those texts to visible text content instead of losing them
                    for text in &child_static_texts {
                        if text_content.len() < MAX_TEXT_CHARS_DURING_SCAN {
                            text_content.push_str(text);
                            text_content.push_str(" | ");
                        }
                    }
                }
            }
        }

        // Add children to queue with updated context
        if let Ok(children) = current.attribute(&AXAttribute::children()) {
            let next_priority = in_priority_branch || role_is_priority;

            // Build sibling texts and check for sibling headers
            let mut sibling_texts_for_children = Vec::new();
            let mut sibling_header = String::new();

            // First pass: look for headers and static text among children
            for child in children.iter() {
                if let Ok(child_role) = child.attribute(&AXAttribute::role()) {
                    let child_role_str = clean_attribute_value(&format!("{:?}", child_role));

                    // Check if this is a heading at the same level as other elements
                    if child_role_str.contains("AXHeading") {
                        if let Some(header_info) = create_actionable_element_fast(&child, child_role_str.clone(), window_title, String::new(), depth + 1) {
                            let header_text = if !header_info.value.is_empty() {
                                header_info.value
                            } else if !header_info.title.is_empty() {
                                header_info.title
                            } else {
                                header_info.description
                            };

                            if !header_text.is_empty() && header_text.len() > 3 {
                                // This heading will be the group context for its siblings
                                sibling_header = header_text;
                                // Break after finding first heading to use it as the group header
                                break;
                            }
                        }
                    }
                }
            }

            // Second pass: collect ALL static texts (but not headers) - we want them all
            for child in children.iter() {
                if let Ok(child_role) = child.attribute(&AXAttribute::role()) {
                    let child_role_str = clean_attribute_value(&format!("{:?}", child_role));
                    // Only collect non-header static text for sibling context
                    if child_role_str.contains("AXStaticText") && !child_role_str.contains("AXHeading") {
                        // Try multiple attributes to get the text
                        let mut text = String::new();

                        // Try value first
                        if let Ok(value) = child.attribute(&AXAttribute::value()) {
                            text = extract_cfstring_content(&format!("{:?}", value));
                        }

                        // If value is empty, try title
                        if text.is_empty() {
                            if let Ok(title) = child.attribute(&AXAttribute::title()) {
                                text = extract_cfstring_content(&format!("{:?}", title));
                            }
                        }

                        // If still empty, try description
                        if text.is_empty() {
                            if let Ok(desc) = child.attribute(&AXAttribute::description()) {
                                text = extract_cfstring_content(&format!("{:?}", desc));
                            }
                        }

                        if !text.is_empty() && text.len() > 2 {  // Lower threshold to catch more text
                            sibling_texts_for_children.push(text);
                        }
                    }
                }
            }

            // If we found a sibling header, use it as the group context for all siblings
            if !sibling_header.is_empty() {
                child_context.current_header = sibling_header;
            }

            // Pass the sibling context to all children
            child_context.sibling_texts = sibling_texts_for_children;

            // For DFS, collect and reverse children to maintain left-to-right order
            let children_vec: Vec<AXUIElement> = children.iter().map(|c| c.clone()).collect();
            for child in children_vec.into_iter().rev() {
  // Check if this child is an AXWebArea - defer it to process after chrome elements
                let child_role = child
                    .attribute(&AXAttribute::role())
                    .map(|r| clean_attribute_value(&format!("{:?}", r)))
                    .unwrap_or_else(|_| String::new());

                if child_role.contains("AXWebArea") {
                    // Defer web areas to ensure browser chrome gets collected first
                    deferred_web_areas.push((child, depth + 1, child_context.clone()));
                } else {
                    stack.push((child, depth + 1, next_priority, child_context.clone()));
                }
            }
        }

        // When stack is empty but we have deferred web areas, push them to continue traversal
        // This ensures chrome elements are collected first, then web content
        if stack.is_empty() && !deferred_web_areas.is_empty() {
            info!("Processing {} deferred AXWebArea elements after chrome collection ({} elements so far)",
                  deferred_web_areas.len(), elements.len());
            for (web_area, web_depth, web_context) in deferred_web_areas.drain(..) {
                // Push as priority branch so we go deep into web content
                stack.push((web_area, web_depth, true, web_context));            }
        }
    }
}

/// Collect actionable elements and text content from a window or element (legacy version)
#[allow(dead_code)]
fn collect_actionable_elements_and_text(
    element: &AXUIElement,
    window_title: &str,
    elements: &mut Vec<(AXUIElement, ActionableElement)>,
    text_content: &mut String,
    start_time: Instant,
) {
    // Use a stack for DFS instead of queue for BFS
    let mut stack = Vec::new();
    // (element, depth, in_priority_branch)
    stack.push((element.clone(), 0usize, false));

    // Track counters for each role to ensure uniqueness when building identifiers
    let mut role_counters: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    while let Some((current, depth, in_priority_branch)) = stack.pop() {
        // Stop early if we exhausted our time/size budgets (do not stop solely due to text cap)
        if start_time.elapsed() > Duration::from_millis(SCAN_TIME_BUDGET_MS)
            || elements.len() >= MAX_ELEMENTS_PER_SCAN {
            break;
        }

        let allowed_depth = if in_priority_branch {
            MAX_TRAVERSAL_DEPTH + PRIORITY_EXTRA_DEPTH
        } else {
            MAX_TRAVERSAL_DEPTH
        };

        if depth > allowed_depth {
            continue;
        }

        // Fetch role first (cheap) and use it to decide whether to fetch more expensive attributes
        let role = current
            .attribute(&AXAttribute::role())
            .map(|r| clean_attribute_value(&format!("{:?}", r)))
            .unwrap_or_else(|_| String::new());

        if role.is_empty() {
            // Still traverse children
        } else {
            let is_actionable = is_truly_actionable_element(&role);
            let _is_text_bearing = is_text_bearing_element(&role);
            let role_is_priority = is_priority_container(&role);

            // Process actionable elements separately (they go into elements list but NOT combined_text)
            if is_actionable {
                if let Some(mut element_info) = create_actionable_element_fast(&current, role.clone(), window_title, String::new(), depth) {
                    let has_meaningful_content = !element_info.title.is_empty() ||
                                                !element_info.description.is_empty() ||
                                                !element_info.value.is_empty() ||
                                                !element_info.identifier.is_empty();

                    // Always include high-value actionable controls even without labels
                    if has_meaningful_content || is_high_value_actionable(&role) {
                        // Filter out AXGroup elements with no meaningful content (structural containers)
                        let is_empty_group = element_info.role.contains("AXGroup") &&
                                            element_info.title.is_empty() &&
                                            element_info.description.is_empty();

                        if !is_empty_group {
                            // Build a simple, role-qualified identifier without extra AX calls
                            let simple_id = make_role_qualified_identifier(&element_info, &mut role_counters);
                            element_info.path = simple_id;
                            elements.push((current.clone(), element_info));
                        }
                    }
                }
            } else {
                // Non-actionable elements - include their text in combined_text
                if let Some(element_info) = create_actionable_element_fast(&current, role.clone(), window_title, String::new(), depth) {
                    // Extract text content from non-actionable elements for combined_text
                    let mut text_parts = Vec::new();
                    if !element_info.value.is_empty() { text_parts.push(element_info.value.clone()); }
                    if !element_info.title.is_empty() { text_parts.push(element_info.title.clone()); }
                    // Skip description if it's identical to value to avoid redundancy
                    if !element_info.description.is_empty() && element_info.description != element_info.value {
                        text_parts.push(element_info.description);
                    }

                    if !text_parts.is_empty() {
                        let combined_text = text_parts.join(" ");
                        if combined_text.len() > 3 && !combined_text.trim().is_empty() {
                            // Append but respect MAX_TEXT_CHARS_DURING_SCAN
                            let remaining = MAX_TEXT_CHARS_DURING_SCAN.saturating_sub(text_content.len());
                            if remaining > 0 {
                                if combined_text.len() <= remaining {
                                    text_content.push_str(&combined_text);
                                } else {
                                    let mut char_boundary = remaining;
                                    while !combined_text.is_char_boundary(char_boundary) && char_boundary > 0 {
                                        char_boundary -= 1;
                                    }
                                    if char_boundary > 0 {
                                        text_content.push_str(&combined_text[..char_boundary]);
                                    }
                                }
                                // Add separator between elements for better LLM comprehension
                                text_content.push_str(" | ");
                            }
                        }
                    }
                }
            }

            // Add children to the stack if within depth limit
            if let Ok(children) = current.attribute(&AXAttribute::children()) {
                let next_priority = in_priority_branch || role_is_priority;
                // Collect and reverse to maintain left-to-right order in DFS
                let children_vec: Vec<AXUIElement> = children.iter().map(|c| c.clone()).collect();
                for child in children_vec.into_iter().rev() {
                    stack.push((child, depth + 1, next_priority));
                }
                // Continue to next loop iteration since we handled children here
                continue;
            }
        }
    }
}

/// Check if an element has a press action
#[allow(dead_code)]
fn has_press_action(element: &AXUIElement) -> bool {
    // Create the press action name
    let _press_action = CFString::new("AXPress");
    
    // Try to get the actions this element supports
    // In the accessibility API, there's no direct method to check if an action
    // is supported without trying to perform it.
    // As a workaround, we'll try to perform the action but in a safe way
    
    // First, check if the element is likely to be pressable based on role
    if let Ok(role) = element.attribute(&AXAttribute::role()) {
        let role_str = format!("{:?}", role);
        if role_str.contains("AXButton") || 
           role_str.contains("AXMenuItem") ||
           role_str.contains("AXCheckBox") ||
           role_str.contains("AXRadioButton") {
            return true;
        }
    }
    
    // For other elements, we'll have to be more conservative
    false
}

/// Try multiple accessibility actions on an element for better compatibility
fn try_multiple_actions(element: &AXUIElement, element_name: &str, element_role: &str) -> ActionResult {
    // Define actions to try in order of preference
    let actions_to_try = [
        ("AXPress", "press"),       // Most common action
        ("AXClick", "click"),       // Alternative click action 
        ("AXConfirm", "confirm"),   // For confirmation elements
        ("AXPick", "pick"),         // For selection elements
        ("AXFocus", "focus"),       // Fallback to just focus
        ("AXShowMenu", "show menu") // For menu elements
    ];
    
    // Try each action in sequence
    for (action_name, action_desc) in actions_to_try.iter() {
        let action = CFString::new(action_name);
        
        match element.perform_action(&action) {
            Ok(_) => {
                let msg = format!("Successfully {} {} '{}' using {}", 
                                 action_desc, element_role, element_name, action_name);
                info!("{}", msg);
                return ActionResult {
                    success: true,
                    message: msg,
                };
            },
            Err(e) => {
                debug!("Action {} failed on {} '{}': {:?}", action_name, element_role, element_name, e);
                // Continue to next action
            }
        }
    }
    
    // If all actions failed, return failure with diagnostic info
    let msg = format!("Failed to interact with {} '{}': All accessibility actions failed. This may be due to app security restrictions or unsupported element type.", element_role, element_name);
    warn!("{}", msg);
    ActionResult {
        success: false,
        message: msg,
    }
}

/// Find a sibling AXStaticText element that likely serves as a label for the given element
fn find_sibling_text_label(element: &AXUIElement) -> Option<String> {
    // Get the parent element
    let parent = match element.attribute(&AXAttribute::parent()) {
        Ok(parent) => parent,
        Err(e) => {
            debug!("Could not get parent for element: {:?}", e);
            return None;
        }
    };

    // Get all children of the parent (siblings of our element)
    let children: CFArray<AXUIElement> = match parent.attribute(&AXAttribute::children()) {
        Ok(children) => children,
        Err(e) => {
            debug!("Could not get siblings for element: {:?}", e);
            return None;
        }
    };

    // Look through siblings for AXStaticText elements
    let mut found_checkbox = false;
    let mut text_labels = Vec::new();

    for child in children.iter() {
        // Get the role of this sibling
        let role = child
            .attribute(&AXAttribute::role())
            .map(|r| clean_attribute_value(&format!("{:?}", r)))
            .unwrap_or_else(|_| String::new());

        // Check if this is our checkbox or radio button (by comparing references)
        // We can't directly compare AXUIElement references, so we'll use a heuristic
        if role.contains("AXCheckBox") || role.contains("AXRadioButton") {
            // Check if this is likely our element by comparing some attributes
            let child_title = child
                .attribute(&AXAttribute::title())
                .map(|t| format!("{:?}", t))
                .unwrap_or_default();
            let element_title = element
                .attribute(&AXAttribute::title())
                .map(|t| format!("{:?}", t))
                .unwrap_or_default();

            if child_title == element_title {
                found_checkbox = true;
                // Clear any text labels found before the checkbox
                // We want text that comes after or very close to the checkbox
                if text_labels.is_empty() {
                    // Continue searching for text after the checkbox
                    continue;
                }
            }
        }

        // If this is static text, capture it
        if role.contains("AXStaticText") {
            // Get the text content
            let mut label_text = String::new();

            // Try to get value first (most common for static text)
            if let Ok(value) = child.attribute(&AXAttribute::value()) {
                let value_str = format!("{:?}", value);
                let extracted = extract_cfstring_content(&value_str);
                if !extracted.is_empty() {
                    label_text = extracted;
                }
            }

            // If no value, try title
            if label_text.is_empty() {
                if let Ok(title) = child.attribute(&AXAttribute::title()) {
                    let title_str = format!("{:?}", title);
                    let extracted = extract_cfstring_content(&title_str);
                    if !extracted.is_empty() {
                        label_text = extracted;
                    }
                }
            }

            // If we found text, add it to our collection
            if !label_text.is_empty() {
                text_labels.push(label_text);

                // If we already found our checkbox and now found text,
                // this is likely the label - return it immediately
                if found_checkbox {
                    return Some(text_labels.last().unwrap().clone());
                }
            }
        }

        // If we found our checkbox and the next element isn't text,
        // but we have previously collected text, use the most recent one
        if found_checkbox && !role.contains("AXStaticText") && !text_labels.is_empty() {
            // The last text element before or right after the checkbox is likely the label
            return Some(text_labels.last().unwrap().clone());
        }
    }

    // If we collected any text labels near the checkbox, return the most relevant one
    if !text_labels.is_empty() {
        // Return the first text label found (assuming it's the most relevant)
        return Some(text_labels[0].clone());
    }

    None
}

/// Extract actual content from CFString debug output, returns empty string if no meaningful content
fn extract_cfstring_content(debug_string: &str) -> String {
    // Look for contents = "..." pattern
    if let Some(start) = debug_string.find("contents = \"") {
        let content_start = start + 12; // Length of "contents = \""
        if let Some(end) = debug_string[content_start..].find("\"") {
            let content = &debug_string[content_start..content_start + end];
            return content.to_string();
        }
    }
    // If no contents pattern found, return empty string
    String::new()
}

/// Extract numeric value from CFNumber debug output
fn extract_cfnumber_value(debug_string: &str) -> String {
    // Look for pattern like "value = +5000.0000000000" or "value = 5000"
    if let Some(start) = debug_string.find("value = ") {
        let value_start = start + 8; // Length of "value = "
        // Find the end of the number (comma or closing brace)
        if let Some(end_idx) = debug_string[value_start..].find(',')
            .or_else(|| debug_string[value_start..].find('}')) {

            let value_str = &debug_string[value_start..value_start + end_idx];
            // Clean up the number (remove + sign, convert to integer if it's a whole number)
            let cleaned = value_str.trim_start_matches('+').trim();

            // Try to parse as float and format nicely
            if let Ok(float_val) = cleaned.parse::<f64>() {
                // If it's a whole number, format without decimals
                if float_val.fract() == 0.0 {
                    return format!("{}", float_val as i64);
                } else {
                    // Otherwise, format with reasonable precision
                    return format!("{:.2}", float_val);
                }
            } else {
                return cleaned.to_string();
            }
        }
    }

    // Fallback: if the string is just a number already, return it
    if debug_string.parse::<f64>().is_ok() {
        return debug_string.to_string();
    }

    String::new()
}

/// Create an ActionableElement from an AXUIElement
fn create_actionable_element(element: &AXUIElement, path: String) -> Option<ActionableElement> {
    // Get basic element properties
    let role = element
        .attribute(&AXAttribute::role())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    // Only fetch attributes that are actually used
    let title = element
        .attribute(&AXAttribute::title())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    let description = element
        .attribute(&AXAttribute::description())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    let value_raw = element
        .attribute(&AXAttribute::value())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    // Set unused fields to empty/default values
    let subrole = String::new();
    let identifier = String::new();
    let help = String::new();
    let enabled = false;
        
    // Get window title by traversing up to window
    let window_title = get_window_title(element);

    // Extract clean value content - try CFString first, then CFNumber
    let mut value = extract_cfstring_content(&value_raw);
    if value.is_empty() {
        value = extract_cfnumber_value(&value_raw);
    }
    
    Some(ActionableElement {
        role: clean_attribute_value(&role),
        subrole: clean_attribute_value(&subrole),
        title: clean_attribute_value(&title),
        description: rename_element_description(&clean_attribute_value(&description)),
        identifier: clean_attribute_value(&identifier),
        value,
        help: clean_attribute_value(&help),
        enabled,
        window_title,
        path,
        group_context: String::new(),     // Will be filled by context-aware collector
        sibling_context: String::new(),   // Will be filled by context-aware collector
        depth: 0,                          // Will be filled during traversal
    })
}

/// Faster variant: Create ActionableElement using precomputed role and window title
fn create_actionable_element_fast(
    element: &AXUIElement,
    precomputed_role: String,
    window_title: &str,
    path: String,
    depth: usize,
) -> Option<ActionableElement> {
    // Only fetch attributes that are actually used
    let title = element
        .attribute(&AXAttribute::title())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());

    let mut description = element
        .attribute(&AXAttribute::description())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());

    let value_raw = element
        .attribute(&AXAttribute::value())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());

    // For checkboxes and radio buttons with empty description/title, try to find sibling text labels
    if precomputed_role.contains("AXCheckBox") || precomputed_role.contains("AXRadioButton") {
        let cleaned_title = clean_attribute_value(&title);
        let cleaned_desc = clean_attribute_value(&description);

        if cleaned_title.is_empty() && cleaned_desc.is_empty() {
            // Try to find a sibling AXStaticText element that serves as the label
            if let Some(label_text) = find_sibling_text_label(element) {
                info!("Found sibling label for {}: {}", precomputed_role, label_text);
                description = label_text;
            }
        }
    }

    // For text fields and sliders, the value might be a simple string not wrapped in CFString format
    let mut value = if precomputed_role.contains("TextField") || precomputed_role.contains("TextArea") || precomputed_role.contains("Slider") {
        // First try the CFString extraction
        let extracted = extract_cfstring_content(&value_raw);
        if !extracted.is_empty() {
            extracted
        } else {
            // If that fails, try cleaning the raw value directly
            // The value might be formatted as just "11000" or "\"11000\""
            clean_attribute_value(&value_raw)
        }
    } else {
        // For other elements (checkboxes, radio buttons, etc.), try CFString first, then CFNumber
        let extracted = extract_cfstring_content(&value_raw);
        if !extracted.is_empty() {
            extracted
        } else {
            extract_cfnumber_value(&value_raw)
        }
    };

    // For text fields, text areas, and combo boxes - capture placeholder text if value is empty
    if (precomputed_role.contains("TextField") || precomputed_role.contains("TextArea") || precomputed_role.contains("ComboBox")) && value.is_empty() {
        if let Ok(placeholder) = element.attribute(&AXAttribute::new(&CFString::new("AXPlaceholderValue"))) {
            let placeholder_str = format!("{:?}", placeholder);
            let placeholder_clean = extract_cfstring_content(&placeholder_str);
            if !placeholder_clean.is_empty() {
                value = placeholder_clean;
            }
        }
    }

    // For sliders, extract clean numeric value and build descriptive text
    if precomputed_role.contains("Slider") {
        // Extract the current numeric value (this is what can be SET)
        if !value.is_empty() {
            let numeric_value = extract_cfnumber_value(&value);
            if !numeric_value.is_empty() {
                value = numeric_value;
            }
        }

        // Build descriptive info for the description field
        let mut slider_info = vec![];

        // Try to get min/max values for the description
        if let Ok(min_value) = element.attribute(&AXAttribute::new(&CFString::new("AXMinValue"))) {
            let min_str = format!("{:?}", min_value);
            let min_numeric = extract_cfnumber_value(&min_str);
            if !min_numeric.is_empty() {
                slider_info.push(format!("min:{}", min_numeric));
            }
        }

        if let Ok(max_value) = element.attribute(&AXAttribute::new(&CFString::new("AXMaxValue"))) {
            let max_str = format!("{:?}", max_value);
            let max_numeric = extract_cfnumber_value(&max_str);
            if !max_numeric.is_empty() {
                slider_info.push(format!("max:{}", max_numeric));
            }
        }

        // Try to get value description (more user-friendly description)
        if let Ok(value_desc) = element.attribute(&AXAttribute::new(&CFString::new("AXValueDescription"))) {
            let desc_str = format!("{:?}", value_desc);
            let desc_clean = extract_cfstring_content(&desc_str);
            if !desc_clean.is_empty() {
                slider_info.push(desc_clean);
            }
        }

        // Append slider info to the description field (not value)
        if !slider_info.is_empty() {
            let slider_desc = slider_info.join(" ");
            // Append to existing description or create new one
            if description.is_empty() {
                description = slider_desc;
            } else {
                let existing_desc = clean_attribute_value(&description);
                if !existing_desc.is_empty() {
                    description = format!("{} [{}]", existing_desc, slider_desc);
                } else {
                    description = slider_desc;
                }
            }
        }
    }

    // Mark as enabled if it's an actionable role to avoid over-filtering downstream
    let enabled = is_truly_actionable_element(&precomputed_role);

    Some(ActionableElement {
        role: clean_attribute_value(&precomputed_role),
        subrole: String::new(),
        title: clean_attribute_value(&title),
        description: rename_element_description(&clean_attribute_value(&description)),
        identifier: String::new(),
        value,
        help: String::new(),
        enabled,
        window_title: window_title.to_string(),
        path,
        group_context: String::new(),     // Will be filled by context-aware collector
        sibling_context: String::new(),   // Will be filled by context-aware collector
        depth,
    })
}

/// Get the title of the window containing this element
fn get_window_title(element: &AXUIElement) -> String {
    let mut current = element.clone();
    
    // Try to traverse up the tree to find a window
    for _ in 0..10 {  // Limit traversal depth
        if let Ok(role) = current.attribute(&AXAttribute::role()) {
            if format!("{:?}", role).contains("AXWindow") {
                // Found a window, get its title
                return current
                    .attribute(&AXAttribute::title())
                    .map(|title| clean_attribute_value(&format!("{:?}", title)))
                    .unwrap_or_else(|_| "Untitled Window".to_string());
            }
        }
        
        // Try to get the parent
        match current.attribute(&AXAttribute::parent()) {
            Ok(parent) => current = parent,
            Err(_) => break,
        }
    }
    
    "Unknown Window".to_string()
}

/// Scans all windows in the application for actionable elements and text content
fn scan_windows_for_actionable_elements_and_text(application: &AXUIElement) -> (Vec<ActionableElement>, String, String) {
    let mut actionable_elements = Vec::new();
    let mut all_elements = Vec::new();
    let mut text_content = String::new();
    let mut full_text_content = String::new();

    // Get all windows from the application
    let windows: CFArray<AXUIElement> = match application.attribute(&AXAttribute::windows()) {
        Ok(windows) => windows,
        Err(_) => return (actionable_elements, text_content.clone(), String::new()),
    };

    let start_time = Instant::now();

    // Check if this is Microsoft Word by checking the application title
    let app_title = application.attribute(&AXAttribute::title())
        .map(|t| format!("{:?}", t))
        .unwrap_or_default();

    info!("🔍 WORD DETECTION CHECK: App title = '{}'", app_title);

    // Check window titles as well
    let has_word_window = windows.iter().any(|window| {
        let title = window.attribute(&AXAttribute::title());
        let title_str = title.as_ref().map(|t| format!("{:?}", t)).unwrap_or_default();
        info!("🔍 WORD DETECTION CHECK: Window title = '{}'", title_str);
        title_str.contains("Microsoft Word") || title_str.contains("Word") || title_str.ends_with(".docx") || title_str.ends_with(".doc")
    });

    let is_word = app_title.contains("Microsoft Word") || app_title.contains("Word") || has_word_window;
    info!("🔍 WORD DETECTION RESULT: is_word = {}", is_word);

    // Process each window with context-aware collection
    for window in windows.iter() {
        // Compute window title once per window
        let window_title = window
            .attribute(&AXAttribute::title())
            .map(|t| clean_attribute_value(&format!("{:?}", t)))
            .unwrap_or_else(|_| "Untitled Window".to_string());

        // Use the new context-aware collection
        collect_actionable_elements_with_context(
            &window,
            &window_title,
            &mut all_elements,
            &mut text_content,
            &mut full_text_content,
            start_time,
        );

        if all_elements.len() >= MAX_ELEMENTS_PER_SCAN
            || start_time.elapsed() > Duration::from_millis(SCAN_TIME_BUDGET_MS) {
            break;
        }
    }

    // Convert to just ActionableElement without the AXUIElement
    actionable_elements = all_elements.into_iter().map(|(_, element)| element).collect();

    // Add synthetic Document element for Microsoft Word
    if is_word {
        info!("📝 WORD SYNTHETIC ELEMENT: Detected Microsoft Word - adding synthetic Document element");
        let document_element = ActionableElement {
            role: "AXTextArea".to_string(),
            subrole: "".to_string(),
            title: "Document".to_string(),
            description: "Word document content area".to_string(),
            identifier: "word_document_content".to_string(),
            value: "".to_string(),
            help: "Type text into the Word document".to_string(),
            enabled: true,
            window_title: if let Some(first_elem) = actionable_elements.first() {
                first_elem.window_title.clone()
            } else {
                "Microsoft Word".to_string()
            },
            path: "word_document_content".to_string(),
            group_context: "".to_string(),
            sibling_context: "".to_string(),
            depth: 999,
        };
        // Add at the end of the list
        actionable_elements.push(document_element);
        info!("✅ WORD SYNTHETIC ELEMENT: Successfully added synthetic Document element at end of list with path 'word_document_content'");
    } else {
        info!("❌ WORD SYNTHETIC ELEMENT: Not Word, skipping synthetic element creation");
    }

    // Reorder to move browser chrome to the top
    actionable_elements = reorder_browser_chrome(actionable_elements);

    // Dynamically truncate text content based on UI complexity
    // Allow more context for very large screens (>150 elements)
    let char_limit: usize = if actionable_elements.len() > 150 { 20_000 } else { 5_000 };

    let truncated_text = if text_content.len() > char_limit {
        let mut char_boundary = char_limit;
        while !text_content.is_char_boundary(char_boundary) && char_boundary > 0 {
            char_boundary -= 1;
        }
        format!("{}...", &text_content[..char_boundary])
    } else {
        text_content
    };

    // Return: (elements, truncated "Other Visible Content", complete text with all static/headers)
    (actionable_elements, truncated_text, full_text_content)
}

// NOTE: scan_windows_for_full_text_only and collect_all_text_content have been removed.
// Full text is now collected during the initial scan in collect_actionable_elements_with_context
// and returned as the third element of the tuple from scan_windows_for_actionable_elements_and_text.

/// Determines if an element is a text-bearing element that should contribute to visible text content
fn is_text_bearing_element(role: &str) -> bool {
    // Text-bearing elements that provide important content context
    role.contains("AXStaticText") ||     // Static text elements (paragraphs, labels, etc.)
    role.contains("AXText") ||           // Other text elements
    role.contains("AXHeading") ||        // Headings that provide structure context
    role.contains("AXLabel") ||          // Labels that describe other elements
    role.contains("AXTitle") ||          // Title elements
    role.contains("AXDescription") ||    // Description text
    role.contains("AXParagraph") ||      // Paragraph text
    role.contains("AXCaption") ||        // Caption text
    
    // Table/Grid elements that often contain data
    role.contains("AXCell") ||           // Table cells with text data
    role.contains("AXColumnHeader") ||   // Column headers
    role.contains("AXRowHeader") ||      // Row headers
    role.contains("AXGridCell") ||       // Grid cells
    
    // List elements
    role.contains("AXListItem") ||       // List items often have text
    role.contains("AXListMarker") ||     // Bullet points/numbers
    
    // Definition lists
    role.contains("AXDefinition") ||     // Definition text
    role.contains("AXTerm") ||           // Terms being defined
    
    // Web-specific content
    role.contains("AXBlockquote") ||     // Quoted text
    role.contains("AXCode") ||           // Code snippets
    role.contains("AXPre") ||            // Preformatted text
    
    // Status/Info elements
    role.contains("AXStatusBar") ||      // Status messages
    role.contains("AXProgressIndicator") || // Progress text
    
    // Time/Date displays
    role.contains("AXTimeField") ||      // Time displays
    role.contains("AXDateField") ||      // Date displays
    
    // Help text (removing from skip list)
    role.contains("AXHelpTag")           // Help/hint text for elements
}

/// Determines if an element is truly actionable (can be clicked, typed into, etc.)
fn is_truly_actionable_element(role: &str) -> bool {
    // Only include elements that can actually be interacted with
    role.contains("AXButton") || 
    role.contains("AXMenuItem") ||
    role.contains("AXMenu") || 
    role.contains("AXPopUpButton") || 
    role.contains("AXRadioButton") || 
    role.contains("AXCheckBox") || 
    role.contains("AXTextField") || 
    role.contains("AXTextArea") || 
    role.contains("AXSlider") || 
    role.contains("AXScrollBar") || 
    role.contains("AXComboBox") || 
    role.contains("AXToolbar") || 
    role.contains("AXList") || 
    role.contains("AXPopover") || 
    role.contains("AXTabGroup") ||
    role.contains("AXTab") ||
    role.contains("AXSearchField") ||
    
    // Web-specific elements that are commonly actionable in browsers
    role.contains("AXLink") ||           // Clickable links
    role.contains("AXWebArea") ||        // Web page content areas
    role.contains("AXCell") ||           // Table cells that can be clickable
    role.contains("AXRow") ||            // Table/list rows
    
    // Interactive containers that might be clickable
    (role.contains("AXGroup") && 
     // Only if they're likely to be interactive (we'll check for press actions separately)
     false) // For now, we'll let has_press_action handle AXGroup elements
}

/// Determines if an element is actionable based on its role (legacy function for compatibility)
#[allow(dead_code)]
fn is_actionable_element_type(role: &str) -> bool {
    // This function is kept for backward compatibility with other parts of the code
    // It includes both actionable elements and text elements
    is_truly_actionable_element(role) ||
    
    // Text-bearing elements that provide important content context
    role.contains("AXStaticText") ||     // Static text elements (table cells, labels, etc.)
    role.contains("AXText") ||           // Other text elements
    role.contains("AXHeading") ||        // Headings that provide structure context
    role.contains("AXLabel") ||          // Labels that describe other elements
    role.contains("AXImage") ||          // Images that might have descriptive text
    role.contains("AXGenericContainer")  // Generic containers
}

/// Gets an identifier for the element to use in paths
#[allow(dead_code)]
fn get_element_identifier(element: &AXUIElement) -> String {
    // Prioritize description and title as suggested by the user
    let description = element
        .attribute(&AXAttribute::description())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    if !description.is_empty() && description != "null" && description != "\"\"" {
        return clean_attribute_value(&description);
    }
    
    let title = element
        .attribute(&AXAttribute::title())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    if !title.is_empty() && title != "null" && title != "\"\"" {
        return clean_attribute_value(&title);
    }
    
    // Then try identifier
    let identifier = element
        .attribute(&AXAttribute::identifier())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    if !identifier.is_empty() && identifier != "null" && identifier != "\"\"" {
        return clean_attribute_value(&identifier);
    }
    
    // Try value for elements like text fields
    let value = element
        .attribute(&AXAttribute::value())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    if !value.is_empty() && value != "null" && value != "\"\"" {
        let cleaned_value = clean_attribute_value(&value);
        if !cleaned_value.is_empty() && cleaned_value.len() < 50 {  // Avoid very long values
            return cleaned_value;
        }
    }
    
    // Final fallback to role (this ensures we always have something)
    let role = element
        .attribute(&AXAttribute::role())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| "unknown".to_string());
        
    clean_attribute_value(&role)
}

/// Enhanced element identifier that guarantees uniqueness using role counters
fn get_enhanced_element_identifier(
    element: &AXUIElement,
    role_counters: &mut std::collections::HashMap<String, usize>
) -> String {
    // Get the role first
    let role = element
        .attribute(&AXAttribute::role())
        .map(|r| clean_attribute_value(&format!("{:?}", r)))
        .unwrap_or_else(|_| "Unknown".to_string());

    // Get description, title, identifier, and value
    let description = element
        .attribute(&AXAttribute::description())
        .map(|d| clean_attribute_value(&format!("{:?}", d)))
        .unwrap_or_else(|_| String::new());

    let title = element
        .attribute(&AXAttribute::title())
        .map(|t| clean_attribute_value(&format!("{:?}", t)))
        .unwrap_or_else(|_| String::new());

    let identifier = element
        .attribute(&AXAttribute::identifier())
        .map(|i| clean_attribute_value(&format!("{:?}", i)))
        .unwrap_or_else(|_| String::new());

    let value_raw = element
        .attribute(&AXAttribute::value())
        .map(|v| format!("{:?}", v))
        .unwrap_or_else(|_| String::new());

    // Extract clean value - try CFString first, then CFNumber (for checkboxes, sliders, etc.)
    let mut value = extract_cfstring_content(&value_raw);
    if value.is_empty() {
        value = extract_cfnumber_value(&value_raw);
    }

    // Create a role-qualified identifier with value for better disambiguation
    let base_identifier = if !description.is_empty() {
        // Include value if present and not too long
        if !value.is_empty() && value.len() <= 50 {
            format!("{}_{}", description, value)
        } else {
            description
        }
    } else if !title.is_empty() {
        // Include value if present and not too long
        if !value.is_empty() && value.len() <= 50 {
            format!("{}_{}", title, value)
        } else {
            title
        }
    } else if !identifier.is_empty() {
        identifier
    } else if !value.is_empty() {
        value
    } else {
        // Use role + counter as fallback
        let counter = role_counters.entry(role.clone()).or_insert(0);
        *counter += 1;
        format!("{}-{}", role, counter)
    };

    // Always prefix with role for better matching precision
    format!("{}:{}", role, base_identifier)
}

/// Build a simple role-qualified identifier using already-fetched fields (no extra AX calls)
fn make_role_qualified_identifier(
    element_info: &ActionableElement,
    role_counters: &mut std::collections::HashMap<String, usize>
) -> String {
    let role = &element_info.role;

    // Build identifier - include value for uniqueness when multiple fields have same description/title
    let mut base_identifier = if !element_info.description.is_empty() {
        // If we have both description and value, combine them for uniqueness
        if !element_info.value.is_empty() && element_info.value.len() <= 50 {
            format!("{}_{}", element_info.description, element_info.value)
        } else {
            element_info.description.clone()
        }
    } else if !element_info.title.is_empty() {
        // If we have both title and value, combine them for uniqueness
        if !element_info.value.is_empty() && element_info.value.len() <= 50 {
            format!("{}_{}", element_info.title, element_info.value)
        } else {
            element_info.title.clone()
        }
    } else if !element_info.value.is_empty() {
        element_info.value.clone()
    } else {
        let counter = role_counters.entry(role.clone()).or_insert(0);
        *counter += 1;
        format!("{}-{}", role, counter)
    };

    // For text elements and headings, truncate very long identifiers to keep paths manageable
    // This is especially important for STATIC_TEXT_MARKER and HEADING_MARKER elements
    if (role == "STATIC_TEXT_MARKER" || role == "HEADING_MARKER") && base_identifier.len() > 100 {
        // Truncate to 100 chars for text elements to keep paths reasonable
        let truncated: String = base_identifier.chars().take(100).collect();
        base_identifier = format!("{}...", truncated);
    }

    format!("{}:{}", role, base_identifier)
}

/// Cleans up attribute values by removing quotes and special characters
fn clean_attribute_value(value: &str) -> String {
    let cleaned = value
        .trim_start_matches('"')
        .trim_end_matches('"')
        .trim()
        .to_string();

    if cleaned == "null" {
        String::new()
    } else {
        cleaned
    }
}

/// Renames element descriptions to be more descriptive and user-friendly
fn rename_element_description(description: &str) -> String {
    // Check for exact match of "Address and search bar" and rename it
    if description == "Address and search bar" {
        return "Browser address and search bar - use to input website names.".to_string();
    }

    // Return original description if no match
    description.to_string()
}

/// Formats a collection of actionable elements into a human-readable string with hierarchical context
pub fn format_actionable_elements(elements: &[ActionableElement]) -> String {
    let mut result = String::new();

    if elements.is_empty() {
        return "No actionable elements found.".to_string();
    }

    let mut current_window = String::new();
    let mut current_group_context = String::new();
    let mut count = 0;

    for element in elements {
        // Start a new window section if needed
        if current_window != element.window_title {
            if !current_window.is_empty() {
                result.push_str("\n");
            }
            result.push_str(&format!("══════ WINDOW: {} ══════\n", element.window_title));
            current_window = element.window_title.clone();
            current_group_context = String::new(); // Reset group context for new window
        }

        // Display group context if it changed (like "Machine Learning Engineers")
        if !element.group_context.is_empty() && element.group_context != current_group_context {
            result.push_str(&format!("\n▶ GROUP: {}\n", element.group_context));
            result.push_str("----------------------------------------\n");
            current_group_context = element.group_context.clone();
        }

        // Handle special marker roles differently for better readability
        if element.role == "HEADING_MARKER" {
            result.push_str(&format!("\n━━━ HEADING: {} ━━━\n",
                if !element.title.is_empty() {
                    &element.title
                } else if !element.description.is_empty() {
                    &element.description
                } else {
                    &element.value
                }
            ));
            // Don't increment count for headers, they're structural
            continue;
        } else if element.role == "STATIC_TEXT_MARKER" {
            // Display static text prominently but not as a numbered element
            result.push_str(&format!("    📝 {}\n",
                if !element.title.is_empty() {
                    &element.title
                } else if !element.description.is_empty() {
                    &element.description
                } else {
                    &element.value
                }
            ));
            // Don't increment count for static text markers
            continue;
        }

        count += 1;

        // Format the element entry (for actual actionable elements)
        result.push_str(&format!("{}. [{} {}] {}\n",
            count,
            element.role,
            if element.enabled { "✓" } else { "✗" },
            if !element.title.is_empty() {
                &element.title
            } else if !element.description.is_empty() {
                &element.description
            } else {
                "[No Label]"
            }
        ));

        // Add sibling context if present (e.g., nearby static text)
        if !element.sibling_context.is_empty() {
            result.push_str(&format!("   Context: {}\n", element.sibling_context));
        }

        // Add description if present and different from title
        if !element.description.is_empty() && element.description != element.title {
            result.push_str(&format!("   Description: {}\n", element.description));
        }

        // Add value if present
        if !element.value.is_empty() {
            result.push_str(&format!("   Current Value: {}\n", element.value));
        }

        // Always add the path for automation purposes (but make it less prominent)
        result.push_str(&format!("   [Path: {}]\n", element.path));
        result.push_str("\n");
    }

    // Add summary
    result.push_str("────────────────────────────────────────\n");
    result.push_str(&format!("Total actionable elements: {}\n", count));

    result
}

/// Clicks on a specific element by its reference
pub fn click_element_by_id(pid: &str, element_ref: &str) -> ActionResult {
    info!("Attempting to click element by reference ID: {} in app with PID: {}", element_ref, pid);
    
    // Parse the PID to i32
    let pid = match pid.parse::<i32>() {
        Ok(pid) => pid,
        Err(e) => {
            let msg = format!("Failed to parse PID: {} - Error: {}", pid, e);
            info!("❌ {}", msg);
            return ActionResult {
                success: false,
                message: msg,
            };
        }
    };
    
    // Get the application from the PID
    let application = AXUIElement::application(pid);
    
    // Find the element with the matching ID/path using the helper function
    let element_ui = match find_element_by_path(&application, element_ref) {
        Some(element) => element,
        None => {
            let msg = format!("No element with ID '{}' could be found", element_ref);
            warn!("{}", msg);
            return ActionResult {
                success: false,
                message: msg,
            };
        }
    };
    
    // Get element info for logging
    let role = element_ui
        .attribute(&AXAttribute::role())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    let title = element_ui
        .attribute(&AXAttribute::title())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
        
    let description = element_ui
        .attribute(&AXAttribute::description())
        .map(|r| format!("{:?}", r))
        .unwrap_or_else(|_| String::new());
    
    let title_clean = clean_attribute_value(&title);
    let description_clean = clean_attribute_value(&description);
    
    info!("Found matching element: {} - '{}'", 
          role, 
          if !title_clean.is_empty() { &title_clean } else { &description_clean });
    
    let element_name = if !title_clean.is_empty() { 
        &title_clean 
    } else if !description_clean.is_empty() { 
        &description_clean 
    } else { 
        "unnamed" 
    };
    
    // Use the helper function to try multiple actions
    try_multiple_actions(&element_ui, element_name, &role)
}

/// Types text into the currently focused element using accessibility API
pub fn type_text_in_focused_element(pid: &str, text: &str) -> ActionResult {
    // Parse PID for accessibility API usage
    let app_pid = match pid.parse::<i32>() {
        Ok(pid) => pid,
        Err(e) => return ActionResult {
            success: false,
            message: format!("Invalid PID: {}", e),
        },
    };
    
    // Get application reference
    let application = AXUIElement::application(app_pid);
    
    // Create the attribute for focused UI element
    let focused_string = CFString::new("AXFocusedUIElement");
    let focused_attr = AXAttribute::new(&focused_string);
    
    // Try to get the focused element
    let focused_element = match application.attribute(&focused_attr) {
        Ok(element) => element,
        Err(e) => return ActionResult {
            success: false,
            message: format!("Failed to get focused element: {:?}", e),
        },
    };
    
    // Convert to AXUIElement
    let element_ref = match focused_element.downcast::<AXUIElement>() {
        Some(element) => element,
        None => return ActionResult {
            success: false,
            message: "Failed to convert focused element to AXUIElement".to_string(),
        },
    };
    
    // Check if it's a text field
    let role_attr = AXAttribute::role();
    let role = match element_ref.attribute(&role_attr) {
        Ok(role) => format!("{:?}", role),
        Err(_) => String::new(),
    };
    
    // Only proceed if it's a text field or text area
    if !role.contains("AXTextField") && !role.contains("AXTextArea") {
        return ActionResult {
            success: false,
            message: format!("Focused element is not a text field (role: {})", role),
        };
    }
    
    // Create the text value
    let text_value = CFString::new(text);
    
    // Set the value
    unsafe {
        let attr_name = CFString::new("AXValue");
        
        let result = AXUIElementSetAttributeValue(
            element_ref.as_concrete_TypeRef() as *mut _,
            attr_name.as_concrete_TypeRef(),
            text_value.as_CFTypeRef(),
        );
        
        if result == 0 {
            return ActionResult {
                success: true,
                message: format!("Successfully typed '{}' in text field using accessibility API", text),
            };
        } else {
            return ActionResult {
                success: false,
                message: format!("Failed to type text with accessibility API: error code {}", result),
            };
        }
    }
}

pub fn type_text_in_element_by_id(pid: &str, element_path: &str, text: &str) -> ActionResult {
    // Parse PID for accessibility API usage
    let app_pid = match pid.parse::<i32>() {
        Ok(pid) => pid,
        Err(e) => return ActionResult {
            success: false,
            message: format!("Invalid PID: {}", e),
        },
    };

    // Get application reference
    let application = AXUIElement::application(app_pid);

    // Get the specific element by path
    let element_ref = match find_element_by_path(&application, element_path) {
        Some(element) => element,
        None => {
            error!("Could not find element with path: {}", element_path);
            return ActionResult {
                success: false,
                message: format!("Could not find element with path: {}", element_path),
            };
        },
    };

    // Check if it's a text field
    let role_attr = AXAttribute::role();
    let role = match element_ref.attribute(&role_attr) {
        Ok(role) => format!("{:?}", role),
        Err(_) => String::new(),
    };
    
    // STEP 1: First PRESS (click) the element explicitly
    // This better simulates a real user click which both focuses and usually selects text
    unsafe {
        let press_action = CFString::new("AXPress");
        let press_result = AXUIElementPerformAction(
            element_ref.as_concrete_TypeRef() as *mut _,
            press_action.as_concrete_TypeRef()
        );
        
        if press_result != 0 {
            info!("Could not press/click element - attempting focus instead");
            
            // Try focus as fallback
            let focus_action = CFString::new("AXFocus");
            let focus_result = AXUIElementPerformAction(
                element_ref.as_concrete_TypeRef() as *mut _,
                focus_action.as_concrete_TypeRef()
            );
            
            if focus_result != 0 {
                warn!("Both press and focus failed for element - API calls returned: Press={}, Focus={}", 
                     press_result, focus_result);
            } else {
                info!("Successfully focused element before typing");
            }
        } else {
            info!("Successfully clicked element before typing");
        }
        
        // Reduced delay after clicking/focusing to let UI respond
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    
    // STEP 2: For text fields and text areas, clear existing content using AXValue API
    if role.contains("AXTextField") || role.contains("AXTextArea") {
        unsafe {
            // First try to select all text using AXSelectedTextRange
            let selection_start = CFString::new("AXSelectedTextRange");
            let range_value = CFString::new("0,9999"); // Start at 0, select everything
            let range_result = AXUIElementSetAttributeValue(
                element_ref.as_concrete_TypeRef() as *mut _,
                selection_start.as_concrete_TypeRef(),
                range_value.as_CFTypeRef(),
            );

            if range_result == 0 {
                info!("✓ Selected all text in field");
            } else {
                info!("Could not select text using AXSelectedTextRange: {}", range_result);
            }

            // Now clear the field by setting value to empty string
            let empty_value = CFString::new("");
            let clear_result = AXUIElementSetAttributeValue(
                element_ref.as_concrete_TypeRef() as *mut _,
                CFString::new("AXValue").as_concrete_TypeRef(),
                empty_value.as_CFTypeRef(),
            );

            if clear_result == 0 {
                info!("✓ Cleared text field before typing");
                std::thread::sleep(std::time::Duration::from_millis(50));
            } else {
                info!("Could not clear field: {}, will type over existing content", clear_result);
            }
        }
    }

    // STEP 3: Type the text character-by-character
    info!("Typing text character-by-character");
    match type_text_character_by_character(text) {
        Ok(_) => {
            info!("✓ SUCCESS: Typed text character-by-character");
            return ActionResult {
                success: true,
                message: format!("Successfully typed '{}' using keyboard events", text),
            };
        },
        Err(e) => {
            error!("✗ FAILED: Character-by-character typing failed: {}", e);
            return ActionResult {
                success: false,
                message: format!("Failed to type text: {}", e),
            };
        }
    }
}

// OLD IMPLEMENTATION BELOW - Keeping for reference
fn _type_text_in_element_by_id_old_with_axvalue(pid: &str, element_path: &str, text: &str) -> ActionResult {
    let app_pid = match pid.parse::<i32>() {
        Ok(pid) => pid,
        Err(e) => return ActionResult {
            success: false,
            message: format!("Invalid PID: {}", e),
        },
    };

    let application = AXUIElement::application(app_pid);

    let element_ref = match find_element_by_path(&application, element_path) {
        Some(element) => element,
        None => {
            error!("Could not find element with path: {}", element_path);
            return ActionResult {
                success: false,
                message: format!("Could not find element with path: {}", element_path),
            };
        },
    };

    let role_attr = AXAttribute::role();
    let role = match element_ref.attribute(&role_attr) {
        Ok(role) => format!("{:?}", role),
        Err(_) => String::new(),
    };

    if role.contains("AXTextField") || role.contains("AXTextArea") {
        unsafe {
            let selection_start = CFString::new("AXSelectedTextRange");
            
            // Try to create a selection range (start at 0, length = all text)
            // We'll use a more robust approach by trying multiple selection methods
            
            // Method 1: Try AXSelectedTextRange if available
            let range_value = CFString::new("0,9999"); // Start at 0, select everything
            let range_result = AXUIElementSetAttributeValue(
                element_ref.as_concrete_TypeRef() as *mut _,
                selection_start.as_concrete_TypeRef(),
                range_value.as_CFTypeRef(),
            );
            
            if range_result == 0 {
                info!("Successfully selected all text in field");
            } else {
                info!("Could not select text using AXSelectedTextRange: {}", range_result);
                
                // Method 2: For browser address bars, just set empty value first
                // This effectively clears the field
                let empty_value = CFString::new("");
                let clear_result = AXUIElementSetAttributeValue(
                    element_ref.as_concrete_TypeRef() as *mut _,
                    CFString::new("AXValue").as_concrete_TypeRef(),
                    empty_value.as_CFTypeRef(),
                );
                
                if clear_result == 0 {
                    info!("Successfully cleared text field before typing");
                    // Minimal pause after clearing - removed unnecessary delay
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    }
    
    // STEP 3: Now set the text value
    let text_value = CFString::new(text);

    unsafe {
        let attr_name = CFString::new("AXValue");

        info!("Attempting to set text value using AXValue API");

        let result = AXUIElementSetAttributeValue(
            element_ref.as_concrete_TypeRef() as *mut _,
            attr_name.as_concrete_TypeRef(),
            text_value.as_CFTypeRef(),
        );

        info!("AXValue set result code: {}", result);

        if result == 0 {
            // API call succeeded, but we need to VERIFY the value was actually set in the UI
            // Give the UI and event system a moment to process and fire AXValueChanged notification
            std::thread::sleep(std::time::Duration::from_millis(50));

            // Verify by reading back value directly
            info!("Verifying by reading back value...");

            // Read back the actual value from the element
            let readback = element_ref.attribute(&AXAttribute::value());

            let value_actually_set = match readback {
                Ok(val) => {
                    let val_str = format!("{:?}", val);
                    let extracted = extract_cfstring_content(&val_str);

                    info!("Value read back from element: '{}'", extracted);
                    info!("Expected value: '{}'", text);

                    // Smart verification logic:
                    // 1. Exact match - perfect!
                    if extracted == text {
                        info!("✓ Exact match!");
                        true
                    }
                    // 2. For URL fields: if we're setting a URL and the readback starts with what we set
                    //    (browser may have autocompleted or kept existing path)
                    else if text.starts_with("http://") || text.starts_with("https://") {
                        if extracted.starts_with(text) {
                            info!("✓ URL match - readback starts with expected value (browser kept the path)");
                            true
                        } else if extracted == text.trim_end_matches('/') || text == extracted.trim_end_matches('/') {
                            info!("✓ URL match - differs only by trailing slash");
                            true
                        } else {
                            info!("✗ URL mismatch - readback doesn't start with expected value");
                            false
                        }
                    }
                    // 3. Readback is empty - value definitely wasn't set
                    else if extracted.is_empty() {
                        info!("✗ Readback is empty - value not set");
                        false
                    }
                    // 4. For other fields, require exact match
                    else {
                        info!("✗ No match");
                        false
                    }
                },
                Err(e) => {
                    warn!("Could not read back value to verify: {:?}", e);
                    false
                }
            };

            if value_actually_set {
                info!("✓ SUCCESS: Value was set correctly using AXValue API");
                return ActionResult {
                    success: true,
                    message: format!("Successfully typed '{}' in element at path {}", text, element_path),
                };
            } else {
                warn!("✗ VERIFICATION FAILED: AXValue API call succeeded but value not reflected in UI");
                warn!("→ This is likely a web-based editor (like Reddit) that needs keyboard events");
                warn!("→ Falling back to character-by-character typing");
                // Fall through to character-by-character typing below
            }
        } else {
            warn!("AXValue API call failed with error code: {}", result);
            warn!("→ Falling back to character-by-character typing");
            // Fall through to character-by-character typing below
        }

        // FALLBACK: Character-by-character typing for web-based editors
        info!("Using character-by-character typing (human-like keyboard events)");
        match type_text_character_by_character(text) {
            Ok(_) => {
                info!("✓ SUCCESS: Typed text character-by-character");
                return ActionResult {
                    success: true,
                    message: format!("Successfully typed '{}' using keyboard events", text),
                };
            },
            Err(e) => {
                error!("✗ FAILED: Character-by-character typing failed: {}", e);
                return ActionResult {
                    success: false,
                    message: format!("Failed to type text: {}", e),
                };
            }
        }
    }
}

/// Types text character-by-character using CGEvent API
/// This works better for web-based text editors that rely on keyboard events
fn type_text_character_by_character(text: &str) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventTapLocation, CGKeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    // Create an event source
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|e| format!("Failed to create event source: {:?}", e))?;

    // Random number generator for human-like timing variation
    let mut rng = rand::thread_rng();

    for ch in text.chars() {
        // Create a keyboard event for this character
        let key_down = CGEvent::new_keyboard_event(source.clone(), 0 as CGKeyCode, true)
            .map_err(|e| format!("Failed to create key down event: {:?}", e))?;

        let key_up = CGEvent::new_keyboard_event(source.clone(), 0 as CGKeyCode, false)
            .map_err(|e| format!("Failed to create key up event: {:?}", e))?;

        // Set the Unicode character for this event
        key_down.set_string_from_utf16_unchecked(&[ch as u16]);
        key_up.set_string_from_utf16_unchecked(&[ch as u16]);

        // Post the key down event
        key_down.post(CGEventTapLocation::HID);

        // Randomized delay between down and up - typical key press duration (15-30ms)
        let key_press_duration = rng.gen_range(15..30);
        std::thread::sleep(std::time::Duration::from_millis(key_press_duration));

        // Post the key up event
        key_up.post(CGEventTapLocation::HID);

        // Randomized delay before next character (40-70ms) - mimics efficient human typing (~80-100 WPM)
        let char_delay = rng.gen_range(40..70);
        std::thread::sleep(std::time::Duration::from_millis(char_delay));
    }

    // Small delay at the end to ensure all events are processed
    std::thread::sleep(std::time::Duration::from_millis(100));

    Ok(())
}

/// Find an element by its path in the accessibility tree
fn find_element_by_path(application: &AXUIElement, element_path: &str) -> Option<AXUIElement> {
    // Get all windows from the application
    let windows: CFArray<AXUIElement> = match application.attribute(&AXAttribute::windows()) {
        Ok(windows) => windows,
        Err(e) => {
            warn!("Failed to get windows: {:?}", e);
            return None;
        },
    };

    if windows.len() == 0 {
        return None;
    }

    // Collect all elements with their paths
    let mut all_elements = Vec::new();

    let start_time = Instant::now();

    for window in windows.iter() {
        let window_title = window
            .attribute(&AXAttribute::title())
            .map(|t| clean_attribute_value(&format!("{:?}", t)))
            .unwrap_or_else(|_| "Untitled Window".to_string());

        // Use context-aware collection for better element matching
        collect_actionable_elements_with_context(
            &window,
            &window_title,
            &mut all_elements,
            &mut String::new(),
            &mut String::new(),
            start_time,
        );

        // For find, be a bit more permissive on time, but still allow break if we already have a lot
        if all_elements.len() >= MAX_ELEMENTS_PER_SCAN
            || start_time.elapsed() > Duration::from_millis(SCAN_TIME_BUDGET_MS * 2) {
            break;
        }
    }

    // Parse the element path to check if it's role-qualified (e.g., "AXTextArea:Search")
    let (target_role, target_identifier) = if element_path.contains(':') {
        let parts: Vec<&str> = element_path.splitn(2, ':').collect();
        (Some(parts[0]), parts[1])
    } else {
        (None, element_path)
    };

    // Find the element with matching criteria, prioritizing exact matches
    let mut exact_matches = Vec::new();
    let mut fuzzy_matches = Vec::new();
    
    for (_, (element_ui, element_info)) in all_elements.iter().enumerate() {
        // Check for exact path match first (highest priority)
        if element_info.path == element_path {
            exact_matches.push((element_ui.clone(), element_info, "exact path"));
            continue;
        }
        
        // If we have a role-qualified search, check role + identifier match
       if let Some(role) = target_role {
            if element_info.role == role {
                // Role matches, now check if identifier matches description, title, or path suffix
                if element_info.description == target_identifier ||
                   element_info.title == target_identifier ||
                   element_info.path.ends_with(&format!(":{}", target_identifier)) {
                    exact_matches.push((element_ui.clone(), element_info, "role + identifier"));
                    continue;
                }
                

                // Then check for exact description/title match (not just contains)
                if element_info.description == target_identifier ||
                   element_info.title == target_identifier {
                    exact_matches.push((element_ui.clone(), element_info, "role + identifier"));
                    continue;
                }
            }
        }
        
        // Fuzzy matching (lower priority) - only consider if no role specified
        if target_role.is_none() {
            if element_info.path.contains(target_identifier) ||
               element_info.description == target_identifier ||
               element_info.title == target_identifier {
                fuzzy_matches.push((element_ui.clone(), element_info, "fuzzy"));
            }
        }
    }
    
    // Return the best match
    if !exact_matches.is_empty() {
        let (element_ui, _, _) = &exact_matches[0];
        return Some(element_ui.clone());
    }
    
    if !fuzzy_matches.is_empty() {
        let (element_ui, element_info, _) = &fuzzy_matches[0];
        warn!("Using fuzzy match for element path '{}': role='{}', desc='{}'", 
              element_path, element_info.role, element_info.description);
        return Some(element_ui.clone());
    }
    
    warn!("No element found matching path: '{}'", element_path);
    None
}

/// Debug function to collect ALL elements in the accessibility tree (not just actionable ones)
/// This helps diagnose what elements exist and why they might not be detected as actionable
pub fn debug_all_elements_by_pid(pid: &str) -> Vec<ActionableElement> {
    info!("DEBUG: Getting ALL elements from application with PID: {}", pid);
    
    // Parse the PID to i32
    let pid = match pid.parse::<i32>() {
        Ok(pid) => pid,
        Err(e) => {
            info!("❌ Failed to parse PID: {} - Error: {}", pid, e);
            return Vec::new();
        }
    };
    
    // Get the application from the PID
    let application = AXUIElement::application(pid);
    
    // Get all windows from the application
    let windows: CFArray<AXUIElement> = match application.attribute(&AXAttribute::windows()) {
        Ok(windows) => windows,
        Err(_) => return Vec::new(),
    };
    
    let mut all_elements = Vec::new();
    
    // Process each window
    for window in windows.iter() {
        debug_collect_all_elements(&window, &mut all_elements, 0);
    }
    
    info!("DEBUG: Found {} total elements in accessibility tree", all_elements.len());
    
    all_elements
}

/// Helper function to collect ALL elements recursively with depth tracking
fn debug_collect_all_elements(element: &AXUIElement, elements: &mut Vec<ActionableElement>, depth: usize) {
    // Limit depth to prevent infinite loops
    if depth > 50 {
        warn!("DEBUG: Reached maximum depth of 50, stopping traversal");
        return;
    }
    
    // Track counters for consistent path generation
    let mut role_counters: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    debug_collect_all_elements_with_counters(element, elements, depth, &mut role_counters);
}

/// Helper function for debug collection with role counters
fn debug_collect_all_elements_with_counters(
    element: &AXUIElement, 
    elements: &mut Vec<ActionableElement>, 
    depth: usize,
    role_counters: &mut std::collections::HashMap<String, usize>
) {
    if depth > 50 {
        return;
    }
    
    // Create element info for every element we encounter
    let enhanced_id = get_enhanced_element_identifier(element, role_counters);
    let path = format!("depth_{}_{}", depth, enhanced_id);
    
    if let Some(element_info) = create_actionable_element(element, path) {
        // Filter out empty AXGroup elements even in debug mode for cleaner output
        let is_empty_group = element_info.role.contains("AXGroup") && 
                            element_info.title.is_empty() && 
                            element_info.description.is_empty() &&
                            element_info.identifier.is_empty();
        
        if !is_empty_group {
            elements.push(element_info);
        }
    }
    
    // Add children to continue traversal
    if let Ok(children) = element.attribute(&AXAttribute::children()) {
        for child in children.iter() {
            debug_collect_all_elements_with_counters(&child, elements, depth + 1, role_counters);
        }
    }
}

/// High-value actionable roles to include even without labels
fn is_high_value_actionable(role: &str) -> bool {
    role.contains("AXTextArea") ||
    role.contains("AXTextField") ||
    role.contains("AXSearchField") ||
    role.contains("AXComboBox")
}

/// A container whose subtree is worth exploring deeper and earlier (e.g., web content)
fn is_priority_container(role: &str) -> bool {
    role.contains("AXWebArea") ||
    role.contains("AXScrollArea") ||
    role.contains("AXLayout") ||
    role.contains("AXGenericContainer")
}