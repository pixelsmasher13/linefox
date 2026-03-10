use lazy_static::lazy_static;
use std::sync::{Arc, Mutex};
use tokio::process::Command;

/// Curated list of CLI tools worth probing.
/// These are tools the agent might want to use via TERMINAL_RUN.
const TOOLS_TO_PROBE: &[&str] = &[
    // Version control
    "git", "gh",
    // Node / JS
    "node", "npm", "yarn", "pnpm", "bun",
    // Python
    "python3", "python", "pip3", "pip", "uv",
    // Rust
    "cargo", "rustc",
    // Package managers
    "brew", "make",
    // AI CLI tools
    "claude", "aider", "openai", "llm",
    // Media / data
    "ffmpeg", "ffprobe", "magick", "convert", "yt-dlp",
    // Cloud / DevOps
    "docker", "docker-compose", "kubectl", "terraform", "aws", "gcloud",
    // Utilities
    "curl", "wget", "jq", "rg", "fd", "bat", "fzf",
    // Shells / scripting
    "zsh", "bash",
];

lazy_static! {
    static ref CACHED_TOOLS: Arc<Mutex<Option<Vec<String>>>> = Arc::new(Mutex::new(None));
}

/// Probes for available CLI tools using `which`, caches the result.
/// Should be called once at app startup via `tokio::spawn`.
pub async fn probe_and_cache() {
    let mut found: Vec<String> = Vec::new();

    for tool in TOOLS_TO_PROBE {
        if let Ok(output) = Command::new("which").arg(tool).output().await {
            if output.status.success() {
                found.push(tool.to_string());
            }
        }
    }

    found.sort();

    log::info!(
        "[cli_probe] Detected {} CLI tools: {}",
        found.len(),
        found.join(", ")
    );

    if let Ok(mut cache) = CACHED_TOOLS.lock() {
        *cache = Some(found);
    }
}

/// Returns the cached tools list, or None if probe hasn't run yet.
pub fn get_cached_tools() -> Option<Vec<String>> {
    CACHED_TOOLS.lock().ok()?.clone()
}

/// Returns a formatted string for injection into the agent system prompt.
/// Returns None if probe hasn't run yet or no tools were found.
pub fn get_cached_tools_str() -> Option<String> {
    let tools = get_cached_tools()?;
    if tools.is_empty() {
        return None;
    }
    let has_ai = tools.iter().any(|t| t == "claude" || t == "openai");
    let ai_note = if has_ai {
        "\n⚡ AI CLI detected — use TERMINAL_RUN:claude or TERMINAL_RUN:openai for ANY code, writing, or analysis task instead of opening apps."
    } else {
        ""
    };

    Some(format!(
        r#"
────────────────────────────
# AVAILABLE CLI TOOLS (detected on this machine)
────────────────────────────
These tools are confirmed installed. Use TERMINAL_RUN instead of UI automation whenever one applies:
{}
{}
Strategy: CLI first → UI only as a last resort.
"#,
        tools.join(", "),
        ai_note
    ))
}

/// Re-runs the probe (e.g. triggered from a settings UI refresh button).
pub async fn refresh() {
    // Clear cache first so stale data isn't used during refresh
    if let Ok(mut cache) = CACHED_TOOLS.lock() {
        *cache = None;
    }
    probe_and_cache().await;
}
