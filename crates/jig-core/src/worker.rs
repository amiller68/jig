//! Worker state machine
//!
//! A Worker represents a Claude Code session working on a task in an isolated worktree.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for a worker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerId(pub Uuid);

impl WorkerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A worker represents a Claude Code session working on a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub id: WorkerId,
    /// Human-readable name, e.g., "feature-auth"
    pub name: String,
    /// Path to the worktree directory
    pub worktree_path: PathBuf,
    /// Branch name in the worktree
    pub branch: String,
    /// Base branch this was created from
    pub base_branch: String,
    /// Task context (what the worker should do)
    pub task: Option<TaskContext>,
    /// Current status
    pub status: WorkerStatus,
    /// Tmux session name
    pub tmux_session: String,
    /// Window within the tmux session
    pub tmux_window: Option<String>,
    /// When the worker was created
    pub created_at: DateTime<Utc>,
    /// When the worker was last updated
    pub updated_at: DateTime<Utc>,
}

impl Worker {
    /// Create a new worker
    pub fn new(
        name: String,
        worktree_path: PathBuf,
        branch: String,
        base_branch: String,
        tmux_session: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: WorkerId::new(),
            name: name.clone(),
            worktree_path,
            branch,
            base_branch,
            task: None,
            status: WorkerStatus::Spawned,
            tmux_session,
            tmux_window: Some(name),
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the worker's status
    pub fn set_status(&mut self, status: WorkerStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    /// Set the task context
    pub fn set_task(&mut self, task: TaskContext) {
        self.task = Some(task);
        self.updated_at = Utc::now();
    }

    /// Check if the worker is in a terminal state
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Check if the worker is active (not terminal)
    pub fn is_active(&self) -> bool {
        !self.is_terminal()
    }
}

/// Task context describing what the worker should do
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    /// Description of the task
    pub description: String,
    /// Files the task is expected to touch
    pub files_hint: Vec<String>,
    /// Other workers that must complete first
    pub depends_on: Vec<WorkerId>,
    /// Optional issue reference (e.g., "#14" or "issues/014.md")
    pub issue_ref: Option<String>,
}

impl TaskContext {
    pub fn new(description: String) -> Self {
        Self {
            description,
            files_hint: Vec::new(),
            depends_on: Vec::new(),
            issue_ref: None,
        }
    }

    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.files_hint = files;
        self
    }

    pub fn with_dependencies(mut self, deps: Vec<WorkerId>) -> Self {
        self.depends_on = deps;
        self
    }

    pub fn with_issue(mut self, issue_ref: String) -> Self {
        self.issue_ref = Some(issue_ref);
        self
    }
}

/// Worker status state machine
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    /// Worker is being created (worktree + on-create hook running)
    Initializing,
    /// Worker just spawned, no events yet
    Spawned,
    /// Tool use events flowing, actively working
    Running,
    /// Stop event fired, agent at shell prompt
    Idle,
    /// Notification event fired, agent waiting for input
    WaitingInput,
    /// No events for silence_threshold, agent may be stuck
    Stalled,
    /// PR opened, waiting for human review
    WaitingReview,
    /// PR approved, ready to merge
    Approved,
    /// PR merged successfully
    Merged,
    /// Worker failed or was killed
    Failed,
    /// Worker archived/cleaned up
    Archived,
}

impl WorkerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Initializing => "initializing",
            Self::Spawned => "spawned",
            Self::Running => "running",
            Self::Idle => "idle",
            Self::WaitingInput => "waiting_input",
            Self::Stalled => "stalled",
            Self::WaitingReview => "waiting_review",
            Self::Approved => "approved",
            Self::Merged => "merged",
            Self::Failed => "failed",
            Self::Archived => "archived",
        }
    }

    /// States that indicate worker needs attention
    pub fn needs_attention(&self) -> bool {
        matches!(self, Self::WaitingInput | Self::Stalled | Self::Failed)
    }

    /// States that indicate worker is actively working
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Spawned)
    }

    /// States that indicate work is complete
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Merged | Self::Archived | Self::Failed)
    }

    pub fn is_waiting_review(&self) -> bool {
        matches!(self, Self::WaitingReview)
    }

    /// Migrate old status strings to new enum
    pub fn from_legacy(s: &str) -> Self {
        match s {
            "spawned" => Self::Spawned,
            "running" => Self::Running,
            "idle" => Self::Idle,
            "waiting_input" => Self::WaitingInput,
            "stalled" => Self::Stalled,
            "waiting_review" | "review" => Self::WaitingReview,
            "approved" => Self::Approved,
            "merged" => Self::Merged,
            "failed" => Self::Failed,
            "archived" => Self::Archived,
            _ => Self::Running,
        }
    }
}

/// Statistics about the diff in a worktree
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DiffStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub files: Vec<FileDiff>,
}

impl DiffStats {
    pub fn is_empty(&self) -> bool {
        self.files_changed == 0
    }
}

/// Diff information for a single file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub insertions: usize,
    pub deletions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_needs_attention() {
        assert!(WorkerStatus::WaitingInput.needs_attention());
        assert!(WorkerStatus::Stalled.needs_attention());
        assert!(WorkerStatus::Failed.needs_attention());
        assert!(!WorkerStatus::Running.needs_attention());
        assert!(!WorkerStatus::Spawned.needs_attention());
        assert!(!WorkerStatus::Idle.needs_attention());
    }

    #[test]
    fn status_is_active() {
        assert!(WorkerStatus::Running.is_active());
        assert!(WorkerStatus::Spawned.is_active());
        assert!(!WorkerStatus::Idle.is_active());
        assert!(!WorkerStatus::Merged.is_active());
        assert!(!WorkerStatus::Failed.is_active());
    }

    #[test]
    fn status_is_terminal() {
        assert!(WorkerStatus::Merged.is_terminal());
        assert!(WorkerStatus::Archived.is_terminal());
        assert!(WorkerStatus::Failed.is_terminal());
        assert!(!WorkerStatus::Running.is_terminal());
        assert!(!WorkerStatus::WaitingReview.is_terminal());
    }

    #[test]
    fn status_serialization() {
        let status = WorkerStatus::WaitingInput;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"waiting_input\"");

        let parsed: WorkerStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, WorkerStatus::WaitingInput);
    }

    #[test]
    fn status_all_variants_roundtrip() {
        let variants = [
            WorkerStatus::Spawned,
            WorkerStatus::Running,
            WorkerStatus::Idle,
            WorkerStatus::WaitingInput,
            WorkerStatus::Stalled,
            WorkerStatus::WaitingReview,
            WorkerStatus::Approved,
            WorkerStatus::Merged,
            WorkerStatus::Failed,
            WorkerStatus::Archived,
        ];
        for status in &variants {
            let json = serde_json::to_string(status).unwrap();
            let parsed: WorkerStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, status);
        }
    }

    #[test]
    fn legacy_migration() {
        assert_eq!(WorkerStatus::from_legacy("running"), WorkerStatus::Running);
        assert_eq!(WorkerStatus::from_legacy("spawned"), WorkerStatus::Spawned);
        assert_eq!(
            WorkerStatus::from_legacy("review"),
            WorkerStatus::WaitingReview
        );
        assert_eq!(
            WorkerStatus::from_legacy("waiting_review"),
            WorkerStatus::WaitingReview
        );
        assert_eq!(
            WorkerStatus::from_legacy("unknown_value"),
            WorkerStatus::Running
        );
    }
}
