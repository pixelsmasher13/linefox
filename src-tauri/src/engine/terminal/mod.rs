pub mod permissions;
pub mod executor;
pub mod claude_cli_parser;

// Re-export for initialization
pub use executor::set_app_handle;
