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
        matches!(
            self.status,
            WorkerStatus::Merged | WorkerStatus::Failed { .. } | WorkerStatus::Archived
        )
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerStatus {
    /// Worktree created, tmux session started, waiting for Claude
    Spawned,
    /// Claude is actively working (detected via tmux activity or git changes)
    Running,
    /// Claude appears idle, changes detected, ready for review
    WaitingReview {
        diff_stats: DiffStats,
    },
    /// Human approved the changes
    Approved,
    /// Changes merged into base branch
    Merged,
    /// Worker killed or errored
    Failed {
        reason: String,
    },
    /// Worktree removed, kept for history
    Archived,
}

impl WorkerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkerStatus::Spawned => "spawned",
            WorkerStatus::Running => "running",
            WorkerStatus::WaitingReview { .. } => "review",
            WorkerStatus::Approved => "approved",
            WorkerStatus::Merged => "merged",
            WorkerStatus::Failed { .. } => "failed",
            WorkerStatus::Archived => "archived",
        }
    }

    pub fn is_waiting_review(&self) -> bool {
        matches!(self, WorkerStatus::WaitingReview { .. })
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
