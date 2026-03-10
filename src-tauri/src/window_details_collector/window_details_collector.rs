#[cfg(target_os = "macos")]
use crate::window_details_collector::macos::macos_accessibility_engine::get_element_tree_by_pid;

#[cfg(target_os = "windows")]
use crate::window_details_collector::windows::windows_nvda_bridge;

#[cfg(target_os = "linux")]
use crate::window_details_collector::linux::window_details_collector_linux;

#[cfg(target_os = "macos")]
pub fn get_element_tree_by_window_app_name(pid: &str) -> (String, String) {
    get_element_tree_by_pid(pid)
}

#[cfg(target_os = "windows")]
pub fn get_element_tree_by_window_app_name(pid: &str) -> (String, String) {
    let tree = windows_nvda_bridge::get_element_tree_by_pid(pid);
    (String::new(), tree)
}

#[cfg(target_os = "linux")]
pub fn get_element_tree_by_window_app_name(_pid: &str) -> (String, String) {
    (String::from(""), String::from(""))
}