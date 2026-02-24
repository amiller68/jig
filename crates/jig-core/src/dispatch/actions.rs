//! Action types for the dispatch system.

/// An action to be executed in response to a state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Send message to worker via tmux.
    Nudge { worker_id: String, message: String },

    /// Auto-approve a stuck prompt.
    AutoApprove { worker_id: String },

    /// Emit a notification.
    Notify { worker_id: String, message: String },

    /// Kill and restart worker.
    Restart { worker_id: String, reason: String },

    /// Archive worker and cleanup.
    Cleanup { worker_id: String },
}
