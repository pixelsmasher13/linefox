use active_win_pos_rs::ActiveWindow;
use log::info;
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl, class};

/// Pre-check that `NSWorkspace.sharedWorkspace.frontmostApplication` is non-nil.
///
/// The `active-win-pos-rs` crate panics with abort (non-unwinding) when
/// `frontmostApplication` returns nil, which happens during app transitions,
/// screen-saver activation, or when a process is terminating.
/// `catch_unwind` cannot save us from an abort, so we validate first.
#[cfg(target_os = "macos")]
fn frontmost_application_exists() -> bool {
    unsafe {
        let cls = match objc::runtime::Class::get("NSWorkspace") {
            Some(c) => c,
            None => return false,
        };
        let workspace: *mut objc::runtime::Object = objc::msg_send![cls, sharedWorkspace];
        if workspace.is_null() { return false; }
        let app: *mut objc::runtime::Object = objc::msg_send![workspace, frontmostApplication];
        !app.is_null()
    }
}

#[cfg(not(target_os = "macos"))]
fn frontmost_application_exists() -> bool {
    true // Not applicable on non-macOS; always proceed
}

/// Safely get the active window, with a nil pre-check on macOS followed by catch_unwind.
/// `active_win_pos_rs::get_active_window()` can abort with a null pointer
/// dereference when `NSWorkspace.frontmostApplication` is nil.
pub fn get_active_window() -> ActiveWindow {
    if !frontmost_application_exists() {
        info!("get_active_window: frontmostApplication is nil, returning default");
        return ActiveWindow::default();
    }

    let result = std::panic::catch_unwind(|| {
        active_win_pos_rs::get_active_window()
    });
    match result {
        Ok(Ok(active_window)) => active_window,
        Ok(Err(())) => {
            info!("error occurred while getting the active window");
            ActiveWindow::default()
        }
        Err(_) => {
            info!("get_active_window panicked (likely null pointer from unusual app like Screen Saver)");
            ActiveWindow::default()
        }
    }
}

#[allow(dead_code)]
pub fn get_active_window_details() -> Result<ActiveWindow, String> {
    if !frontmost_application_exists() {
        return Err("frontmostApplication is nil (no active app)".to_string());
    }

    let result = std::panic::catch_unwind(|| {
        active_win_pos_rs::get_active_window()
    });
    match result {
        Ok(Ok(active_window)) => Ok(active_window),
        Ok(Err(())) => Err("Failed to get active window details".to_string()),
        Err(_) => Err("get_active_window panicked (null pointer from unusual app)".to_string()),
    }
}
