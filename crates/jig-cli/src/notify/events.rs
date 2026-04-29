//! Notification event types for human consumption.

use serde::{Deserialize, Serialize};

/// High-level notification events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationEvent {
    WorkStarted {
        repo: String,
        worker: String,
        issue: Option<String>,
    },
    PrOpened {
        repo: String,
        worker: String,
        pr_url: String,
    },
    FeedbackReceived {
        repo: String,
        worker: String,
        pr_url: String,
    },
    FeedbackAddressed {
        repo: String,
        worker: String,
        pr_url: String,
    },
    NeedsIntervention {
        repo: String,
        worker: String,
        reason: String,
    },
    WorkCompleted {
        repo: String,
        worker: String,
        pr_url: Option<String>,
    },
}

/// A timestamped, uniquely identified notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub ts: i64,
    pub id: String,
    #[serde(flatten)]
    pub event: NotificationEvent,
}

impl Notification {
    /// Serialize to a JSON string.
    pub fn to_json(&self) -> jig_core::error::Result<String> {
        Ok(serde_json::to_string(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_serializes_flat() {
        let n = Notification {
            ts: 1000,
            id: "abc".to_string(),
            event: NotificationEvent::WorkStarted {
                repo: "jig".to_string(),
                worker: "feat".to_string(),
                issue: Some("ABC-123".to_string()),
            },
        };
        let json = serde_json::to_string(&n).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "work_started");
        assert_eq!(parsed["repo"], "jig");
        assert_eq!(parsed["ts"], 1000);
        assert_eq!(parsed["id"], "abc");
    }

    #[test]
    fn notification_roundtrip() {
        let n = Notification {
            ts: 2000,
            id: "def".to_string(),
            event: NotificationEvent::NeedsIntervention {
                repo: "jig".to_string(),
                worker: "fix".to_string(),
                reason: "stalled".to_string(),
            },
        };
        let json = serde_json::to_string(&n).unwrap();
        let restored: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.ts, 2000);
        assert_eq!(restored.id, "def");
        assert!(matches!(
            restored.event,
            NotificationEvent::NeedsIntervention { .. }
        ));
    }
}
