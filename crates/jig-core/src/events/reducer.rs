//! State reducer — builds rich WorkerState from events.

use std::collections::HashMap;

use crate::global::HealthConfig;
use crate::worker::WorkerStatus;

use super::schema::{Event, EventType};

/// Rich derived state for a worker, computed by replaying events.
#[derive(Debug, Clone)]
pub struct WorkerState {
    pub status: WorkerStatus,
    pub commit_count: u32,
    pub last_commit_at: Option<i64>,
    pub pr_url: Option<String>,
    pub nudge_counts: HashMap<String, u32>,
    pub started_at: Option<i64>,
    pub last_event_at: Option<i64>,
}

impl Default for WorkerState {
    fn default() -> Self {
        Self {
            status: WorkerStatus::Spawned,
            commit_count: 0,
            last_commit_at: None,
            pr_url: None,
            nudge_counts: HashMap::new(),
            started_at: None,
            last_event_at: None,
        }
    }
}

impl WorkerState {
    /// Reduce an event stream into a WorkerState.
    pub fn reduce(events: &[Event], config: &HealthConfig) -> Self {
        let mut state = Self::default();

        for event in events {
            state.apply(event);
        }

        state.check_silence(config);
        state
    }

    fn apply(&mut self, event: &Event) {
        // Track timestamps
        if self.started_at.is_none() {
            self.started_at = Some(event.ts);
        }
        self.last_event_at = Some(event.ts);

        // Check for terminal markers first
        if let Some(terminal) = event.data.get("terminal").and_then(|v| v.as_str()) {
            match terminal {
                "merged" => {
                    self.status = WorkerStatus::Merged;
                    return;
                }
                "approved" => {
                    self.status = WorkerStatus::Approved;
                    return;
                }
                "failed" => {
                    self.status = WorkerStatus::Failed;
                    return;
                }
                "archived" => {
                    self.status = WorkerStatus::Archived;
                    return;
                }
                _ => {}
            }
        }

        // Don't process events after terminal state
        if self.status.is_terminal() {
            return;
        }

        match event.event_type {
            EventType::Spawn => {
                self.status = WorkerStatus::Spawned;
            }
            EventType::ToolUseStart | EventType::ToolUseEnd => {
                self.status = WorkerStatus::Running;
            }
            EventType::Commit => {
                self.status = WorkerStatus::Running;
                self.commit_count += 1;
                self.last_commit_at = Some(event.ts);
            }
            EventType::Push => {
                self.status = WorkerStatus::Running;
            }
            EventType::Notification => {
                self.status = WorkerStatus::WaitingInput;
            }
            EventType::Stop => {
                self.status = WorkerStatus::Idle;
            }
            EventType::PrOpened => {
                self.status = WorkerStatus::WaitingReview;
                if let Some(url) = event.data.get("pr_url").and_then(|v| v.as_str()) {
                    self.pr_url = Some(url.to_string());
                }
            }
            EventType::Nudge => {
                if let Some(nudge_type) = event.data.get("nudge_type").and_then(|v| v.as_str()) {
                    *self.nudge_counts.entry(nudge_type.to_string()).or_insert(0) += 1;
                }
            }
            EventType::Review => {
                self.status = WorkerStatus::WaitingReview;
            }
            EventType::CiStatus => {}
            EventType::Terminal => {
                // Terminal markers are handled above via data.terminal field
            }
        }
    }

    fn check_silence(&mut self, config: &HealthConfig) {
        if self.status.is_terminal() {
            return;
        }
        // Only mark as stalled if the worker was previously active.
        // A worker that's only "Spawned" hasn't started yet — don't nudge it.
        if self.status == WorkerStatus::Spawned {
            return;
        }
        if let Some(last_ts) = self.last_event_at {
            let now = chrono::Utc::now().timestamp();
            let age = now - last_ts;
            if age > config.silence_threshold_seconds as i64 {
                self.status = WorkerStatus::Stalled;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> HealthConfig {
        HealthConfig::default()
    }

    #[test]
    fn empty_events_returns_spawned() {
        let state = WorkerState::reduce(&[], &default_config());
        assert_eq!(state.status, WorkerStatus::Spawned);
        assert_eq!(state.commit_count, 0);
    }

    #[test]
    fn commit_count_accumulates() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::Commit).with_field("sha", "abc"),
            Event::new(EventType::Commit).with_field("sha", "def"),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.commit_count, 2);
        assert!(state.last_commit_at.is_some());
    }

    #[test]
    fn pr_url_extracted() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::PrOpened).with_field("pr_url", "https://github.com/pr/1"),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::WaitingReview);
        assert_eq!(state.pr_url.as_deref(), Some("https://github.com/pr/1"));
    }

    #[test]
    fn nudge_counts_tracked() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::Nudge).with_field("nudge_type", "stalled"),
            Event::new(EventType::Nudge).with_field("nudge_type", "stalled"),
            Event::new(EventType::Nudge).with_field("nudge_type", "waiting"),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.nudge_counts.get("stalled"), Some(&2));
        assert_eq!(state.nudge_counts.get("waiting"), Some(&1));
    }

    #[test]
    fn terminal_state_is_sticky() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::ToolUseEnd).with_field("terminal", "failed"),
            Event::new(EventType::ToolUseStart),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::Failed);
    }

    #[test]
    fn timestamps_tracked() {
        let events = vec![
            Event::new(EventType::Spawn),
            Event::new(EventType::ToolUseEnd),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert!(state.started_at.is_some());
        assert!(state.last_event_at.is_some());
    }

    #[test]
    fn silence_triggers_stalled() {
        let old_ts = chrono::Utc::now().timestamp() - 600;
        let events = vec![Event {
            ts: old_ts,
            event_type: EventType::ToolUseEnd,
            data: serde_json::Value::Object(serde_json::Map::new()),
        }];
        let config = HealthConfig {
            silence_threshold_seconds: 300,
            ..Default::default()
        };
        let state = WorkerState::reduce(&events, &config);
        assert_eq!(state.status, WorkerStatus::Stalled);
    }
}
