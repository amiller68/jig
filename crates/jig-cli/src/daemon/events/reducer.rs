//! State reducer — builds DaemonState from events.

use jig_core::Reducible;

use super::schema::{Event, EventKind};

#[derive(Debug, Clone, Default)]
pub struct DaemonState {
    pub started_at: Option<i64>,
    pub stopped_at: Option<i64>,
    pub pid: Option<u32>,
    pub stop_reason: Option<String>,
}

impl Reducible for Event {
    type State = DaemonState;

    fn apply(state: &mut DaemonState, event: &Event) {
        match &event.kind {
            EventKind::Started { pid } => {
                state.started_at = Some(event.ts);
                state.stopped_at = None;
                state.pid = Some(*pid);
                state.stop_reason = None;
            }
            EventKind::Stopped { pid: _, reason } => {
                state.stopped_at = Some(event.ts);
                state.stop_reason = Some(reason.clone());
            }
        }
    }
}

impl DaemonState {
    pub fn previous_run_crashed(&self) -> bool {
        self.started_at.is_some() && self.stopped_at.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reduce(events: &[Event]) -> DaemonState {
        let mut state = DaemonState::default();
        for event in events {
            Event::apply(&mut state, event);
        }
        state
    }

    #[test]
    fn empty_events_defaults() {
        let state = reduce(&[]);
        assert!(state.started_at.is_none());
        assert!(!state.previous_run_crashed());
    }

    #[test]
    fn started_without_stopped_is_crash() {
        let events = vec![Event::started()];
        let state = reduce(&events);
        assert!(state.started_at.is_some());
        assert!(state.previous_run_crashed());
    }

    #[test]
    fn started_then_stopped_is_clean() {
        let events = vec![Event::started(), Event::stopped("normal")];
        let state = reduce(&events);
        assert!(state.started_at.is_some());
        assert!(state.stopped_at.is_some());
        assert!(!state.previous_run_crashed());
        assert_eq!(state.stop_reason.as_deref(), Some("normal"));
    }

    #[test]
    fn multiple_runs_last_wins() {
        let events = vec![Event::started(), Event::stopped("normal"), Event::started()];
        let state = reduce(&events);
        assert!(state.previous_run_crashed());
        assert!(state.stopped_at.is_none());
    }
}
