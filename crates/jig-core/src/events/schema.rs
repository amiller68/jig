//! Event schema for the worker event log.

use serde::{Deserialize, Serialize};

/// Types of events that can occur in a worker's lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
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
    Review,
    Terminal,
}

/// A single event in the worker event log.
///
/// Serializes flat: `{"ts":..., "type":"spawn", "sha":"abc123"}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts: i64,
    #[serde(rename = "type")]
    pub event_type: EventType,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

impl Event {
    /// Create a new event with the given type, timestamped to now.
    pub fn new(event_type: EventType) -> Self {
        Self {
            ts: chrono::Utc::now().timestamp(),
            event_type,
            data: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Builder method to add a field to the event data.
    pub fn with_field(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        if let serde_json::Value::Object(ref mut map) = self.data {
            map.insert(key.to_string(), value.into());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_new_sets_timestamp() {
        let before = chrono::Utc::now().timestamp();
        let event = Event::new(EventType::Spawn);
        let after = chrono::Utc::now().timestamp();

        assert!(event.ts >= before && event.ts <= after);
        assert_eq!(event.event_type, EventType::Spawn);
    }

    #[test]
    fn event_with_field_adds_data() {
        let event = Event::new(EventType::Commit)
            .with_field("sha", "abc123")
            .with_field("message", "fix bug");

        let map = event.data.as_object().unwrap();
        assert_eq!(map["sha"], "abc123");
        assert_eq!(map["message"], "fix bug");
    }

    #[test]
    fn event_serializes_flat() {
        let event = Event::new(EventType::Spawn).with_field("sha", "abc123");
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["ts"].is_i64());
        assert_eq!(parsed["type"], "spawn");
        assert_eq!(parsed["sha"], "abc123");
        // No nested "data" key
        assert!(parsed.get("data").is_none());
    }

    #[test]
    fn event_deserialization_roundtrip() {
        let original = Event::new(EventType::PrOpened)
            .with_field("url", "https://github.com/pr/1")
            .with_field("draft", true);

        let json = serde_json::to_string(&original).unwrap();
        let restored: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.event_type, EventType::PrOpened);
        assert_eq!(restored.ts, original.ts);
        let map = restored.data.as_object().unwrap();
        assert_eq!(map["url"], "https://github.com/pr/1");
        assert_eq!(map["draft"], true);
    }
}
