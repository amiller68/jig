//! Action types for the dispatch system.

use crate::nudge::NudgeType;

/// Distinguishes the semantic meaning of a notification so that the executor
/// can emit the correct [`NotificationEvent`] variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyKind {
    /// Worker needs human attention (max nudges, failure, etc.)
    NeedsIntervention,
    /// A PR was opened for this worker.
    PrOpened { pr_url: String },
    /// Worker's work is complete (PR merged).
    WorkCompleted { pr_url: Option<String> },
    /// New review feedback was received on the worker's PR.
    FeedbackReceived { pr_url: String },
    /// Automated review approved the worker's PR.
    ReviewApproved { pr_url: Option<String> },
}

/// An action to be executed in response to a state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Send a nudge to a worker via tmux.
    Nudge {
        worker_id: String,
        nudge_type: NudgeType,
    },

    /// Emit a notification (escalation or informational).
    Notify {
        worker_id: String,
        message: String,
        kind: NotifyKind,
    },

    /// Kill and restart worker.
    Restart { worker_id: String, reason: String },

    /// Archive worker and cleanup.
    Cleanup { worker_id: String },

    /// Mark a linked issue as Complete after PR merge.
    UpdateIssueStatus { worker_id: String, issue_id: String },
}
