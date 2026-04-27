//! State derivation from event logs.
//!
//! Given an event stream, derive the current WorkerStatus.

use crate::config::HealthConfig;
use crate::worker::WorkerStatus;

use super::schema::{Event, EventKind, EventType, TerminalKind};

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

    let event_type = last_event.event_type();

    // Check silence threshold (but not for initializing workers — they may have long hooks)
    if last_event_age > config.silence_threshold_seconds as i64
        && !matches!(event_type, EventType::Initializing | EventType::Create)
    {
        return WorkerStatus::Stalled;
    }

    match event_type {
        EventType::Stop => WorkerStatus::Idle,
        EventType::Notification => WorkerStatus::WaitingInput,
        EventType::ToolUseStart | EventType::ToolUseEnd => WorkerStatus::Running,
        EventType::Commit | EventType::Push => WorkerStatus::Running,
        EventType::PrOpened => WorkerStatus::WaitingReview,
        EventType::Create => WorkerStatus::Created,
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
        if let EventKind::Terminal { terminal, .. } = &event.kind {
            return Some(match terminal {
                TerminalKind::Merged => WorkerStatus::Merged,
                TerminalKind::Approved => WorkerStatus::Approved,
                TerminalKind::Failed => WorkerStatus::Failed,
                TerminalKind::Archived => WorkerStatus::Archived,
            });
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

    fn spawn_event() -> Event {
        Event::now(EventKind::Spawn {
            branch: "main".into(),
            repo: "r".into(),
            issue: crate::issues::issue::IssueRef::new("JIG-1"),
        })
    }

    #[test]
    fn empty_events_returns_spawned() {
        assert_eq!(derive_status(&[], &default_config()), WorkerStatus::Spawned);
    }

    #[test]
    fn derive_running_from_tool_use() {
        let events = vec![spawn_event(), Event::now(EventKind::ToolUseStart)];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::Running
        );
    }

    #[test]
    fn derive_idle_from_stop() {
        let events = vec![
            spawn_event(),
            Event::now(EventKind::ToolUseEnd),
            Event::now(EventKind::Stop),
        ];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::Idle
        );
    }

    #[test]
    fn derive_waiting_input_from_notification() {
        let events = vec![spawn_event(), Event::now(EventKind::Notification)];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::WaitingInput
        );
    }

    #[test]
    fn derive_stalled_from_silence() {
        let old_ts = chrono::Utc::now().timestamp() - 600;
        let events = vec![Event::at(old_ts, EventKind::ToolUseEnd)];
        let config = HealthConfig {
            silence_threshold_seconds: 300,
            ..Default::default()
        };
        assert_eq!(derive_status(&events, &config), WorkerStatus::Stalled);
    }

    #[test]
    fn terminal_state_is_sticky() {
        let events = vec![
            spawn_event(),
            Event::now(EventKind::Terminal {
                terminal: TerminalKind::Merged,
                reason: None,
            }),
            Event::now(EventKind::ToolUseStart),
        ];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::Merged
        );
    }

    #[test]
    fn derive_waiting_review_from_pr() {
        let events = vec![
            spawn_event(),
            Event::now(EventKind::PrOpened {
                pr_url: "https://github.com/pr/1".into(),
                pr_number: "1".into(),
            }),
        ];
        assert_eq!(
            derive_status(&events, &default_config()),
            WorkerStatus::WaitingReview
        );
    }
}
