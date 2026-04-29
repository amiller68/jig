//! Typed event schema for the daemon event log.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: i64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Started { pid: u32 },
    Stopped { pid: u32, reason: String },
}

impl Event {
    pub fn now(kind: EventKind) -> Self {
        Self {
            ts: chrono::Utc::now().timestamp(),
            kind,
        }
    }

    pub fn started() -> Self {
        Self::now(EventKind::Started {
            pid: std::process::id(),
        })
    }

    pub fn stopped(reason: &str) -> Self {
        Self::now(EventKind::Stopped {
            pid: std::process::id(),
            reason: reason.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn started_serializes_flat() {
        let event = Event::started();
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["ts"].is_i64());
        assert_eq!(parsed["type"], "started");
        assert!(parsed["pid"].is_u64());
        assert!(parsed.get("kind").is_none());
    }

    #[test]
    fn stopped_roundtrip() {
        let original = Event::stopped("signal");
        let json = serde_json::to_string(&original).unwrap();
        let restored: Event = serde_json::from_str(&json).unwrap();

        assert!(matches!(
            restored.kind,
            EventKind::Stopped { ref reason, .. } if reason == "signal"
        ));
    }
}
