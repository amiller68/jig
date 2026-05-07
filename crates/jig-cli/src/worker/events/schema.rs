//! Typed event schema for the worker event log.

use serde::{Deserialize, Serialize};

use jig_core::issues::issue::IssueRef;

/// A single event in the worker event log.
///
/// Each variant carries exactly the fields relevant to that lifecycle stage.
/// Serializes as `{"ts":..., "type":"spawn", "branch":"feat/foo", ...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: i64,
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    pub fn event_type(&self) -> EventType {
        self.kind.event_type()
    }
}

/// The discriminated payload of an event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Create {
        branch: String,
    },
    Initializing {
        branch: String,
        base: String,
        #[serde(default, skip_serializing_if = "is_false")]
        auto: bool,
    },
    Spawn {
        branch: String,
        repo: String,
        issue: IssueRef,
    },
    Resume,
    Commit {
        sha: String,
        repo: String,
    },
    Push {
        sha: String,
        repo: String,
    },
    PrOpened {
        pr_url: String,
        pr_number: String,
    },
    Nudge {
        nudge_type: String,
        message: String,
    },
    Stop,
    Notification,
    ToolUseStart,
    ToolUseEnd,
    CiStatus,
    PrCiStatus {
        passed: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        failures: Vec<String>,
    },
    PrConflict {
        has_conflict: bool,
    },
    PrReviewFeedback {
        comment_count: u32,
        changes_requested: u32,
    },
    PrCommitLint {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bad_commits: Vec<String>,
    },
    PrMerged {
        pr_url: String,
    },
    PrClosed {
        pr_url: String,
    },
    Terminal {
        terminal: TerminalKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

fn is_false(v: &bool) -> bool {
    !v
}

/// Terminal state markers — sticky, once set they can't be overridden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalKind {
    Failed,
    Merged,
    Approved,
    Archived,
}

/// Discriminant-only enum for pattern matching (no payload).
/// Used by the reducer and derive modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Create,
    Initializing,
    Spawn,
    Resume,
    ToolUseStart,
    ToolUseEnd,
    Commit,
    Push,
    PrOpened,
    Notification,
    Stop,
    Nudge,
    CiStatus,
    PrCiStatus,
    PrConflict,
    PrReviewFeedback,
    PrCommitLint,
    PrMerged,
    PrClosed,
    Terminal,
}

impl EventKind {
    pub fn event_type(&self) -> EventType {
        match self {
            EventKind::Create { .. } => EventType::Create,
            EventKind::Initializing { .. } => EventType::Initializing,
            EventKind::Spawn { .. } => EventType::Spawn,
            EventKind::Resume => EventType::Resume,
            EventKind::ToolUseStart => EventType::ToolUseStart,
            EventKind::ToolUseEnd => EventType::ToolUseEnd,
            EventKind::Commit { .. } => EventType::Commit,
            EventKind::Push { .. } => EventType::Push,
            EventKind::PrOpened { .. } => EventType::PrOpened,
            EventKind::Notification => EventType::Notification,
            EventKind::Stop => EventType::Stop,
            EventKind::Nudge { .. } => EventType::Nudge,
            EventKind::CiStatus => EventType::CiStatus,
            EventKind::PrCiStatus { .. } => EventType::PrCiStatus,
            EventKind::PrConflict { .. } => EventType::PrConflict,
            EventKind::PrReviewFeedback { .. } => EventType::PrReviewFeedback,
            EventKind::PrCommitLint { .. } => EventType::PrCommitLint,
            EventKind::PrMerged { .. } => EventType::PrMerged,
            EventKind::PrClosed { .. } => EventType::PrClosed,
            EventKind::Terminal { .. } => EventType::Terminal,
        }
    }
}

impl Event {
    pub fn now(kind: EventKind) -> Self {
        Self {
            ts: chrono::Utc::now().timestamp(),
            kind,
        }
    }

    #[cfg(test)]
    pub fn at(ts: i64, kind: EventKind) -> Self {
        Self { ts, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_serializes_flat() {
        let event = Event::now(EventKind::Spawn {
            branch: "feat/foo".into(),
            repo: "myrepo".into(),
            issue: IssueRef::new("JIG-5"),
        });
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["ts"].is_i64());
        assert_eq!(parsed["type"], "spawn");
        assert_eq!(parsed["branch"], "feat/foo");
        assert_eq!(parsed["issue"], "JIG-5");
        assert!(parsed.get("kind").is_none());
    }

    #[test]
    fn pr_opened_roundtrip() {
        let original = Event::now(EventKind::PrOpened {
            pr_url: "https://github.com/pr/1".into(),
            pr_number: "42".into(),
        });
        let json = serde_json::to_string(&original).unwrap();
        let restored: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.event_type(), EventType::PrOpened);
        if let EventKind::PrOpened { pr_url, pr_number } = &restored.kind {
            assert_eq!(pr_url, "https://github.com/pr/1");
            assert_eq!(pr_number, "42");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn terminal_serialization() {
        let event = Event::now(EventKind::Terminal {
            terminal: TerminalKind::Failed,
            reason: Some("on-create hook failed".into()),
        });
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "terminal");
        assert_eq!(parsed["terminal"], "failed");
        assert_eq!(parsed["reason"], "on-create hook failed");
    }

    #[test]
    fn unit_variant_serialization() {
        let event = Event::now(EventKind::Stop);
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "stop");
    }

    #[test]
    fn initializing_only_has_branch_and_base() {
        let event = Event::now(EventKind::Initializing {
            branch: "feat/foo".into(),
            base: "main".into(),
            auto: false,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("issue"));
        assert!(!json.contains("repo"));
        assert!(!json.contains("auto")); // false is skipped
    }
}
