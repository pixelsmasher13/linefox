#![cfg(any(target_os = "windows"))]

use std::env;
use log::info;

pub fn append_to_path(new_path: &str) {
    let current_path = env::var("PATH").unwrap_or_default();
    
    // Check if path already exists
    if !current_path.contains(new_path) {
        let new_full_path = format!("{};{}", current_path, new_path);
        env::set_var("PATH", new_full_path);
        info!("Added {} to PATH", new_path);
    } else {
        info!("{} already in PATH", new_path);
    }
}