//! Daemon configuration types.

use std::collections::HashMap;

use super::display::{TriageDisplayInfo, WorkerDisplayInfo, WorkerTickInfo};

/// Configuration for the daemon loop.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// How often to poll, in seconds.
    pub interval_seconds: u64,
    /// Whether to run once and exit (vs. looping).
    pub once: bool,
    /// Tmux session prefix (default: "jig-").
    pub session_prefix: String,
    /// Skip `git fetch` on each tick (unused with actors — kept for API compat).
    pub skip_sync: bool,
    /// If set, only process workers for this repo name.
    pub repo_filter: Option<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            once: false,
            session_prefix: "jig-".to_string(),
            skip_sync: false,
            repo_filter: None,
        }
    }
}

/// Result of a single daemon tick.
#[derive(Debug, Default)]
pub struct TickResult {
    pub workers_checked: usize,
    pub actions_dispatched: usize,
    pub nudges_sent: usize,
    pub notifications_sent: usize,
    pub errors: Vec<String>,
    /// Per-worker PR health info, keyed by "repo/worker".
    pub worker_info: HashMap<String, WorkerTickInfo>,
    /// Issues auto-spawned this tick (completed).
    pub auto_spawned: Vec<String>,
    /// Worker names currently being spawned in the background.
    pub spawning: Vec<String>,
    /// Workers pruned (worktree removed) this tick.
    pub pruned: Vec<String>,
    /// Pre-computed display data for the render callback (zero I/O).
    pub worker_display: Vec<WorkerDisplayInfo>,
    /// Pre-computed display data for in-flight triages.
    pub triage_display: Vec<TriageDisplayInfo>,
    /// Nudge messages delivered this tick: (worker_name, nudge_type, message_text).
    pub nudge_messages: Vec<(String, String, String)>,
    /// Seconds until the next poll tick.
    pub poll_remaining_secs: u64,
}
