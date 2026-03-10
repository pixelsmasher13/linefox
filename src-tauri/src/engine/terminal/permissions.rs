use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use once_cell::sync::Lazy;

// ===== SECURITY MODES =====

/// Security mode for terminal execution
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TerminalSecurityMode {
    /// Block all terminal commands
    Deny,
    /// Only commands on allowlist
    Allowlist,
    /// Ask for every command
    AskAll,
    /// Allow everything (requires user confirmation at start)
    Full,
}

impl Default for TerminalSecurityMode {
    fn default() -> Self {
        Self::Allowlist
    }
}

/// When command isn't on allowlist
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AskMode {
    /// Don't ask, just deny
    Off,
    /// Ask when not in allowlist
    OnMiss,
    /// Ask for every command
    Always,
}

impl Default for AskMode {
    fn default() -> Self {
        Self::OnMiss
    }
}

/// User's decision when prompted
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    /// Run this time only
    AllowOnce,
    /// Add to allowlist
    AllowAlways,
    /// Allow for this session only
    AllowSession,
    /// Block
    Deny,
}

// ===== ALLOWLIST ENTRY =====

/// Allowlist entry with pattern matching
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AllowlistEntry {
    /// Pattern to match: "git", "/usr/bin/npm", "~/.cargo/bin/*"
    pub pattern: String,
    /// When this entry was added
    pub added_at: i64,
    /// Last time this pattern was used
    pub last_used: Option<i64>,
    /// How many times this pattern has been used
    pub use_count: u32,
    /// Optional description of why this is allowed
    pub description: Option<String>,
}

impl AllowlistEntry {
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            added_at: chrono::Utc::now().timestamp_millis(),
            last_used: None,
            use_count: 0,
            description: None,
        }
    }

    pub fn with_description(pattern: &str, description: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            added_at: chrono::Utc::now().timestamp_millis(),
            last_used: None,
            use_count: 0,
            description: Some(description.to_string()),
        }
    }

    /// Check if a command matches this pattern
    pub fn matches(&self, executable: &str, resolved_path: Option<&str>) -> bool {
        let pattern = self.pattern.trim();
        if pattern.is_empty() {
            return false;
        }

        // Expand ~ in pattern
        let expanded_pattern = if pattern.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&pattern[2..]).to_string_lossy().to_string()
            } else {
                pattern.to_string()
            }
        } else {
            pattern.to_string()
        };

        // Check if pattern contains path separators (full path pattern)
        let is_path_pattern = expanded_pattern.contains('/');

        if is_path_pattern {
            // Match against resolved path or executable
            let target = resolved_path.unwrap_or(executable);
            self.glob_match(&expanded_pattern, target)
        } else {
            // Simple executable name match
            let exe_name = std::path::Path::new(executable)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(executable);
            self.glob_match(&expanded_pattern, exe_name)
        }
    }

    /// Simple glob matching (* and **)
    fn glob_match(&self, pattern: &str, target: &str) -> bool {
        let pattern_lower = pattern.to_lowercase();
        let target_lower = target.to_lowercase();

        // Handle ** (matches anything including /)
        if pattern_lower.contains("**") {
            let parts: Vec<&str> = pattern_lower.split("**").collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                return target_lower.starts_with(prefix) && target_lower.ends_with(suffix);
            }
        }

        // Handle * (matches anything except /)
        if pattern_lower.contains('*') {
            let parts: Vec<&str> = pattern_lower.split('*').collect();
            let mut pos = 0;
            for part in parts {
                if part.is_empty() {
                    continue;
                }
                if let Some(found) = target_lower[pos..].find(part) {
                    pos += found + part.len();
                } else {
                    return false;
                }
            }
            return true;
        }

        // Exact match
        pattern_lower == target_lower
    }

    /// Record a use of this pattern
    pub fn record_use(&mut self) {
        self.last_used = Some(chrono::Utc::now().timestamp_millis());
        self.use_count += 1;
    }
}

// ===== PERMISSIONS CONFIG =====

/// Permission config stored in ~/.linefox/permissions.json
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TerminalPermissions {
    /// Version for config migration
    pub version: u32,
    /// Security mode
    pub security_mode: TerminalSecurityMode,
    /// Ask mode
    pub ask_mode: AskMode,
    /// Allowed executables/patterns
    pub allowlist: Vec<AllowlistEntry>,
    /// Always block these patterns (dangerous commands)
    pub blocklist: Vec<String>,
    /// Default working directory for commands (e.g., ~/Linefox)
    /// If None, falls back to ~/Linefox
    #[serde(default)]
    pub default_cwd: Option<String>,
    /// Session-only allowlist (cleared on restart)
    #[serde(skip)]
    pub session_allowlist: HashSet<String>,
}

impl Default for TerminalPermissions {
    fn default() -> Self {
        Self {
            version: 1,
            security_mode: TerminalSecurityMode::Allowlist,
            ask_mode: AskMode::OnMiss,
            allowlist: Self::default_allowlist(),
            blocklist: Self::default_blocklist(),
            default_cwd: None, // Falls back to ~/Linefox if not set
            session_allowlist: HashSet::new(),
        }
    }
}

impl TerminalPermissions {
    /// Default safe commands that are pre-approved
    fn default_allowlist() -> Vec<AllowlistEntry> {
        vec![
            // Read-only file operations
            AllowlistEntry::with_description("ls", "List directory contents"),
            AllowlistEntry::with_description("pwd", "Print working directory"),
            AllowlistEntry::with_description("cat", "Display file contents"),
            AllowlistEntry::with_description("head", "Display first lines of file"),
            AllowlistEntry::with_description("tail", "Display last lines of file"),
            AllowlistEntry::with_description("less", "View file contents"),
            AllowlistEntry::with_description("more", "View file contents"),
            AllowlistEntry::with_description("wc", "Word/line count"),
            AllowlistEntry::with_description("file", "Determine file type"),
            AllowlistEntry::with_description("stat", "File statistics"),
            AllowlistEntry::with_description("find", "Find files"),
            AllowlistEntry::with_description("locate", "Locate files"),
            AllowlistEntry::with_description("which", "Locate a command"),
            AllowlistEntry::with_description("whereis", "Locate binary/source/manual"),
            AllowlistEntry::with_description("type", "Display command type"),
            
            // Text processing (read-only)
            AllowlistEntry::with_description("grep", "Search text patterns"),
            AllowlistEntry::with_description("awk", "Text processing"),
            AllowlistEntry::with_description("sed", "Stream editor"),
            AllowlistEntry::with_description("sort", "Sort lines"),
            AllowlistEntry::with_description("uniq", "Filter duplicate lines"),
            AllowlistEntry::with_description("cut", "Cut sections from lines"),
            AllowlistEntry::with_description("tr", "Translate characters"),
            AllowlistEntry::with_description("diff", "Compare files"),
            AllowlistEntry::with_description("cmp", "Compare files byte by byte"),
            
            // System info (read-only)
            AllowlistEntry::with_description("echo", "Print text"),
            AllowlistEntry::with_description("date", "Display date/time"),
            AllowlistEntry::with_description("cal", "Display calendar"),
            AllowlistEntry::with_description("uname", "System information"),
            AllowlistEntry::with_description("hostname", "Show hostname"),
            AllowlistEntry::with_description("whoami", "Show current user"),
            AllowlistEntry::with_description("id", "Show user/group IDs"),
            AllowlistEntry::with_description("env", "Show environment"),
            AllowlistEntry::with_description("printenv", "Print environment variables"),
            AllowlistEntry::with_description("df", "Disk space usage"),
            AllowlistEntry::with_description("du", "Directory space usage"),
            AllowlistEntry::with_description("free", "Memory usage"),
            AllowlistEntry::with_description("top", "Process monitor"),
            AllowlistEntry::with_description("htop", "Interactive process viewer"),
            AllowlistEntry::with_description("ps", "Process status"),
            AllowlistEntry::with_description("uptime", "System uptime"),
            
            // Git (version control)
            AllowlistEntry::with_description("git", "Version control"),
            
            // Common dev tools
            AllowlistEntry::with_description("node", "Node.js runtime"),
            AllowlistEntry::with_description("npm", "Node package manager"),
            AllowlistEntry::with_description("npx", "Node package executor"),
            AllowlistEntry::with_description("yarn", "Yarn package manager"),
            AllowlistEntry::with_description("pnpm", "pnpm package manager"),
            AllowlistEntry::with_description("bun", "Bun runtime"),
            AllowlistEntry::with_description("python", "Python interpreter"),
            AllowlistEntry::with_description("python3", "Python 3 interpreter"),
            AllowlistEntry::with_description("pip", "Python package manager"),
            AllowlistEntry::with_description("pip3", "Python 3 package manager"),
            AllowlistEntry::with_description("cargo", "Rust package manager"),
            AllowlistEntry::with_description("rustc", "Rust compiler"),
            AllowlistEntry::with_description("go", "Go compiler"),
            AllowlistEntry::with_description("make", "Build automation"),
            AllowlistEntry::with_description("cmake", "Build system generator"),
            
            // AI coding assistants
            AllowlistEntry::with_description("claude", "Claude CLI - AI coding assistant"),
            AllowlistEntry::with_description("codex", "OpenAI Codex CLI"),
            AllowlistEntry::with_description("aider", "Aider - AI pair programming"),
            AllowlistEntry::with_description("copilot", "GitHub Copilot CLI"),
            
            // Shells (for subcommands)
            AllowlistEntry::with_description("bash", "Bash shell"),
            AllowlistEntry::with_description("sh", "Shell"),
            AllowlistEntry::with_description("zsh", "Z shell"),
            
            // Network (read-only)
            AllowlistEntry::with_description("ping", "Test network connectivity"),
            AllowlistEntry::with_description("nslookup", "DNS lookup"),
            AllowlistEntry::with_description("dig", "DNS lookup"),
            AllowlistEntry::with_description("host", "DNS lookup"),
            
            // Common file/directory operations
            AllowlistEntry::with_description("cd", "Change directory"),
            AllowlistEntry::with_description("mkdir", "Create directory"),
            AllowlistEntry::with_description("touch", "Create empty file"),
            AllowlistEntry::with_description("cp", "Copy files"),
            AllowlistEntry::with_description("mv", "Move files"),
            AllowlistEntry::with_description("rm", "Remove files"),

            // Compression (often needed for package operations)
            AllowlistEntry::with_description("tar", "Archive utility"),
            AllowlistEntry::with_description("gzip", "Compression"),
            AllowlistEntry::with_description("gunzip", "Decompression"),
            AllowlistEntry::with_description("zip", "Compression"),
            AllowlistEntry::with_description("unzip", "Decompression"),
        ]
    }

    /// Default dangerous patterns that are always blocked
    fn default_blocklist() -> Vec<String> {
        vec![
            // Destructive file operations
            "rm -rf /".to_string(),
            "rm -rf ~".to_string(),
            "rm -rf /*".to_string(),
            "rm -rf ~/*".to_string(),
            "rm -rf $HOME".to_string(),
            "sudo rm -rf".to_string(),
            
            // Disk operations
            "mkfs".to_string(),
            "dd if=".to_string(),
            "> /dev/sd".to_string(),
            "> /dev/nvme".to_string(),
            
            // Fork bomb
            ":(){ :|:& };:".to_string(),
            
            // History manipulation
            "history -c".to_string(),
            
            // Dangerous redirects
            "> /etc/passwd".to_string(),
            "> /etc/shadow".to_string(),
            
            // Curl/wget to shell (code execution from internet)
            "curl * | sh".to_string(),
            "curl * | bash".to_string(),
            "wget * | sh".to_string(),
            "wget * | bash".to_string(),
            
            // Chmod dangerous patterns
            "chmod -R 777 /".to_string(),
            "chmod 777 /".to_string(),
            
            // Chown root
            "chown -R root /".to_string(),
        ]
    }

    /// Get the config file path
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".linefox");
        config_dir.join("terminal-permissions.json")
    }

    /// Load permissions from disk
    pub fn load() -> Self {
        let path = Self::config_path();
        
        if let Ok(contents) = fs::read_to_string(&path) {
            match serde_json::from_str::<TerminalPermissions>(&contents) {
                Ok(mut perms) => {
                    info!("Loaded terminal permissions from {:?}", path);
                    // Ensure session_allowlist is empty on load
                    perms.session_allowlist = HashSet::new();
                    return perms;
                }
                Err(e) => {
                    warn!("Failed to parse terminal permissions: {}", e);
                }
            }
        }
        
        info!("Using default terminal permissions");
        Self::default()
    }

    /// Save permissions to disk
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        
        // Create directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
        
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize permissions: {}", e))?;
        
        fs::write(&path, json)
            .map_err(|e| format!("Failed to write permissions: {}", e))?;
        
        info!("Saved terminal permissions to {:?}", path);
        Ok(())
    }

    /// Check if a command is blocked
    pub fn is_blocked(&self, command: &str) -> bool {
        let command_lower = command.to_lowercase();
        
        for pattern in &self.blocklist {
            let pattern_lower = pattern.to_lowercase();
            
            // Simple contains check for blocklist patterns
            if pattern_lower.contains('*') {
                // Handle glob patterns
                let parts: Vec<&str> = pattern_lower.split('*').collect();
                let mut matches = true;
                let mut search_start = 0;
                
                for part in parts {
                    if part.is_empty() {
                        continue;
                    }
                    if let Some(pos) = command_lower[search_start..].find(part) {
                        search_start += pos + part.len();
                    } else {
                        matches = false;
                        break;
                    }
                }
                
                if matches {
                    warn!("Command blocked by pattern '{}': {}", pattern, command);
                    return true;
                }
            } else if command_lower.contains(&pattern_lower) {
                warn!("Command blocked by pattern '{}': {}", pattern, command);
                return true;
            }
        }
        
        false
    }

    /// Check if a command is allowed
    pub fn is_allowed(&self, executable: &str, resolved_path: Option<&str>, full_command: &str) -> bool {
        // First check blocklist (always takes precedence)
        if self.is_blocked(full_command) {
            return false;
        }

        // Check security mode
        match self.security_mode {
            TerminalSecurityMode::Deny => false,
            TerminalSecurityMode::Full => true,
            TerminalSecurityMode::AskAll => false, // Will trigger ask
            TerminalSecurityMode::Allowlist => {
                // Check session allowlist first
                if self.session_allowlist.contains(executable) {
                    return true;
                }
                
                // Check permanent allowlist
                for entry in &self.allowlist {
                    if entry.matches(executable, resolved_path) {
                        return true;
                    }
                }
                
                false
            }
        }
    }

    /// Add a command to the permanent allowlist
    pub fn add_to_allowlist(&mut self, executable: &str, description: Option<&str>) {
        // Check if already exists
        if self.allowlist.iter().any(|e| e.pattern == executable) {
            return;
        }
        
        let entry = if let Some(desc) = description {
            AllowlistEntry::with_description(executable, desc)
        } else {
            AllowlistEntry::new(executable)
        };
        
        self.allowlist.push(entry);
        info!("Added '{}' to terminal allowlist", executable);
        
        // Auto-save
        if let Err(e) = self.save() {
            error!("Failed to save allowlist: {}", e);
        }
    }

    /// Add a command to the session allowlist
    pub fn add_to_session_allowlist(&mut self, executable: &str) {
        self.session_allowlist.insert(executable.to_string());
        info!("Added '{}' to session allowlist", executable);
    }

    /// Record usage of an allowlist entry
    pub fn record_usage(&mut self, executable: &str, resolved_path: Option<&str>) {
        for entry in &mut self.allowlist {
            if entry.matches(executable, resolved_path) {
                entry.record_use();
                // Don't save on every usage - too expensive
                return;
            }
        }
    }

    /// Should we ask the user for this command?
    pub fn should_ask(&self, executable: &str, resolved_path: Option<&str>, full_command: &str) -> bool {
        // Never ask for blocked commands - just deny
        if self.is_blocked(full_command) {
            return false;
        }

        match self.security_mode {
            TerminalSecurityMode::Deny => false,
            TerminalSecurityMode::Full => false,
            TerminalSecurityMode::AskAll => true,
            TerminalSecurityMode::Allowlist => {
                match self.ask_mode {
                    AskMode::Off => false,
                    AskMode::Always => true,
                    AskMode::OnMiss => !self.is_allowed(executable, resolved_path, full_command),
                }
            }
        }
    }
}

// ===== GLOBAL PERMISSIONS STATE =====

static PERMISSIONS: Lazy<RwLock<TerminalPermissions>> = Lazy::new(|| {
    RwLock::new(TerminalPermissions::load())
});

/// Get a reference to the global permissions
pub fn get_permissions() -> std::sync::RwLockReadGuard<'static, TerminalPermissions> {
    PERMISSIONS.read().unwrap()
}

/// Get a mutable reference to the global permissions
pub fn get_permissions_mut() -> std::sync::RwLockWriteGuard<'static, TerminalPermissions> {
    PERMISSIONS.write().unwrap()
}

/// Reload permissions from disk
#[allow(dead_code)]
pub fn reload_permissions() {
    let mut perms = PERMISSIONS.write().unwrap();
    *perms = TerminalPermissions::load();
}

// ===== COMMAND RESOLUTION =====

/// Resolved command information
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ResolvedCommand {
    /// The raw executable name (e.g., "git")
    pub executable: String,
    /// Full path to the executable (e.g., "/usr/bin/git")
    pub resolved_path: Option<String>,
    /// The arguments
    pub args: Vec<String>,
    /// The full command string
    pub full_command: String,
    /// Working directory
    pub cwd: Option<String>,
}

impl ResolvedCommand {
    /// Parse a command string into a resolved command.
    /// If the command starts with `cd <dir> && <rest>` or `cd <dir> ; <rest>`,
    /// the executable is extracted from <rest> so the allowlist check applies to
    /// the actual command, not to `cd`. The full_command is kept intact for
    /// blocklist checks (which operate on the whole string).
    pub fn parse(command: &str, cwd: Option<&str>) -> Self {
        let cmd = command.trim();

        // Unwrap `cd <dir> && <rest>` / `cd <dir> ; <rest>` prefix.
        // The LLM frequently emits `cd ~/project && npm run dev` — without
        // unwrapping, the permission check sees "cd" as the executable and
        // either triggers a spurious popup or auto-allows without checking the
        // real command.
        let effective_cmd = if cmd.starts_with("cd ") {
            // Find the first && or ; separator after cd
            let after_cd = &cmd[3..];
            if let Some(pos) = after_cd.find("&&") {
                after_cd[pos + 2..].trim()
            } else if let Some(pos) = after_cd.find(';') {
                after_cd[pos + 1..].trim()
            } else {
                cmd // bare `cd /some/dir` — keep as-is
            }
        } else {
            cmd
        };

        let parts: Vec<&str> = effective_cmd.split_whitespace().collect();
        let executable = parts.first().map(|s| s.to_string()).unwrap_or_default();
        let args: Vec<String> = parts.iter().skip(1).map(|s| s.to_string()).collect();

        // Try to resolve the executable path
        let resolved_path = Self::resolve_executable(&executable);

        Self {
            executable,
            resolved_path,
            args,
            full_command: command.to_string(),
            cwd: cwd.map(|s| s.to_string()),
        }
    }

    /// Resolve an executable to its full path
    fn resolve_executable(executable: &str) -> Option<String> {
        // If it's already a path, check if it exists
        if executable.contains('/') {
            let path = PathBuf::from(executable);
            if path.exists() && path.is_file() {
                return Some(path.to_string_lossy().to_string());
            }
            return None;
        }

        // Search in PATH
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in path_var.split(':') {
                let full_path = PathBuf::from(dir).join(executable);
                if full_path.exists() && full_path.is_file() {
                    return Some(full_path.to_string_lossy().to_string());
                }
            }
        }

        // macOS specific paths
        #[cfg(target_os = "macos")]
        {
            let additional_paths = [
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin",
            ];
            
            for dir in additional_paths {
                let full_path = PathBuf::from(dir).join(executable);
                if full_path.exists() && full_path.is_file() {
                    return Some(full_path.to_string_lossy().to_string());
                }
            }
        }

        None
    }
}

// ===== APPROVAL REQUEST =====

/// A request for user approval
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApprovalRequest {
    /// Unique request ID
    pub request_id: String,
    /// The command to run
    pub command: String,
    /// The executable name
    pub executable: String,
    /// Resolved executable path
    pub resolved_path: Option<String>,
    /// Working directory
    pub working_dir: Option<String>,
    /// Timestamp of request
    pub timestamp: i64,
    /// Why this command is being run
    pub reason: Option<String>,
}

impl ApprovalRequest {
    pub fn new(resolved: &ResolvedCommand, reason: Option<&str>) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            command: resolved.full_command.clone(),
            executable: resolved.executable.clone(),
            resolved_path: resolved.resolved_path.clone(),
            working_dir: resolved.cwd.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            reason: reason.map(|s| s.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowlist_matching() {
        let entry = AllowlistEntry::new("git");
        assert!(entry.matches("git", None));
        assert!(entry.matches("git", Some("/usr/bin/git")));
        assert!(!entry.matches("gitk", None));
    }

    #[test]
    fn test_glob_matching() {
        let entry = AllowlistEntry::new("node*");
        assert!(entry.matches("node", None));
        assert!(entry.matches("nodemon", None));
        assert!(entry.matches("node-gyp", None));
        assert!(!entry.matches("anode", None));
    }

    #[test]
    fn test_path_matching() {
        let entry = AllowlistEntry::new("/usr/bin/*");
        assert!(entry.matches("git", Some("/usr/bin/git")));
        assert!(!entry.matches("git", Some("/usr/local/bin/git")));
    }

    #[test]
    fn test_blocklist() {
        let perms = TerminalPermissions::default();
        assert!(perms.is_blocked("rm -rf /"));
        assert!(perms.is_blocked("sudo rm -rf /home"));
        assert!(!perms.is_blocked("rm file.txt"));
    }

    #[test]
    fn test_default_allowlist() {
        let perms = TerminalPermissions::default();
        assert!(perms.is_allowed("git", Some("/usr/bin/git"), "git status"));
        assert!(perms.is_allowed("ls", Some("/bin/ls"), "ls -la"));
        assert!(perms.is_allowed("npm", Some("/usr/local/bin/npm"), "npm install"));
    }
}
