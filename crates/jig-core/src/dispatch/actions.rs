//! Action types for the dispatch system.

use crate::nudge::NudgeType;

/// An action to be executed in response to a state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Send a nudge to a worker via tmux.
    Nudge {
        worker_id: String,
        nudge_type: NudgeType,
    },

    /// Emit a notification (escalation or informational).
    Notify { worker_id: String, message: String },

    /// Kill and restart worker.
    Restart { worker_id: String, reason: String },

    /// Archive worker and cleanup.
    Cleanup { worker_id: String },
}
