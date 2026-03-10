//! App-specific commands module
//!
//! This module provides app-specific commands that are more reliable than UI clicking.
//! For example, Excel on macOS can use AppleScript for direct cell manipulation.

#[cfg(target_os = "macos")]
pub mod excel_macos;

#[cfg(target_os = "macos")]
pub mod word_macos;

/// Get app-specific commands documentation for the given app name.
/// Returns None if no special commands are available for this app.
pub fn get_app_specific_commands(app_name: &str) -> Option<String> {
    let app_lower = app_name.to_lowercase();

    #[cfg(target_os = "macos")]
    {
        if app_lower.contains("excel") || app_lower.contains("microsoft excel") {
            return Some(excel_macos::get_excel_commands_prompt());
        }
        if app_lower.contains("word") || app_lower.contains("microsoft word") {
            return Some(word_macos::get_word_commands_prompt());
        }
    }

    // No app-specific commands available
    None
}

/// Check if an app has specific commands available
pub fn has_app_specific_commands(app_name: &str) -> bool {
    get_app_specific_commands(app_name).is_some()
}
