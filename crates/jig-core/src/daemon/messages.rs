//! Message types for daemon actor channels.

use std::path::PathBuf;

use crate::issues::{Issue, ProviderKind};
use crate::spawn::SpawnKind;

/// Request sent to the sync actor to fetch repos.
pub struct SyncRequest {
    /// (repo_name, repo_path, base_branch)
    pub repos: Vec<(String, PathBuf, String)>,
    /// Parent branches that child workers depend on: (repo_name, repo_path, branch).
    /// These are fetched in addition to base branches so remote refs stay current
    /// for parent worktree auto-update.
    pub parent_branches: Vec<(String, PathBuf, String)>,
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
    /// The parsed issue.
    pub issue: Issue,
    /// Derived worker name (e.g., "eng-123" or "feature/eng-123-some-slug").
    pub worker_name: String,
    /// Provider kind for completion instructions.
    pub provider_kind: ProviderKind,
    /// Whether this is a normal or triage spawn.
    pub kind: SpawnKind,
}

/// Result of creating (or skipping) a parent integration branch.
#[derive(Debug, Clone)]
pub struct ParentBranchResult {
    /// Repo root path.
    pub repo_root: PathBuf,
    /// Repo name (derived from repo root).
    pub repo_name: String,
    /// Parent issue ID.
    pub issue_id: String,
    /// Branch name created/verified.
    pub branch_name: String,
    /// Whether the branch was newly created (vs already existed).
    pub created: bool,
    /// Whether the issue status was flipped to InProgress.
    pub status_updated: bool,
    /// Error message if something went wrong (branch still may have been created).
    pub error: Option<String>,
}

/// Response from the issue actor containing both spawnable and triageable issues.
pub struct IssueResponse {
    /// Issues eligible for normal auto-spawn (status=Planned).
    pub spawnable: Vec<SpawnableIssue>,
    /// Issues eligible for triage (status=Triage, repo has triage enabled).
    /// These are run as direct subprocesses by the triage actor — no worker,
    /// no worktree, no branch.
    pub triageable: Vec<TriageIssue>,
    /// Parent integration branches created or verified this poll.
    pub parent_branches: Vec<ParentBranchResult>,
    /// Parent issues ready for wrap-up (all children Complete + merged into the
    /// parent integration branch).
    pub wrapup: Vec<SpawnableIssue>,
}

/// Request sent to the spawn actor to create workers.
pub struct SpawnRequest {
    pub issues: Vec<SpawnableIssue>,
}

/// Result of spawning a single worker.
pub struct SpawnResult {
    pub worker_name: String,
    /// Repo name (derived from repo root path) for notifications.
    pub repo_name: String,
    /// Issue ID for notifications.
    pub issue_id: Option<String>,
    pub error: Option<String>,
}

/// Response from the spawn actor.
pub struct SpawnComplete {
    pub results: Vec<SpawnResult>,
}

/// A triage issue to run as a direct subprocess.
#[derive(Debug, Clone)]
pub struct TriageIssue {
    /// Repo root path.
    pub repo_root: PathBuf,
    /// The parsed issue.
    pub issue: Issue,
    /// Derived worker name (e.g., "triage-jig-38").
    pub worker_name: String,
    /// Provider kind for status updates.
    pub provider_kind: ProviderKind,
}

/// Request sent to the triage actor to run triage subprocesses.
pub struct TriageRequest {
    pub issues: Vec<TriageIssue>,
}

/// Result of a single triage subprocess.
pub struct TriageResult {
    /// Worker name (e.g., "triage-jig-38").
    pub worker_name: String,
    /// Repo name for notifications.
    pub repo_name: String,
    /// Issue ID for tracker cleanup.
    pub issue_id: String,
    /// Error message if the triage failed, None on success.
    pub error: Option<String>,
}

/// Response from the triage actor.
pub struct TriageComplete {
    pub results: Vec<TriageResult>,
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

/// Request to run an automated review for a worker.
pub struct ReviewRequest {
    /// Worker key ("repo/worker") for correlation.
    pub worker_key: String,
    /// Absolute path to the worktree directory.
    pub worktree_path: PathBuf,
    /// Base branch for diff comparison (e.g. "origin/main").
    pub base_branch: String,
}

/// Result of a review run.
pub struct ReviewComplete {
    /// Worker key ("repo/worker") for correlation.
    pub worker_key: String,
    /// Error message if the review failed to run. None means the review
    /// agent ran and wrote a file to .jig/reviews/ (check the filesystem
    /// for the verdict).
    pub error: Option<String>,
}
