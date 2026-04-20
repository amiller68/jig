use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    /// Bare worktree created via `jig create` — not daemon-managed
    Created,
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
            Self::Created => "created",
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

    pub fn needs_attention(&self) -> bool {
        matches!(self, Self::WaitingInput | Self::Stalled | Self::Failed)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Spawned)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Merged | Self::Archived | Self::Failed)
    }

    pub fn is_waiting_review(&self) -> bool {
        matches!(self, Self::WaitingReview)
    }

    pub fn from_legacy(s: &str) -> Self {
        match s {
            "created" => Self::Created,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_needs_attention() {
        assert!(WorkerStatus::WaitingInput.needs_attention());
        assert!(WorkerStatus::Stalled.needs_attention());
        assert!(WorkerStatus::Failed.needs_attention());
        assert!(!WorkerStatus::Running.needs_attention());
    }

    #[test]
    fn status_is_active() {
        assert!(WorkerStatus::Running.is_active());
        assert!(WorkerStatus::Spawned.is_active());
        assert!(!WorkerStatus::Idle.is_active());
        assert!(!WorkerStatus::Merged.is_active());
    }

    #[test]
    fn status_is_terminal() {
        assert!(WorkerStatus::Merged.is_terminal());
        assert!(WorkerStatus::Archived.is_terminal());
        assert!(WorkerStatus::Failed.is_terminal());
        assert!(!WorkerStatus::Running.is_terminal());
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
            WorkerStatus::Created,
            WorkerStatus::Initializing,
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
        assert_eq!(
            WorkerStatus::from_legacy("review"),
            WorkerStatus::WaitingReview
        );
        assert_eq!(
            WorkerStatus::from_legacy("unknown_value"),
            WorkerStatus::Running
        );
    }
}
