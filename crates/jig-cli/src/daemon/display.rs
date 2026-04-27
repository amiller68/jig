//! Pre-computed display data for the daemon tick callback.
//!
//! These types are populated during `tick()` so the render callback
//! can format output without any subprocess calls or file I/O.

use jig_core::worker::TmuxStatus;
use jig_core::worker::WorkerStatus;

/// Pre-computed display data for a single worker.
#[derive(Debug, Clone)]
pub struct WorkerDisplayInfo {
    pub repo: String,
    pub name: String,
    pub branch: String,
    pub tmux_status: TmuxStatus,
    pub worker_status: Option<WorkerStatus>,
    pub nudge_count: u32,
    pub max_nudges: u32,
    pub commits_ahead: usize,
    pub is_dirty: bool,
    pub pr_url: Option<String>,
    pub issue_ref: Option<String>,
    pub pr_health: WorkerTickInfo,
    pub is_draft: bool,
    pub nudge_cooldown_remaining: Option<u64>,
}

/// Pre-computed display data for an in-flight triage subprocess.
#[derive(Debug, Clone)]
pub struct TriageDisplayInfo {
    pub issue_id: String,
    pub model: String,
    pub elapsed_secs: u64,
    pub repo_name: String,
}

/// Per-worker PR health info collected during a tick.
#[derive(Debug, Clone, Default)]
pub struct WorkerTickInfo {
    pub pr_checks: Vec<(String, bool)>,
    pub pr_error: Option<String>,
    pub has_pr: bool,
}
