//! Message types for daemon actor channels.

use std::path::PathBuf;

use crate::issues::ProviderKind;

/// Request sent to the sync actor to fetch repos.
pub struct SyncRequest {
    /// (repo_name, repo_path, base_branch)
    pub repos: Vec<(String, PathBuf, String)>,
}

/// Response from the sync actor after completing git fetches.
pub struct SyncComplete {
    /// (repo_name, error_message) for repos that failed to sync.
    pub errors: Vec<(String, String)>,
}

/// Request sent to the GitHub actor to check PR status for a worker.
pub struct GitHubRequest {
    /// Worker key ("repo/worker").
    pub worker_key: String,
    /// Repo name for looking up the GitHub client.
    pub repo_name: String,
    /// Branch name (with slashes) for the worker.
    pub branch: String,
    /// PR URL if already known.
    pub pr_url: Option<String>,
    /// Previous draft status from cache — preserved on error paths so API
    /// failures don't clobber the known draft state.
    pub previous_is_draft: bool,
}

/// Response from the GitHub actor for a single worker's PR check.
#[derive(Debug, Clone)]
pub struct GitHubResponse {
    /// Worker key ("repo/worker").
    pub worker_key: String,
    /// Discovered or existing PR URL.
    pub pr_url: Option<String>,
    /// Per-check outcomes: (check_name, has_problem).
    pub pr_checks: Vec<(String, bool)>,
    /// Error message if the GitHub client failed.
    pub pr_error: Option<String>,
    /// Whether the PR was merged.
    pub pr_merged: bool,
    /// Whether the PR was closed (without merge).
    pub pr_closed: bool,
    /// Whether the PR is a draft.
    pub is_draft: bool,
    /// Total review feedback count (inline comments + ChangesRequested reviews).
    /// Used to detect new feedback and reset review nudge counts.
    pub review_feedback_count: Option<u32>,
}

/// Request sent to the issue actor to poll for auto-spawnable issues.
pub struct IssueRequest {
    /// (repo_root, base_branch) for each registered repo.
    pub repos: Vec<(PathBuf, String)>,
    /// Active workers as (repo_name, worker_name) pairs for per-repo budgeting.
    pub existing_workers: Vec<(String, String)>,
}

/// A worker to prune (worktree + event logs + state).
pub struct PruneTarget {
    pub repo_path: PathBuf,
    pub repo_name: String,
    pub worker_name: String,
}

/// Request sent to the prune actor.
pub struct PruneRequest {
    pub targets: Vec<PruneTarget>,
}

/// Result of pruning a single worker.
pub struct PruneResult {
    /// "repo_name/worker_name"
    pub key: String,
    pub error: Option<String>,
}

/// Response from the prune actor.
pub struct PruneComplete {
    pub results: Vec<PruneResult>,
}

/// An issue that is eligible for auto-spawning.
#[derive(Debug, Clone)]
pub struct SpawnableIssue {
    /// Repo root path for spawning.
    pub repo_root: PathBuf,
    /// Issue identifier (e.g., "ENG-123" or "features/my-feature").
    pub issue_id: String,
    /// Issue title.
    pub issue_title: String,
    /// Issue body (markdown).
    pub issue_body: String,
    /// Derived worker name (e.g., "eng-123" or "feature/eng-123-some-slug").
    pub worker_name: String,
    /// Provider kind for completion instructions.
    pub provider_kind: ProviderKind,
    /// Suggested branch name from the issue provider (e.g., Linear's `branchName`).
    pub branch_name: Option<String>,
}

/// Request sent to the spawn actor to create workers.
pub struct SpawnRequest {
    pub issues: Vec<SpawnableIssue>,
}

/// Result of spawning a single worker.
pub struct SpawnResult {
    pub worker_name: String,
    pub error: Option<String>,
}

/// Response from the spawn actor.
pub struct SpawnComplete {
    pub results: Vec<SpawnResult>,
}

/// Request sent to the nudge actor to deliver a nudge via tmux.
pub struct NudgeRequest {
    /// Tmux session name.
    pub session: String,
    /// Tmux window name (branch).
    pub window: String,
    /// Pre-rendered nudge message text.
    pub message: String,
    /// Nudge type key (e.g. "idle", "stuck", "ci").
    pub nudge_type_key: String,
    /// Whether this is a stuck-prompt nudge (needs auto-approve first).
    pub is_stuck: bool,
    /// Repo name (for event log path).
    pub repo_name: String,
    /// Worker name (for event log path).
    pub worker_name: String,
    /// Worker key ("repo/worker") for response correlation.
    pub worker_key: String,
}

/// Response from the nudge actor after delivering (or failing) a nudge.
pub struct NudgeComplete {
    /// Worker key ("repo/worker").
    pub worker_key: String,
    /// Nudge type key (e.g. "idle", "stuck").
    pub nudge_type_key: String,
    /// Error message if delivery failed, None on success.
    pub error: Option<String>,
}
