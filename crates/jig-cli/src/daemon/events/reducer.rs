//! State reducer — builds DaemonState from events.

use super::schema::{Event, EventKind};

#[derive(Debug, Clone, Default)]
pub struct DaemonState {
    pub started_at: Option<i64>,
    pub stopped_at: Option<i64>,
    pub pid: Option<u32>,
    pub stop_reason: Option<String>,
}

impl DaemonState {
    pub fn reduce(events: &[Event]) -> Self {
        let mut state = Self::default();
        for event in events {
            state.apply(event);
        }
        state
    }

    fn apply(&mut self, event: &Event) {
        match &event.kind {
            EventKind::Started { pid } => {
                self.started_at = Some(event.ts);
                self.stopped_at = None;
                self.pid = Some(*pid);
                self.stop_reason = None;
            }
            EventKind::Stopped { pid: _, reason } => {
                self.stopped_at = Some(event.ts);
                self.stop_reason = Some(reason.clone());
            }
        }
    }

    pub fn previous_run_crashed(&self) -> bool {
        self.started_at.is_some() && self.stopped_at.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_events_defaults() {
        let state = DaemonState::reduce(&[]);
        assert!(state.started_at.is_none());
        assert!(!state.previous_run_crashed());
    }

    #[test]
    fn started_without_stopped_is_crash() {
        let events = vec![Event::started()];
        let state = DaemonState::reduce(&events);
        assert!(state.started_at.is_some());
        assert!(state.previous_run_crashed());
    }

    #[test]
    fn started_then_stopped_is_clean() {
        let events = vec![Event::started(), Event::stopped("normal")];
        let state = DaemonState::reduce(&events);
        assert!(state.started_at.is_some());
        assert!(state.stopped_at.is_some());
        assert!(!state.previous_run_crashed());
        assert_eq!(state.stop_reason.as_deref(), Some("normal"));
    }

    #[test]
    fn multiple_runs_last_wins() {
        let events = vec![Event::started(), Event::stopped("normal"), Event::started()];
        let state = DaemonState::reduce(&events);
        assert!(state.previous_run_crashed());
        assert!(state.stopped_at.is_none());
    }
}
