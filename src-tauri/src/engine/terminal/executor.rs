use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{RwLock, oneshot};
use once_cell::sync::Lazy;
use tauri::Emitter;

use super::permissions::{
    ApprovalDecision, ApprovalRequest, ResolvedCommand,
    get_permissions, get_permissions_mut,
};
use super::claude_cli_parser::{
    ClaudeStreamState, ClaudeStreamOutput, ClaudeOutputType,
    is_claude_cli_command, parse_claude_jsonl_line, is_claude_jsonl,
};

// Track whether a Claude CLI session has been started in this execution.
// When true, subsequent Claude CLI commands will auto-get --continue if missing.
static CLAUDE_CLI_SESSION_ACTIVE: Lazy<Arc<std::sync::Mutex<bool>>> =
    Lazy::new(|| Arc::new(std::sync::Mutex::new(false)));

// Track the last working directory used by a terminal command.
// Persists across commands so subsequent TERMINAL_RUN calls inherit the CWD
// without the LLM needing to prefix every command with `cd <dir> &&`.
static LAST_TERMINAL_CWD: Lazy<Arc<std::sync::Mutex<Option<String>>>> =
    Lazy::new(|| Arc::new(std::sync::Mutex::new(None)));

/// Mark that a Claude CLI session has been started (call after successful Claude CLI execution)
fn mark_claude_cli_session_active() {
    if let Ok(mut active) = CLAUDE_CLI_SESSION_ACTIVE.lock() {
        *active = true;
        info!("Claude CLI session marked active (--continue will be auto-added to subsequent calls)");
    }
}

/// Reset Claude CLI session tracking (call when automation starts fresh)
pub fn reset_claude_cli_session() {
    if let Ok(mut active) = CLAUDE_CLI_SESSION_ACTIVE.lock() {
        *active = false;
    }
}

/// Reset terminal state (CWD tracking, etc.) for a new automation
pub fn reset_terminal_state() {
    if let Ok(mut cwd) = LAST_TERMINAL_CWD.lock() {
        *cwd = None;
    }
}

/// Check if a Claude CLI session is currently active
fn is_claude_cli_session_active() -> bool {
    CLAUDE_CLI_SESSION_ACTIVE.lock().map(|a| *a).unwrap_or(false)
}

/// Ensure Claude CLI command has the required flags for JSONL streaming output.
/// Also auto-adds --continue if a Claude CLI session is already active and the
/// command doesn't explicitly have --continue (prevents accidental session loss).
fn ensure_claude_cli_streaming_flags(command: &str) -> String {
    let cmd = command.trim();
    
    // If it already has --print and --output-format, leave it alone (only add --continue if needed)
    let mut result = if cmd.contains("--print") && cmd.contains("--output-format") {
        cmd.to_string()
    } else {
        // Parse the command to insert flags appropriately
        // Claude CLI format: claude [options] [prompt]
        let parts: Vec<&str> = cmd.splitn(2, char::is_whitespace).collect();
        
        if parts.is_empty() {
            return cmd.to_string();
        }
        
        let claude_cmd = parts[0]; // "claude"
        let rest = parts.get(1).unwrap_or(&"");
        
        // Build the flags we need
        let mut flags = Vec::new();
        
        // Add --print if not present
        if !cmd.contains("--print") && !cmd.contains("-p ") && !cmd.contains("-p\t") {
            flags.push("--print");
        }
        
        // Add --output-format stream-json if not present
        if !cmd.contains("--output-format") {
            flags.push("--output-format");
            flags.push("stream-json");
        }
        
        // Add --permission-mode bypassPermissions if not present so Claude CLI can operate
        // autonomously in headless (-p) mode. Without this, Claude CLI blocks on bash
        // commands since it can't prompt interactively. Linefox already gates commands
        // through its own terminal permission layer.
        if !cmd.contains("--permission-mode") {
            flags.push("--permission-mode");
            flags.push("bypassPermissions");
        }

        // Add --verbose if not present (helps with stream-json output)
        if !cmd.contains("--verbose") {
            flags.push("--verbose");
        }

        // Add --include-partial-messages for real-time streaming
        if !cmd.contains("--include-partial-messages") {
            flags.push("--include-partial-messages");
        }
        
        // Reconstruct the command
        if rest.is_empty() {
            format!("{} {}", claude_cmd, flags.join(" "))
        } else {
            // Check if rest starts with options or a prompt
            if rest.starts_with('-') || rest.starts_with("--") {
                format!("{} {} {}", claude_cmd, flags.join(" "), rest)
            } else {
                format!("{} {} {}", claude_cmd, flags.join(" "), rest)
            }
        }
    };
    
    // Auto-add --continue if a Claude CLI session is already active and command doesn't have it.
    // This prevents the LLM from accidentally starting a new session when it forgets --continue.
    if !result.contains("--continue") && is_claude_cli_session_active() {
        info!("Auto-adding --continue to Claude CLI command (session already active)");
        // Insert --continue right after "claude"
        if let Some(pos) = result.find("claude") {
            let insert_pos = pos + "claude".len();
            result.insert_str(insert_pos, " --continue");
        }
    }
    
    result
}

/// Extract the target directory from a command that starts with `cd <dir> && ...` or `cd <dir>; ...`
/// Returns None if the command doesn't start with cd.
fn extract_cd_directory(command: &str) -> Option<String> {
    let cmd = command.trim();
    if !cmd.starts_with("cd ") {
        return None;
    }
    // Extract the directory after "cd " and before "&&", ";", or end of string
    let after_cd = cmd[3..].trim();
    let dir = if let Some(pos) = after_cd.find("&&") {
        after_cd[..pos].trim()
    } else if let Some(pos) = after_cd.find(';') {
        after_cd[..pos].trim()
    } else {
        after_cd.trim()
    };
    if dir.is_empty() {
        None
    } else {
        // Strip surrounding quotes if present
        let dir = dir.trim_matches('"').trim_matches('\'');
        Some(dir.to_string())
    }
}

// ===== PROCESS STATE =====

/// State of a running process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessState {
    /// Unique process ID (internal)
    pub id: String,
    /// The command that was run
    pub command: String,
    /// OS process ID
    pub pid: Option<u32>,
    /// Whether the process is still running
    pub running: bool,
    /// Whether this is a background process
    pub background: bool,
    /// Exit code (if completed)
    pub exit_code: Option<i32>,
    /// Stdout output (buffered)
    pub stdout: String,
    /// Stderr output (buffered)
    pub stderr: String,
    /// Start time
    pub started_at: i64,
    /// End time (if completed)
    pub ended_at: Option<i64>,
    /// Working directory
    pub cwd: Option<String>,
}

/// Registry of running/completed processes
pub struct ProcessRegistry {
    processes: HashMap<String, ProcessState>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
        }
    }

    /// Register a new process
    pub fn register(&mut self, state: ProcessState) {
        self.processes.insert(state.id.clone(), state);
    }

    /// Update process state
    pub fn update(&mut self, id: &str, update: impl FnOnce(&mut ProcessState)) {
        if let Some(state) = self.processes.get_mut(id) {
            update(state);
        }
    }

    /// Get process state
    pub fn get(&self, id: &str) -> Option<&ProcessState> {
        self.processes.get(id)
    }

    /// Get all processes
    #[allow(dead_code)]
    pub fn all(&self) -> Vec<&ProcessState> {
        self.processes.values().collect()
    }

    /// Get running processes
    #[allow(dead_code)]
    pub fn running(&self) -> Vec<&ProcessState> {
        self.processes.values().filter(|p| p.running).collect()
    }

    /// Clean up old completed processes
    #[allow(dead_code)]
    pub fn cleanup_old(&mut self, max_age_secs: i64) {
        let now = chrono::Utc::now().timestamp();
        self.processes.retain(|_, state| {
            if state.running {
                return true;
            }
            if let Some(ended_at) = state.ended_at {
                let age = now - ended_at / 1000;
                age < max_age_secs
            } else {
                true
            }
        });
    }
}

// Global process registry
static REGISTRY: Lazy<Arc<RwLock<ProcessRegistry>>> = Lazy::new(|| {
    Arc::new(RwLock::new(ProcessRegistry::new()))
});

// Global pending approvals - separate from the registry for simplicity
static PENDING_APPROVALS: Lazy<Arc<RwLock<HashMap<String, oneshot::Sender<ApprovalDecision>>>>> = 
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Get the global process registry
pub async fn get_registry() -> tokio::sync::RwLockReadGuard<'static, ProcessRegistry> {
    REGISTRY.read().await
}

/// Get mutable access to the global process registry
pub async fn get_registry_mut() -> tokio::sync::RwLockWriteGuard<'static, ProcessRegistry> {
    REGISTRY.write().await
}

// ===== EXECUTION RESULT =====

/// Result of executing a terminal command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Process ID (for tracking)
    pub process_id: String,
    /// Whether execution was successful
    pub success: bool,
    /// Exit code (if completed)
    pub exit_code: Option<i32>,
    /// Stdout output
    pub stdout: String,
    /// Stderr output
    pub stderr: String,
    /// Whether the process is still running (background)
    pub running: bool,
    /// Error message if execution failed
    pub error: Option<String>,
    /// Whether approval was denied
    pub denied: bool,
    /// Clean parsed content from Claude CLI (thinking + response), if this was a Claude CLI command.
    /// Use this instead of stdout for memory storage since stdout contains raw JSONL.
    pub claude_content: Option<String>,
}

impl ExecutionResult {
    pub fn denied(reason: &str) -> Self {
        Self {
            process_id: String::new(),
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            running: false,
            error: Some(reason.to_string()),
            denied: true,
            claude_content: None,
        }
    }

    pub fn error(reason: &str) -> Self {
        Self {
            process_id: String::new(),
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            running: false,
            error: Some(reason.to_string()),
            denied: false,
            claude_content: None,
        }
    }
}

// ===== EXECUTION OPTIONS =====

/// Options for executing a command
#[derive(Debug, Clone, Default)]
pub struct ExecutionOptions {
    /// Working directory
    pub cwd: Option<String>,
    /// Environment variables to set
    pub env: Option<HashMap<String, String>>,
    /// Timeout in seconds
    pub timeout_secs: Option<u64>,
    /// Run in background
    pub background: bool,
    /// Reason for running this command (for approval UI)
    pub reason: Option<String>,
    /// Skip approval check (use with caution!)
    pub skip_approval: bool,
}

// ===== EXECUTOR =====

/// Permission check result
enum PermissionCheck {
    Allowed,
    Blocked(String),
    NeedsApproval,
    Denied(String),
}

/// Execute a terminal command with permission checking
pub async fn execute(command: &str, options: ExecutionOptions) -> ExecutionResult {
    info!("Terminal execute request: {}", command);
    
    // Parse and resolve the command
    let resolved = ResolvedCommand::parse(command, options.cwd.as_deref());
    
    // Check permissions (unless skipped) - do all sync checks first
    let check_result = if options.skip_approval {
        PermissionCheck::Allowed
    } else {
        // Scope the lock to avoid holding it across await
        let perms = get_permissions();
        
        // Check if blocked
        if perms.is_blocked(&resolved.full_command) {
            PermissionCheck::Blocked(format!(
                "Command '{}' is blocked for safety reasons",
                resolved.executable
            ))
        } else if perms.is_allowed(
            &resolved.executable,
            resolved.resolved_path.as_deref(),
            &resolved.full_command,
        ) {
            PermissionCheck::Allowed
        } else if perms.should_ask(
            &resolved.executable,
            resolved.resolved_path.as_deref(),
            &resolved.full_command,
        ) {
            PermissionCheck::NeedsApproval
        } else {
            let security_mode = format!("{:?}", perms.security_mode);
            PermissionCheck::Denied(format!(
                "Command '{}' is not in the allowlist. Security mode: {}",
                resolved.executable,
                security_mode
            ))
        }
        // perms lock is dropped here
    };
    
    // Handle the permission check result
    match check_result {
        PermissionCheck::Blocked(reason) => {
            warn!("Command blocked: {}", command);
            return ExecutionResult::denied(&reason);
        }
        PermissionCheck::Denied(reason) => {
            warn!("Command not allowed: {}", command);
            return ExecutionResult::denied(&reason);
        }
        PermissionCheck::NeedsApproval => {
            // Request approval - this is async
            match request_approval(&resolved, options.reason.as_deref()).await {
                Ok(decision) => {
                    match decision {
                        ApprovalDecision::AllowOnce => {
                            info!("Command approved (once): {}", command);
                        }
                        ApprovalDecision::AllowAlways => {
                            info!("Command approved (always): {}", command);
                            let mut perms = get_permissions_mut();
                            perms.add_to_allowlist(
                                &resolved.executable,
                                Some(&format!("Approved by user for: {}", command)),
                            );
                        }
                        ApprovalDecision::AllowSession => {
                            info!("Command approved (session): {}", command);
                            let mut perms = get_permissions_mut();
                            perms.add_to_session_allowlist(&resolved.executable);
                        }
                        ApprovalDecision::Deny => {
                            warn!("Command denied by user: {}", command);
                            return ExecutionResult::denied("Command denied by user");
                        }
                    }
                }
                Err(e) => {
                    error!("Approval request failed: {}", e);
                    return ExecutionResult::denied(&format!("Approval request failed: {}", e));
                }
            }
        }
        PermissionCheck::Allowed => {
            // Record usage
            let mut perms = get_permissions_mut();
            perms.record_usage(&resolved.executable, resolved.resolved_path.as_deref());
        }
    }
    
    // Execute the command
    run_command(&resolved, options).await
}

/// Request approval from the user via Tauri event
/// Note: This function requires an app_handle to be available via thread-local or global state
/// For now, we use a simplified approach that works with the async context
async fn request_approval(
    resolved: &ResolvedCommand,
    reason: Option<&str>,
) -> Result<ApprovalDecision, String> {
    let request = ApprovalRequest::new(resolved, reason);
    let request_id = request.request_id.clone();
    
    // Create response channel
    let (tx, rx) = oneshot::channel();
    
    // Register pending approval
    {
        let mut pending = PENDING_APPROVALS.write().await;
        pending.insert(request_id.clone(), tx);
    }
    
    // Log that we need approval (the automation engine will emit the event)
    info!("Terminal approval required for: {} (request_id: {})", resolved.full_command, request_id);
    info!("Approval request details: {:?}", request);
    
    // Store the request in a global so the automation engine can pick it up
    // This is a simpler pattern that doesn't require threading app_handle through
    {
        let mut req = CURRENT_APPROVAL_REQUEST.write().await;
        *req = Some(request);
    }
    
    // Wait for response with timeout (5 minutes — user may be away or reading the command)
    match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
        Ok(Ok(decision)) => {
            // Clear the current request
            let mut req = CURRENT_APPROVAL_REQUEST.write().await;
            *req = None;
            Ok(decision)
        },
        Ok(Err(_)) => {
            // Clean up
            let mut pending = PENDING_APPROVALS.write().await;
            pending.remove(&request_id);
            let mut req = CURRENT_APPROVAL_REQUEST.write().await;
            *req = None;
            Err("Approval channel closed".to_string())
        },
        Err(_) => {
            // Timeout - clean up
            let mut pending = PENDING_APPROVALS.write().await;
            pending.remove(&request_id);
            let mut req = CURRENT_APPROVAL_REQUEST.write().await;
            *req = None;
            Err("Approval request timed out (60 seconds)".to_string())
        }
    }
}

// Global storage for current approval request (for the automation engine to read)
static CURRENT_APPROVAL_REQUEST: Lazy<Arc<RwLock<Option<ApprovalRequest>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

/// Get the current pending approval request (called by automation engine to emit event)
pub async fn get_pending_approval_request() -> Option<ApprovalRequest> {
    let req = CURRENT_APPROVAL_REQUEST.read().await;
    req.clone()
}

/// Response from the UI for an approval request
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ApprovalResponse {
    Allow,
    AllowSession,
    Deny,
}

/// Resolve an approval request (called from UI via Tauri command)
pub async fn resolve_approval(request_id: &str, response: ApprovalResponse, allow_always: bool) {
    let decision = match (response, allow_always) {
        (ApprovalResponse::Allow, true) => ApprovalDecision::AllowAlways,
        (ApprovalResponse::Allow, false) => ApprovalDecision::AllowOnce,
        (ApprovalResponse::AllowSession, _) => ApprovalDecision::AllowSession,
        (ApprovalResponse::Deny, _) => ApprovalDecision::Deny,
    };
    
    let mut pending = PENDING_APPROVALS.write().await;
    if let Some(tx) = pending.remove(request_id) {
        info!("Resolved approval {} with decision: {:?}", request_id, decision);
        let _ = tx.send(decision);
    } else {
        warn!("No pending approval with ID: {}", request_id);
    }
}

/// Run a command (internal, after permission checks)
async fn run_command(resolved: &ResolvedCommand, options: ExecutionOptions) -> ExecutionResult {
    let process_id = uuid::Uuid::new_v4().to_string();
    
    // Check if this is a Claude CLI command (for special output handling)
    let is_claude_cli = is_claude_cli_command(&resolved.full_command);
    
    // Prepare the command - for Claude CLI, add flags for JSONL streaming output
    let actual_command = if is_claude_cli {
        ensure_claude_cli_streaming_flags(&resolved.full_command)
    } else {
        resolved.full_command.clone()
    };
    
    // Check if this command might need PTY-like behavior for real-time output.
    // Many tools suppress progress output when not connected to a TTY (e.g. git clone,
    // cargo build). Wrapping with `script` creates a pseudo-TTY so they show progress.
    // Note: Claude CLI with --print doesn't need PTY wrapping.
    let needs_pty = !is_claude_cli && (
        resolved.full_command.starts_with("npm ")
        || resolved.full_command.starts_with("yarn ")
        || resolved.full_command.starts_with("pnpm ")
        || resolved.full_command.starts_with("git ")
        || resolved.full_command.starts_with("cargo ")
        || resolved.full_command.starts_with("pip ")
        || resolved.full_command.starts_with("pip3 ")
        || resolved.full_command.starts_with("rustc ")
        || resolved.full_command.starts_with("make ")
    );
    
    // === DETAILED COMMAND LOGGING ===
    info!("┌─────────────────────────────────────────────────────────────");
    info!("│ TERMINAL COMMAND EXECUTION [{}]", &process_id[..8]);
    info!("├─────────────────────────────────────────────────────────────");
    info!("│ Original command: {}", resolved.full_command);
    if actual_command != resolved.full_command {
        info!("│ Transformed to:   {}", actual_command);
    }
    if is_claude_cli {
        info!("│ Mode: Claude CLI (JSONL streaming enabled)");
    }
    if needs_pty {
        info!("│ Mode: PTY wrapper enabled (for unbuffered output)");
    }
    
    // Build the command
    let mut cmd = if cfg!(target_os = "macos") {
        // Use shell for better PATH resolution
        let mut c = Command::new("/bin/zsh");
        c.arg("-c");
        
        // Wrap with `script` if we need PTY-like output (forces line buffering)
        if needs_pty {
            // script -q /dev/null runs the command with a pseudo-terminal
            // This forces programs to use line buffering instead of block buffering
            c.arg(format!("script -q /dev/null {}", &actual_command));
        } else {
            c.arg(&actual_command);
        }
        c
    } else if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.arg("/C");
        c.arg(&actual_command);
        c
    } else {
        // Linux - use script as well
        let mut c = Command::new("/bin/sh");
        c.arg("-c");
        if needs_pty {
            c.arg(format!("script -q -c '{}' /dev/null", &actual_command));
        } else {
            c.arg(&actual_command);
        }
        c
    };
    
    // Determine working directory
    // Priority: explicit options > resolved from command > last used CWD > config default > ~/Linefox
    let effective_cwd = options.cwd.clone()
        .or_else(|| resolved.cwd.clone())
        .or_else(|| {
            // Use the last CWD from a previous terminal command (persists across steps)
            LAST_TERMINAL_CWD.lock().ok().and_then(|cwd| cwd.clone())
        })
        .or_else(|| {
            let perms = get_permissions();
            perms.default_cwd.clone()
        })
        .or_else(|| {
            // Use ~/Linefox as default workspace - visible to user and avoids
            // macOS privacy prompts (not in Documents/Desktop/Downloads)
            dirs::home_dir().map(|home| {
                let workspace = home.join("Linefox");
                // Create the directory if it doesn't exist
                let _ = std::fs::create_dir_all(&workspace);
                workspace.to_string_lossy().to_string()
            })
        });
    
    // Remember this CWD for subsequent commands
    if let Some(ref cwd) = effective_cwd {
        if let Ok(mut last_cwd) = LAST_TERMINAL_CWD.lock() {
            *last_cwd = Some(cwd.clone());
        }
    }
    
    // If command starts with "cd <dir> && ..." or "cd <dir>;", extract and track the directory
    // so the next command inherits it even without explicit cd
    if let Some(cd_dir) = extract_cd_directory(&resolved.full_command) {
        let resolved_dir = if cd_dir.starts_with('/') || cd_dir.starts_with('~') {
            // Absolute path or home-relative
            let expanded = if cd_dir.starts_with('~') {
                dirs::home_dir()
                    .map(|h| cd_dir.replacen('~', &h.to_string_lossy(), 1))
                    .unwrap_or(cd_dir)
            } else {
                cd_dir
            };
            expanded
        } else if let Some(ref cwd) = effective_cwd {
            // Relative path - resolve against effective CWD
            format!("{}/{}", cwd, cd_dir)
        } else {
            cd_dir
        };
        if let Ok(mut last_cwd) = LAST_TERMINAL_CWD.lock() {
            info!("Tracking CWD change from command: {}", resolved_dir);
            *last_cwd = Some(resolved_dir);
        }
    }
    
    // Set working directory
    if let Some(cwd) = &effective_cwd {
        cmd.current_dir(cwd);
    }
    
    // Set environment variables for unbuffered output and non-interactive mode
    cmd.env("PYTHONUNBUFFERED", "1");       // Python: don't buffer output
    cmd.env("NODE_NO_WARNINGS", "1");       // Node.js: suppress warnings
    cmd.env("FORCE_COLOR", "1");            // Enable colors (many tools detect TTY for this)
    cmd.env("TERM", "xterm-256color");      // Pretend we're a terminal
    
    // Suppress interactive prompts - tools fail fast with clear errors instead of hanging
    cmd.env("CI", "true");                  // Widely recognized: npm/yarn/pip/many others skip prompts
    cmd.env("GIT_TERMINAL_PROMPT", "0");    // Git: don't prompt for credentials, fail immediately
    cmd.env("NPM_CONFIG_YES", "true");      // npm: auto-accept prompts
    cmd.env("PIP_NO_INPUT", "1");           // pip: skip prompts
    cmd.env("CARGO_TERM_PROGRESS_WHEN", "always"); // cargo: show progress even without TTY
    cmd.env("HOMEBREW_NO_AUTO_UPDATE", "1"); // brew: skip auto-update prompts
    
    // Set user-provided environment variables
    if let Some(env) = &options.env {
        for (key, value) in env {
            cmd.env(key, value);
        }
    }
    
    // Set up stdio
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());
    
    // === LOG THE EXACT EXECUTION DETAILS ===
    let shell_name = if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "/bin/sh"
    };
    
    info!("│ Working directory: {}", effective_cwd.as_deref().unwrap_or("<none>"));
    info!("│ Shell: {} -c \"...\"", shell_name);
    if let Some(env) = &options.env {
        if !env.is_empty() {
            info!("│ Custom env vars: {:?}", env.keys().collect::<Vec<_>>());
        }
    }
    info!("├─────────────────────────────────────────────────────────────");
    info!("│ COPY-PASTE TO REPRODUCE:");
    info!("│ cd \"{}\" && {}", 
        effective_cwd.as_deref().unwrap_or("~"),
        &actual_command
    );
    info!("└─────────────────────────────────────────────────────────────");
    
    // Spawn the process
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            error!("Failed to spawn process: {}", e);
            return ExecutionResult::error(&format!("Failed to spawn process: {}", e));
        }
    };
    
    let pid = child.id();
    
    // Register the process
    let initial_state = ProcessState {
        id: process_id.clone(),
        command: actual_command.clone(),
        pid,
        running: true,
        background: options.background,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        started_at: chrono::Utc::now().timestamp_millis(),
        ended_at: None,
        cwd: options.cwd.clone().or(resolved.cwd.clone()),
    };
    
    {
        let mut registry = get_registry_mut().await;
        registry.register(initial_state);
    }
    
    // Emit process started event for UI (show original command for cleaner UX)
    emit_process_started(&process_id, &resolved.full_command);
    
    if options.background {
        // Background execution - don't wait for completion
        tokio::spawn(monitor_process(process_id.clone(), child, options.timeout_secs));
        
        ExecutionResult {
            process_id,
            success: true,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            running: true,
            error: None,
            denied: false,
            claude_content: None,
        }
    } else {
        // Foreground execution - wait for completion
        wait_for_process(process_id, child, options.timeout_secs, is_claude_cli).await
    }
}

// Global app handle for emitting events (using std::sync::RwLock since emit functions are sync)
static APP_HANDLE: Lazy<Arc<std::sync::RwLock<Option<tauri::AppHandle>>>> =
    Lazy::new(|| Arc::new(std::sync::RwLock::new(None)));

/// Set the app handle for terminal output events
pub fn set_app_handle(handle: tauri::AppHandle) {
    if let Ok(mut guard) = APP_HANDLE.write() {
        *guard = Some(handle);
    }
}

/// Emit a terminal output line to the frontend
fn emit_terminal_output(process_id: &str, line: &str, stream: &str) {
    if let Ok(guard) = APP_HANDLE.read() {
        if let Some(handle) = guard.as_ref() {
            let _ = handle.emit("terminal-output", serde_json::json!({
                "process_id": process_id,
                "line": line,
                "stream": stream,
                "timestamp": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
}

/// Emit terminal process started event
fn emit_process_started(process_id: &str, command: &str) {
    if let Ok(guard) = APP_HANDLE.read() {
        if let Some(handle) = guard.as_ref() {
            let _ = handle.emit("terminal-process-started", serde_json::json!({
                "process_id": process_id,
                "command": command,
                "timestamp": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
}

/// Emit terminal process completed event
fn emit_process_completed(process_id: &str, exit_code: Option<i32>, success: bool) {
    if let Ok(guard) = APP_HANDLE.read() {
        if let Some(handle) = guard.as_ref() {
            let _ = handle.emit("terminal-process-completed", serde_json::json!({
                "process_id": process_id,
                "exit_code": exit_code,
                "success": success,
                "timestamp": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
}

// ===== CLAUDE CLI SPECIFIC EVENTS =====

/// Emit Claude CLI stream event (for structured streaming output)
fn emit_claude_stream(process_id: &str, output: &ClaudeStreamOutput) {
    if let Ok(guard) = APP_HANDLE.read() {
        if let Some(handle) = guard.as_ref() {
            let _ = handle.emit("claude-stream", serde_json::json!({
                "process_id": process_id,
                "output_type": output.output_type,
                "block_index": output.block_index,
                "content": output.content,
                "tool_name": output.tool_name,
                "tool_id": output.tool_id,
                "tool_input": output.tool_input,
                "is_delta": output.is_delta,
                "timestamp": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
}

/// Emit that this process is a Claude CLI process (for UI to switch rendering mode)
fn emit_claude_cli_detected(process_id: &str, command: &str) {
    if let Ok(guard) = APP_HANDLE.read() {
        if let Some(handle) = guard.as_ref() {
            let _ = handle.emit("claude-cli-detected", serde_json::json!({
                "process_id": process_id,
                "command": command,
                "timestamp": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
}

/// Emit Claude CLI thinking update (accumulated thinking text)
fn emit_claude_thinking(process_id: &str, thinking: &str, is_delta: bool) {
    if let Ok(guard) = APP_HANDLE.read() {
        if let Some(handle) = guard.as_ref() {
            let _ = handle.emit("claude-thinking", serde_json::json!({
                "process_id": process_id,
                "thinking": thinking,
                "is_delta": is_delta,
                "timestamp": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
}

/// Emit Claude CLI content update (accumulated response text)
fn emit_claude_content(process_id: &str, content: &str, is_delta: bool) {
    if let Ok(guard) = APP_HANDLE.read() {
        if let Some(handle) = guard.as_ref() {
            let _ = handle.emit("claude-content", serde_json::json!({
                "process_id": process_id,
                "content": content,
                "is_delta": is_delta,
                "timestamp": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
}

/// Emit Claude CLI tool use event
fn emit_claude_tool(
    process_id: &str,
    tool_id: &str,
    tool_name: &str,
    status: &str,
    input: Option<&serde_json::Value>,
    result: Option<&str>,
) {
    if let Ok(guard) = APP_HANDLE.read() {
        if let Some(handle) = guard.as_ref() {
            let _ = handle.emit("claude-tool", serde_json::json!({
                "process_id": process_id,
                "tool_id": tool_id,
                "tool_name": tool_name,
                "status": status,
                "input": input,
                "result": result,
                "timestamp": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
}

/// Wait for a process to complete with real-time output streaming
async fn wait_for_process(
    process_id: String,
    mut child: Child,
    timeout_secs: Option<u64>,
    is_claude_cli: bool,
) -> ExecutionResult {
    // Take ownership of stdout/stderr handles
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();
    
    // Shared state for collecting output
    let stdout_output = Arc::new(RwLock::new(String::new()));
    let stderr_output = Arc::new(RwLock::new(String::new()));
    
    // Spawn stdout reader task - reads chunks for real-time output
    let stdout_task = {
        let process_id = process_id.clone();
        let output = stdout_output.clone();
        tokio::spawn(async move {
            if let Some(stdout) = stdout_handle {
                let mut reader = BufReader::with_capacity(256, stdout); // Smaller buffer for faster output
                let mut line_buffer = String::new();
                let mut buf = [0u8; 256]; // Read in small chunks
                
                // Claude CLI state for parsing JSONL
                let mut claude_state = if is_claude_cli {
                    Some(ClaudeStreamState::new())
                } else {
                    None
                };
                let mut claude_detected_emitted = false;
                
                loop {
                    match reader.read(&mut buf).await {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let chunk = String::from_utf8_lossy(&buf[..n]);
                            
                            // Append to line buffer
                            line_buffer.push_str(&chunk);
                            
                            // Process complete lines
                            while let Some(newline_pos) = line_buffer.find('\n') {
                                let line = line_buffer[..newline_pos].to_string();
                                line_buffer = line_buffer[newline_pos + 1..].to_string();
                                
                                // Check if this is Claude CLI JSONL output
                                if let Some(ref mut state) = claude_state {
                                    if is_claude_jsonl(&line) {
                                        // Emit detection event once
                                        if !claude_detected_emitted {
                                            emit_claude_cli_detected(&process_id, "claude");
                                            claude_detected_emitted = true;
                                        }
                                        
                                        // Parse and process the JSONL event
                                        if let Some(event) = parse_claude_jsonl_line(&line) {
                                            if let Some(output) = state.process_event(&event) {
                                                // Emit the structured event
                                                emit_claude_stream(&process_id, &output);
                                                
                                                // Also emit specific events for easier frontend handling
                                                // NOTE: We only emit claude-stream now, NOT individual events
                                                // The frontend was listening to BOTH and duplicating content!
                                                match output.output_type {
                                                    ClaudeOutputType::ToolStart => {
                                                        if let (Some(tool_name), Some(tool_id)) = 
                                                            (&output.tool_name, &output.tool_id) 
                                                        {
                                                            emit_claude_tool(
                                                                &process_id,
                                                                tool_id,
                                                                tool_name,
                                                                "started",
                                                                output.tool_input.as_ref(),
                                                                None,
                                                            );
                                                        }
                                                    }
                                                    ClaudeOutputType::ToolEnd => {
                                                        if let (Some(tool_name), Some(tool_id)) = 
                                                            (&output.tool_name, &output.tool_id) 
                                                        {
                                                            emit_claude_tool(
                                                                &process_id,
                                                                tool_id,
                                                                tool_name,
                                                                "running",
                                                                output.tool_input.as_ref(),
                                                                None,
                                                            );
                                                        }
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                        
                                        // Still store raw output for logs
                                        {
                                            let mut out = output.write().await;
                                            out.push_str(&line);
                                            out.push('\n');
                                        }
                                        
                                        let mut registry = get_registry_mut().await;
                                        registry.update(&process_id, |state| {
                                            state.stdout.push_str(&line);
                                            state.stdout.push('\n');
                                        });
                                        
                                        continue; // Skip regular terminal output emit
                                    }
                                }
                                
                                // Emit line to frontend (regular terminal output)
                                emit_terminal_output(&process_id, &line, "stdout");
                                
                                // Store in output
                                {
                                    let mut out = output.write().await;
                                    out.push_str(&line);
                                    out.push('\n');
                                }
                                
                                // Update registry
                                let mut registry = get_registry_mut().await;
                                registry.update(&process_id, |state| {
                                    state.stdout.push_str(&line);
                                    state.stdout.push('\n');
                                });
                            }
                            
                            // If we have partial content without newline, emit it too
                            // (for programs that don't always use newlines)
                            // Skip this for Claude CLI as we need complete JSON lines
                            if !is_claude_cli && !line_buffer.is_empty() && line_buffer.len() > 80 {
                                let partial = std::mem::take(&mut line_buffer);
                                emit_terminal_output(&process_id, &partial, "stdout");
                                
                                let mut out = output.write().await;
                                out.push_str(&partial);
                                
                                let mut registry = get_registry_mut().await;
                                registry.update(&process_id, |state| {
                                    state.stdout.push_str(&partial);
                                });
                            }
                        }
                        Err(e) => {
                            error!("Error reading stdout: {}", e);
                            break;
                        }
                    }
                }
                
                // Flush any remaining content
                if !line_buffer.is_empty() {
                    emit_terminal_output(&process_id, &line_buffer, "stdout");
                    let mut out = output.write().await;
                    out.push_str(&line_buffer);
                    
                    let mut registry = get_registry_mut().await;
                    registry.update(&process_id, |state| {
                        state.stdout.push_str(&line_buffer);
                    });
                }
                
                // Return accumulated clean content from Claude CLI parsing
                if let Some(state) = claude_state {
                    let mut clean = String::new();
                    if !state.content.is_empty() {
                        clean.push_str(&state.content);
                    }
                    // Include tool call summaries if any
                    for (_id, tool) in &state.tool_calls {
                        if !clean.is_empty() {
                            clean.push_str("\n\n");
                        }
                        clean.push_str(&format!("[Tool: {}]", tool.name));
                        if let Some(ref input) = tool.input {
                            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                                clean.push_str(&format!(" Command: {}", cmd));
                            }
                        }
                        if let Some(ref result) = tool.result {
                            clean.push_str(&format!("\nResult: {}", result));
                        }
                    }
                    if !clean.is_empty() {
                        return Some(clean);
                    }
                }
            }
            None
        })
    };
    
    // Spawn stderr reader task
    let stderr_task = {
        let process_id = process_id.clone();
        let output = stderr_output.clone();
        tokio::spawn(async move {
            if let Some(stderr) = stderr_handle {
                let mut reader = BufReader::with_capacity(256, stderr);
                let mut line_buffer = String::new();
                let mut buf = [0u8; 256];
                
                loop {
                    match reader.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            let chunk = String::from_utf8_lossy(&buf[..n]);
                            line_buffer.push_str(&chunk);
                            
                            // Split on both \n and \r to handle progress output
                            // (e.g. git clone uses \r for progress like "Receiving objects: 45%")
                            while let Some(sep_pos) = line_buffer.find(|c| c == '\n' || c == '\r') {
                                let line = line_buffer[..sep_pos].to_string();
                                // Skip past the separator, and also skip \n after \r (CRLF)
                                let skip = if line_buffer.as_bytes().get(sep_pos) == Some(&b'\r')
                                    && line_buffer.as_bytes().get(sep_pos + 1) == Some(&b'\n')
                                {
                                    2
                                } else {
                                    1
                                };
                                line_buffer = line_buffer[sep_pos + skip..].to_string();
                                
                                if line.is_empty() {
                                    continue;
                                }
                                
                                emit_terminal_output(&process_id, &line, "stderr");
                                
                                {
                                    let mut out = output.write().await;
                                    out.push_str(&line);
                                    out.push('\n');
                                }
                                
                                let mut registry = get_registry_mut().await;
                                registry.update(&process_id, |state| {
                                    state.stderr.push_str(&line);
                                    state.stderr.push('\n');
                                });
                            }
                            
                            // Emit partial content for stderr too
                            if !line_buffer.is_empty() && line_buffer.len() > 80 {
                                let partial = std::mem::take(&mut line_buffer);
                                emit_terminal_output(&process_id, &partial, "stderr");
                                
                                let mut out = output.write().await;
                                out.push_str(&partial);
                                
                                let mut registry = get_registry_mut().await;
                                registry.update(&process_id, |state| {
                                    state.stderr.push_str(&partial);
                                });
                            }
                        }
                        Err(e) => {
                            error!("Error reading stderr: {}", e);
                            break;
                        }
                    }
                }
                
                if !line_buffer.is_empty() {
                    emit_terminal_output(&process_id, &line_buffer, "stderr");
                    let mut out = output.write().await;
                    out.push_str(&line_buffer);
                    
                    let mut registry = get_registry_mut().await;
                    registry.update(&process_id, |state| {
                        state.stderr.push_str(&line_buffer);
                    });
                }
            }
        })
    };
    
    // Wait for completion (with optional timeout)
    let status = if let Some(timeout) = timeout_secs {
        match tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            child.wait(),
        ).await {
            Ok(result) => result,
            Err(_) => {
                // Timeout - kill the process
                warn!("Process {} timed out after {} seconds", process_id, timeout);
                let _ = child.kill().await;
                
                // Wait for reader tasks to finish
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                
                // Update registry
                let mut registry = get_registry_mut().await;
                registry.update(&process_id, |state| {
                    state.running = false;
                    state.exit_code = Some(-1);
                    state.ended_at = Some(chrono::Utc::now().timestamp_millis());
                });
                
                let stdout_final = stdout_output.read().await.clone();
                let stderr_final = stderr_output.read().await.clone();
                
                return ExecutionResult {
                    process_id,
                    success: false,
                    exit_code: Some(-1),
                    stdout: stdout_final,
                    stderr: format!("{}\n[Process timed out after {} seconds]", stderr_final, timeout),
                    running: false,
                    error: Some(format!("Process timed out after {} seconds", timeout)),
                    denied: false,
                    claude_content: None,
                };
            }
        }
    } else {
        child.wait().await
    };
    
    // Wait for reader tasks to finish after process completes
    // stdout_task returns Option<String> with clean Claude CLI content (if applicable)
    let claude_clean_content = stdout_task.await.ok().flatten();
    let _ = stderr_task.await;
    
    let stdout_final = stdout_output.read().await.clone();
    let stderr_final = stderr_output.read().await.clone();
    
    if claude_clean_content.is_some() {
        info!("Claude CLI clean content captured ({} chars)", 
              claude_clean_content.as_ref().map(|c| c.len()).unwrap_or(0));
    }
    
    match status {
        Ok(status) => {
            let exit_code = status.code().unwrap_or(-1);
            let success = status.success();
            
            // Update registry
            {
                let mut registry = get_registry_mut().await;
                registry.update(&process_id, |state| {
                    state.running = false;
                    state.exit_code = Some(exit_code);
                    state.ended_at = Some(chrono::Utc::now().timestamp_millis());
                });
            }
            
            // Emit process completed event for UI
            emit_process_completed(&process_id, Some(exit_code), success);
            
            // Mark Claude CLI session as active so subsequent calls auto-get --continue
            if is_claude_cli && success {
                mark_claude_cli_session_active();
            }
            
            ExecutionResult {
                process_id,
                success,
                exit_code: Some(exit_code),
                stdout: stdout_final,
                stderr: stderr_final,
                running: false,
                error: if success { None } else { Some(format!("Process exited with code {}", exit_code)) },
                denied: false,
                claude_content: claude_clean_content,
            }
        }
        Err(e) => {
            error!("Failed to wait for process: {}", e);
            
            // Update registry
            {
                let mut registry = get_registry_mut().await;
                registry.update(&process_id, |state| {
                    state.running = false;
                    state.ended_at = Some(chrono::Utc::now().timestamp_millis());
                });
            }
            
            // Emit process completed (failed) event for UI
            emit_process_completed(&process_id, None, false);
            
            ExecutionResult::error(&format!("Failed to wait for process: {}", e))
        }
    }
}

/// Monitor a background process
async fn monitor_process(process_id: String, mut child: Child, timeout_secs: Option<u64>) {
    // Read stdout with chunk-based real-time streaming
    if let Some(stdout) = child.stdout.take() {
        let pid = process_id.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::with_capacity(256, stdout);
            let mut line_buffer = String::new();
            let mut buf = [0u8; 256];
            
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]);
                        line_buffer.push_str(&chunk);
                        
                        while let Some(newline_pos) = line_buffer.find('\n') {
                            let line = line_buffer[..newline_pos].to_string();
                            line_buffer = line_buffer[newline_pos + 1..].to_string();
                            
                            emit_terminal_output(&pid, &line, "stdout");
                            
                            let mut registry = get_registry_mut().await;
                            registry.update(&pid, |state| {
                                state.stdout.push_str(&line);
                                state.stdout.push('\n');
                            });
                        }
                        
                        // Emit partial content for responsiveness
                        if !line_buffer.is_empty() && line_buffer.len() > 80 {
                            let partial = std::mem::take(&mut line_buffer);
                            emit_terminal_output(&pid, &partial, "stdout");
                            
                            let mut registry = get_registry_mut().await;
                            registry.update(&pid, |state| {
                                state.stdout.push_str(&partial);
                            });
                        }
                    }
                    Err(_) => break,
                }
            }
            
            if !line_buffer.is_empty() {
                emit_terminal_output(&pid, &line_buffer, "stdout");
                let mut registry = get_registry_mut().await;
                registry.update(&pid, |state| {
                    state.stdout.push_str(&line_buffer);
                });
            }
        });
    }
    
    // Read stderr with chunk-based real-time streaming
    if let Some(stderr) = child.stderr.take() {
        let pid = process_id.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::with_capacity(256, stderr);
            let mut line_buffer = String::new();
            let mut buf = [0u8; 256];
            
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]);
                        line_buffer.push_str(&chunk);
                        
                        while let Some(newline_pos) = line_buffer.find('\n') {
                            let line = line_buffer[..newline_pos].to_string();
                            line_buffer = line_buffer[newline_pos + 1..].to_string();
                            
                            emit_terminal_output(&pid, &line, "stderr");
                            
                            let mut registry = get_registry_mut().await;
                            registry.update(&pid, |state| {
                                state.stderr.push_str(&line);
                                state.stderr.push('\n');
                            });
                        }
                        
                        if !line_buffer.is_empty() && line_buffer.len() > 80 {
                            let partial = std::mem::take(&mut line_buffer);
                            emit_terminal_output(&pid, &partial, "stderr");
                            
                            let mut registry = get_registry_mut().await;
                            registry.update(&pid, |state| {
                                state.stderr.push_str(&partial);
                            });
                        }
                    }
                    Err(_) => break,
                }
            }
            
            if !line_buffer.is_empty() {
                emit_terminal_output(&pid, &line_buffer, "stderr");
                let mut registry = get_registry_mut().await;
                registry.update(&pid, |state| {
                    state.stderr.push_str(&line_buffer);
                });
            }
        });
    }
    
    // Wait for completion
    let status = if let Some(timeout) = timeout_secs {
        match tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            child.wait(),
        ).await {
            Ok(result) => result,
            Err(_) => {
                warn!("Background process {} timed out", process_id);
                let _ = child.kill().await;
                
                let mut registry = get_registry_mut().await;
                registry.update(&process_id, |state| {
                    state.running = false;
                    state.exit_code = Some(-1);
                    state.ended_at = Some(chrono::Utc::now().timestamp_millis());
                });
                return;
            }
        }
    } else {
        child.wait().await
    };
    
    if let Ok(status) = status {
        let exit_code = status.code().unwrap_or(-1);
        let mut registry = get_registry_mut().await;
        registry.update(&process_id, |state| {
            state.running = false;
            state.exit_code = Some(exit_code);
            state.ended_at = Some(chrono::Utc::now().timestamp_millis());
        });
    }
}

/// Kill a process by ID
pub async fn kill_process(process_id: &str) -> Result<(), String> {
    let registry = get_registry().await;
    
    if let Some(state) = registry.get(process_id) {
        if !state.running {
            return Err("Process is not running".to_string());
        }
        
        if let Some(pid) = state.pid {
            // Kill the process
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                
                kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
                    .map_err(|e| format!("Failed to kill process: {}", e))?;
            }
            
            #[cfg(windows)]
            {
                // On Windows, use taskkill
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output();
            }
            
            Ok(())
        } else {
            Err("Process has no PID".to_string())
        }
    } else {
        Err(format!("Process not found: {}", process_id))
    }
}

/// Get process output
pub async fn get_process_output(process_id: &str) -> Option<(String, String)> {
    let registry = get_registry().await;
    registry.get(process_id).map(|state| {
        (state.stdout.clone(), state.stderr.clone())
    })
}

/// Check if a process is still running
pub async fn is_process_running(process_id: &str) -> bool {
    let registry = get_registry().await;
    registry.get(process_id).map(|s| s.running).unwrap_or(false)
}

