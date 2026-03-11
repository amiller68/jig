//! State derivation from event logs.
//!
//! Given an event stream, derive the current WorkerStatus.

use crate::global::HealthConfig;
use crate::worker::WorkerStatus;

use super::schema::{Event, EventType};

/// Derive worker status from an event stream.
///
/// Replays events and applies transition rules. Terminal states are sticky —
/// once a worker is Merged/Failed/Archived, later events don't change that.
pub fn derive_status(events: &[Event], config: &HealthConfig) -> WorkerStatus {
    if events.is_empty() {
        return WorkerStatus::Spawned;
    }

    // Check for terminal states first (scan backwards)
    if let Some(status) = check_terminal_state(events) {
        return status;
    }

    let now = chrono::Utc::now().timestamp();
    let last_event = events.last().unwrap();
    let last_event_age = now - last_event.ts;

    // Check silence threshold (but not for initializing workers — they may have long hooks)
    if last_event_age > config.silence_threshold_seconds as i64
        && !matches!(last_event.event_type, EventType::Initializing)
    {
        return WorkerStatus::Stalled;
    }

    // Derive from last event type
    match last_event.event_type {
        EventType::Stop => WorkerStatus::Idle,
        EventType::Notification => WorkerStatus::WaitingInput,
        EventType::ToolUseStart | EventType::ToolUseEnd => WorkerStatus::Running,
        EventType::Commit | EventType::Push => WorkerStatus::Running,
        EventType::PrOpened => WorkerStatus::WaitingReview,
        EventType::Initializing => WorkerStatus::Initializing,
        EventType::Spawn | EventType::Resume => WorkerStatus::Spawned,
        EventType::Review => WorkerStatus::WaitingReview,
        EventType::CiStatus | EventType::Nudge => WorkerStatus::Running,
        EventType::Terminal => WorkerStatus::Archived,
    }
}

/// Scan events (newest first) for terminal states.
fn check_terminal_state(events: &[Event]) -> Option<WorkerStatus> {
    for event in events.iter().rev() {
        // Terminal states are signaled via data fields since they aren't
        // separate EventType variants. Check the data for terminal markers.
        if let Some(terminal) = event.data.get("terminal").and_then(|v| v.as_str()) {
            match terminal {
                "merged" => return Some(WorkerStatus::Merged),
                "approved" => return Some(WorkerStatus::Approved),
                "failed" => return Some(WorkerStatus::Failed),
                "archived" => return Some(WorkerStatus::Archived),
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> HealthConfig {
        HealthConfig::default()
    }

    #[test]
    fn empty_events_returns_spawned() {
        assert_eq!(derive_status(&[], &default_config()), WorkerStatus::Spawned);
    }

    #[test]
    fn derive_running_from_tool_use() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::ToolUseStart).with_field("tool", "bash"),
        ];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::Running
        );
    }

    #[test]
    fn derive_idle_from_stop() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::ToolUseEnd),
            Event::new(EventType::Stop),
        ];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::Idle
        );
    }

    #[test]
    fn derive_waiting_input_from_notification() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::Notification).with_field("message", "Need approval"),
        ];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::WaitingInput
        );
    }

    #[test]
    fn derive_stalled_from_silence() {
        let old_ts = chrono::Utc::now().timestamp() - 600; // 10 min ago
        let events = vec![Event {
            ts: old_ts,
            event_type: EventType::ToolUseEnd,
            data: serde_json::Value::Object(serde_json::Map::new()),
        }];
        let config = HealthConfig {
            silence_threshold_seconds: 300,
            ..Default::default()
        };
        assert_eq!(derive_status(&events, &config), WorkerStatus::Stalled);
    }

    #[test]
    fn terminal_state_is_sticky() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::ToolUseEnd).with_field("terminal", "merged"),
            Event::new(EventType::ToolUseStart), // should be ignored
        ];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::Merged
        );
    }

    #[test]
    fn derive_waiting_review_from_pr() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::PrOpened).with_field("url", "https://github.com/pr/1"),
        ];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::WaitingReview
        );
    }
}
