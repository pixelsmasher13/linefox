//! Memory management for automation execution
//!
//! Simplified design following Claude Code/Cursor best practices:
//! - Single flat memory store (no sections - let orchestrator organize)
//! - Append-only with smart compression
//! - Configurable size limits
//! - Context summarization support

use log::info;
use std::sync::{Arc, Mutex};

/// Memory limits for different modes
pub const TASK_MODE_MEMORY_LIMIT: usize = 20_000;      // ~5K tokens
pub const AGENT_MODE_MEMORY_LIMIT: usize = 100_000;        // ~25K tokens

/// Compression threshold - compress when reaching this % of limit
pub const COMPRESSION_THRESHOLD: f32 = 0.85;

/// Simple memory store - append-only with smart trimming
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    /// Raw memory entries (most recent last)
    entries: Vec<MemoryEntry>,
    /// Total character count
    total_chars: usize,
    /// Maximum allowed characters
    max_chars: usize,
    /// Whether we're in agent mode (larger limit)
    agent_mode: bool,
    /// Compressed summary of older entries (created when trimming)
    compressed_summary: Option<String>,
}

/// A single memory entry with metadata
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub content: String,
    pub timestamp: String,
    pub entry_type: EntryType,
}

/// Type of memory entry (for smart compression)
#[derive(Debug, Clone, PartialEq)]
pub enum EntryType {
    Data,           // Raw data collected
    ActionResult,   // Result of an action
    OrchestratorNote, // Note from orchestrator (high priority, don't compress)
    UserInput,      // User clarification (high priority)
}

impl MemoryStore {
    /// Create a new memory store
    pub fn new(agent_mode: bool) -> Self {
        let max_chars = if agent_mode {
            AGENT_MODE_MEMORY_LIMIT
        } else {
            TASK_MODE_MEMORY_LIMIT
        };

        Self {
            entries: Vec::new(),
            total_chars: 0,
            max_chars,
            agent_mode,
            compressed_summary: None,
        }
    }

    /// Save content to memory (simple append)
    pub fn save(&mut self, content: &str, entry_type: EntryType) -> Result<(), String> {
        let entry = MemoryEntry {
            content: content.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            entry_type,
        };

        self.total_chars += content.len();
        self.entries.push(entry);

        // Check if we need to compress
        let threshold = (self.max_chars as f32 * COMPRESSION_THRESHOLD) as usize;
        if self.total_chars > threshold {
            self.compress_old_entries();
        }

        info!("Memory saved: {} total chars ({} entries)", self.total_chars, self.entries.len());
        Ok(())
    }

    /// Simple save (backward compatible - defaults to Data type)
    pub fn save_simple(&mut self, content: &str) -> Result<(), String> {
        self.save(content, EntryType::Data)
    }

    /// Get formatted memory contents for LLM
    /// Note: Action results are NOT included here - executor gets last_action_result directly,
    /// and orchestrator gets action_summary separately. Memory is for USER DATA only.
    pub fn get_formatted(&self) -> String {
        let mut output = String::new();

        // Include compressed summary if exists
        if let Some(ref summary) = self.compressed_summary {
            output.push_str("## Earlier Context (summarized)\n");
            output.push_str(summary);
            output.push_str("\n\n");
        }

        // Only include user data entries, NOT action results
        // Action results clutter the context - executor already sees last_action_result directly
        let data_entries: Vec<_> = self.entries.iter()
            .filter(|e| e.entry_type != EntryType::ActionResult)
            .collect();

        // Include collected data
        if !data_entries.is_empty() {
            for entry in data_entries {
                output.push_str("- ");
                output.push_str(&entry.content);
                output.push('\n');
            }
        }

        if output.is_empty() {
            "<empty>".to_string()
        } else {
            output
        }
    }

    /// Get raw entries (for orchestrator review)
    pub fn get_entries(&self) -> &[MemoryEntry] {
        &self.entries
    }

    /// Compress older entries to make room
    fn compress_old_entries(&mut self) {
        if self.entries.len() <= 5 {
            // Not enough to compress
            return;
        }

        // Keep last 5 entries, compress the rest
        let split_point = self.entries.len() - 5;
        let old_entries: Vec<_> = self.entries.drain(..split_point).collect();

        // Create summary of old entries
        let mut summary = String::new();
        if let Some(existing) = &self.compressed_summary {
            summary.push_str(existing);
            summary.push('\n');
        }

        // Group and summarize old entries
        let data_items: Vec<_> = old_entries.iter()
            .filter(|e| e.entry_type == EntryType::Data)
            .map(|e| e.content.as_str())
            .collect();

        if !data_items.is_empty() {
            summary.push_str(&format!("Previously collected {} data items:\n", data_items.len()));
            // Keep first 200 chars of each, max 10 items in summary
            for (i, item) in data_items.iter().take(10).enumerate() {
                let truncated = if item.len() > 200 {
                    format!("{}...", &item[..200])
                } else {
                    item.to_string()
                };
                summary.push_str(&format!("  {}. {}\n", i + 1, truncated));
            }
            if data_items.len() > 10 {
                summary.push_str(&format!("  ... and {} more items\n", data_items.len() - 10));
            }
        }

        // Keep orchestrator notes and user input in full (high priority)
        for entry in old_entries.iter().filter(|e|
            e.entry_type == EntryType::OrchestratorNote ||
            e.entry_type == EntryType::UserInput
        ) {
            summary.push_str(&format!("Note: {}\n", entry.content));
        }

        self.compressed_summary = Some(summary);

        // Recalculate total chars
        self.total_chars = self.entries.iter().map(|e| e.content.len()).sum::<usize>()
            + self.compressed_summary.as_ref().map(|s| s.len()).unwrap_or(0);

        info!("Memory compressed: {} entries remaining, {} total chars",
            self.entries.len(), self.total_chars);
    }

    /// Clear all memory
    pub fn clear(&mut self) {
        self.entries.clear();
        self.compressed_summary = None;
        self.total_chars = 0;
        info!("Memory cleared");
    }

    /// Get total size
    pub fn len(&self) -> usize {
        self.total_chars
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.compressed_summary.is_none()
    }

    /// Add orchestrator context (high priority, won't be compressed away)
    pub fn add_orchestrator_note(&mut self, note: &str) -> Result<(), String> {
        self.save(note, EntryType::OrchestratorNote)
    }

    /// Add action verification result
    /// Note: This is now a no-op. Action results were cluttering memory.
    /// The executor already gets last_action_result directly in the incremental prompt.
    pub fn add_action_result(&mut self, _result: &str) -> Result<(), String> {
        // Don't store action results - they clutter memory
        // Executor sees last_action_result directly
        Ok(())
    }
}

/// Global memory store instance
lazy_static::lazy_static! {
    pub static ref MEMORY_STORE: Arc<Mutex<MemoryStore>> =
        Arc::new(Mutex::new(MemoryStore::new(false)));
}

// ===== Public API (simplified) =====

/// Initialize memory for a new automation run
pub fn init_memory(agent_mode: bool) {
    let mut mem = MEMORY_STORE.lock().unwrap();
    *mem = MemoryStore::new(agent_mode);
    info!("Memory initialized: agent_mode={}, limit={}", agent_mode, mem.max_chars);
}

/// Save to memory (simple - just content)
pub fn save_to_memory(section: Option<&str>, content: &str) -> Result<(), String> {
    // Section parameter kept for backward compatibility but ignored
    let _ = section;
    let mut mem = MEMORY_STORE.lock().unwrap();
    mem.save_simple(content)
}

/// Get formatted memory contents
pub fn get_memory_contents() -> String {
    let mem = MEMORY_STORE.lock().unwrap();
    mem.get_formatted()
}

/// Clear all memory
pub fn clear_memory() {
    let mut mem = MEMORY_STORE.lock().unwrap();
    mem.clear();
}

/// Get memory size
pub fn get_memory_size() -> usize {
    let mem = MEMORY_STORE.lock().unwrap();
    mem.len()
}

/// Add orchestrator note (won't be compressed)
pub fn add_orchestrator_note(note: &str) -> Result<(), String> {
    let mut mem = MEMORY_STORE.lock().unwrap();
    mem.add_orchestrator_note(note)
}

/// Add action verification result
pub fn add_verification_result(result: &str) -> Result<(), String> {
    let mut mem = MEMORY_STORE.lock().unwrap();
    mem.add_action_result(result)
}

// Keep these for backward compatibility (they now do nothing special)
pub fn revise_memory_section(_section: &str, content: &str) -> Result<(), String> {
    // In simplified model, revise just appends with a note
    save_to_memory(None, &format!("[Updated] {}", content))
}

pub fn get_memory_section(_section: &str) -> Option<String> {
    // No sections anymore, return full memory
    Some(get_memory_contents())
}

pub fn is_agent_mode() -> bool {
    let mem = MEMORY_STORE.lock().unwrap();
    mem.agent_mode
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_save() {
        let mut mem = MemoryStore::new(false);
        mem.save_simple("First item").unwrap();
        mem.save_simple("Second item").unwrap();

        let contents = mem.get_formatted();
        assert!(contents.contains("First item"));
        assert!(contents.contains("Second item"));
    }

    #[test]
    fn test_compression() {
        let mut mem = MemoryStore::new(false);
        mem.max_chars = 500; // Small limit for testing

        // Add many entries to trigger compression
        for i in 0..20 {
            mem.save_simple(&format!("Entry number {} with some content", i)).unwrap();
        }

        // Should have compressed
        assert!(mem.compressed_summary.is_some());
        assert!(mem.entries.len() <= 10); // Most recent kept
    }

    #[test]
    fn test_orchestrator_notes_preserved() {
        let mut mem = MemoryStore::new(false);
        mem.max_chars = 300; // Small limit

        mem.add_orchestrator_note("Important orchestrator context").unwrap();

        // Add data to trigger compression
        for i in 0..15 {
            mem.save_simple(&format!("Data item {}", i)).unwrap();
        }

        // Orchestrator note should be in summary
        let formatted = mem.get_formatted();
        assert!(formatted.contains("Important orchestrator context") ||
                mem.compressed_summary.as_ref().map(|s| s.contains("orchestrator")).unwrap_or(false));
    }
}
