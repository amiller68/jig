//! State reducer — builds rich WorkerState from events.

use std::collections::HashMap;

use crate::config::HealthConfig;
use crate::worker::WorkerStatus;
use jig_core::issues::issue::IssueRef;

use super::schema::{Event, EventKind, TerminalKind};

/// Rich derived state for a worker, computed by replaying events.
#[derive(Debug, Clone)]
pub struct WorkerState {
    pub status: WorkerStatus,
    pub branch: Option<String>,
    pub commit_count: u32,
    pub last_commit_at: Option<i64>,
    pub pr_url: Option<String>,
    pub nudge_counts: HashMap<String, u32>,
    pub last_nudge_at: HashMap<String, i64>,
    pub issue_ref: Option<IssueRef>,
    pub started_at: Option<i64>,
    pub last_event_at: Option<i64>,
}

impl Default for WorkerState {
    fn default() -> Self {
        Self {
            status: WorkerStatus::Created,
            branch: None,
            commit_count: 0,
            last_commit_at: None,
            pr_url: None,
            nudge_counts: HashMap::new(),
            last_nudge_at: HashMap::new(),
            issue_ref: None,
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
        if self.started_at.is_none() {
            self.started_at = Some(event.ts);
        }
        self.last_event_at = Some(event.ts);

        // Terminal events are sticky
        if let EventKind::Terminal { terminal, .. } = &event.kind {
            self.status = match terminal {
                TerminalKind::Merged => WorkerStatus::Merged,
                TerminalKind::Approved => WorkerStatus::Approved,
                TerminalKind::Failed => WorkerStatus::Failed,
                TerminalKind::Archived => WorkerStatus::Archived,
            };
            return;
        }

        if self.status.is_terminal() {
            return;
        }

        match &event.kind {
            EventKind::Create { .. } => {
                self.status = WorkerStatus::Created;
            }
            EventKind::Initializing { branch, .. } => {
                self.status = WorkerStatus::Initializing;
                self.branch = Some(branch.clone());
            }
            EventKind::Spawn { branch, issue, .. } => {
                self.status = WorkerStatus::Spawned;
                self.branch = Some(branch.clone());
                self.issue_ref = Some(issue.clone());
            }
            EventKind::Resume => {
                self.status = WorkerStatus::Spawned;
            }
            EventKind::ToolUseStart | EventKind::ToolUseEnd => {
                self.status = WorkerStatus::Running;
            }
            EventKind::Commit { .. } => {
                self.status = WorkerStatus::Running;
                self.commit_count += 1;
                self.last_commit_at = Some(event.ts);
            }
            EventKind::Push { .. } => {
                self.status = WorkerStatus::Running;
            }
            EventKind::Notification => {
                self.status = WorkerStatus::WaitingInput;
            }
            EventKind::Stop => {
                self.status = WorkerStatus::Idle;
            }
            EventKind::PrOpened { pr_url, .. } => {
                self.status = WorkerStatus::WaitingReview;
                self.pr_url = Some(pr_url.clone());
            }
            EventKind::Nudge { nudge_type, .. } => {
                *self.nudge_counts.entry(nudge_type.clone()).or_insert(0) += 1;
                self.last_nudge_at.insert(nudge_type.clone(), event.ts);
            }
            EventKind::CiStatus => {}
            EventKind::Terminal { .. } => unreachable!(),
        }
    }

    fn check_silence(&mut self, config: &HealthConfig) {
        if self.status.is_terminal() {
            return;
        }
        if matches!(
            self.status,
            WorkerStatus::WaitingReview | WorkerStatus::Initializing | WorkerStatus::Created
        ) {
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
    fn empty_events_returns_created() {
        let state = WorkerState::reduce(&[], &default_config());
        assert_eq!(state.status, WorkerStatus::Created);
        assert_eq!(state.commit_count, 0);
    }

    #[test]
    fn commit_count_accumulates() {
        let events = vec![
            Event::now(EventKind::Spawn {
                branch: "main".into(),
                repo: "r".into(),
                issue: IssueRef::new("JIG-1"),
            }),
            Event::now(EventKind::Commit {
                sha: "abc".into(),
                repo: "r".into(),
            }),
            Event::now(EventKind::Commit {
                sha: "def".into(),
                repo: "r".into(),
            }),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.commit_count, 2);
        assert!(state.last_commit_at.is_some());
    }

    #[test]
    fn pr_url_extracted() {
        let events = vec![
            Event::now(EventKind::Spawn {
                branch: "main".into(),
                repo: "r".into(),
                issue: IssueRef::new("JIG-1"),
            }),
            Event::now(EventKind::PrOpened {
                pr_url: "https://github.com/pr/1".into(),
                pr_number: "1".into(),
            }),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::WaitingReview);
        assert_eq!(state.pr_url.as_deref(), Some("https://github.com/pr/1"));
    }

    #[test]
    fn issue_ref_extracted() {
        let events = vec![
            Event::now(EventKind::Spawn {
                branch: "main".into(),
                repo: "r".into(),
                issue: IssueRef::new("features/smart-context"),
            }),
            Event::now(EventKind::ToolUseStart),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.issue_ref.as_deref(), Some("features/smart-context"));
    }

    #[test]
    fn nudge_counts_tracked() {
        let events = vec![
            Event::now(EventKind::Spawn {
                branch: "main".into(),
                repo: "r".into(),
                issue: IssueRef::new("JIG-1"),
            }),
            Event::now(EventKind::Nudge {
                nudge_type: "stalled".into(),
                message: "m".into(),
            }),
            Event::now(EventKind::Nudge {
                nudge_type: "stalled".into(),
                message: "m".into(),
            }),
            Event::now(EventKind::Nudge {
                nudge_type: "waiting".into(),
                message: "m".into(),
            }),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.nudge_counts.get("stalled"), Some(&2));
        assert_eq!(state.nudge_counts.get("waiting"), Some(&1));
    }

    #[test]
    fn last_nudge_at_tracked() {
        let now = chrono::Utc::now().timestamp();
        let events = vec![
            Event::now(EventKind::Spawn {
                branch: "main".into(),
                repo: "r".into(),
                issue: IssueRef::new("JIG-1"),
            }),
            Event::at(
                now - 600,
                EventKind::Nudge {
                    nudge_type: "ci".into(),
                    message: "m".into(),
                },
            ),
            Event::at(
                now - 100,
                EventKind::Nudge {
                    nudge_type: "ci".into(),
                    message: "m".into(),
                },
            ),
            Event::at(
                now - 500,
                EventKind::Nudge {
                    nudge_type: "review".into(),
                    message: "m".into(),
                },
            ),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.last_nudge_at.get("ci"), Some(&(now - 100)));
        assert_eq!(state.last_nudge_at.get("review"), Some(&(now - 500)));
        assert_eq!(state.nudge_counts.get("ci"), Some(&2));
    }

    #[test]
    fn terminal_state_is_sticky() {
        let events = vec![
            Event::now(EventKind::Spawn {
                branch: "main".into(),
                repo: "r".into(),
                issue: IssueRef::new("JIG-1"),
            }),
            Event::now(EventKind::Terminal {
                terminal: TerminalKind::Failed,
                reason: None,
            }),
            Event::now(EventKind::ToolUseStart),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::Failed);
    }

    #[test]
    fn timestamps_tracked() {
        let events = vec![
            Event::now(EventKind::Spawn {
                branch: "main".into(),
                repo: "r".into(),
                issue: IssueRef::new("JIG-1"),
            }),
            Event::now(EventKind::ToolUseEnd),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert!(state.started_at.is_some());
        assert!(state.last_event_at.is_some());
    }

    #[test]
    fn resume_preserves_commit_count_and_issue_ref() {
        let events = vec![
            Event::now(EventKind::Spawn {
                branch: "main".into(),
                repo: "r".into(),
                issue: IssueRef::new("features/smart-context"),
            }),
            Event::now(EventKind::Commit {
                sha: "abc".into(),
                repo: "r".into(),
            }),
            Event::now(EventKind::Commit {
                sha: "def".into(),
                repo: "r".into(),
            }),
            Event::now(EventKind::Resume),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::Spawned);
        assert_eq!(state.commit_count, 2);
        assert_eq!(state.issue_ref.as_deref(), Some("features/smart-context"));
    }

    #[test]
    fn resume_transitions_to_spawned() {
        let events = vec![
            Event::now(EventKind::Spawn {
                branch: "main".into(),
                repo: "r".into(),
                issue: IssueRef::new("JIG-1"),
            }),
            Event::now(EventKind::ToolUseStart),
            Event::now(EventKind::Stop),
            Event::now(EventKind::Resume),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::Spawned);
    }

    #[test]
    fn silence_triggers_stalled() {
        let old_ts = chrono::Utc::now().timestamp() - 600;
        let events = vec![Event::at(old_ts, EventKind::ToolUseEnd)];
        let config = HealthConfig {
            silence_threshold_seconds: 300,
            ..Default::default()
        };
        let state = WorkerState::reduce(&events, &config);
        assert_eq!(state.status, WorkerStatus::Stalled);
    }

    #[test]
    fn initializing_event_sets_status() {
        let events = vec![Event::now(EventKind::Initializing {
            branch: "main".into(),
            base: "main".into(),
            auto: false,
        })];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::Initializing);
    }

    #[test]
    fn initializing_transitions_to_spawned() {
        let events = vec![
            Event::now(EventKind::Initializing {
                branch: "feat/my-feature".into(),
                base: "main".into(),
                auto: false,
            }),
            Event::now(EventKind::Spawn {
                branch: "feat/my-feature".into(),
                repo: "r".into(),
                issue: IssueRef::new("features/my-feature"),
            }),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::Spawned);
    }

    #[test]
    fn initializing_to_failed_on_terminal() {
        let events = vec![
            Event::now(EventKind::Initializing {
                branch: "main".into(),
                base: "main".into(),
                auto: false,
            }),
            Event::now(EventKind::Terminal {
                terminal: TerminalKind::Failed,
                reason: Some("on-create hook failed".into()),
            }),
        ];
        let state = WorkerState::reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::Failed);
    }

    #[test]
    fn initializing_not_marked_stalled() {
        let old_ts = chrono::Utc::now().timestamp() - 600;
        let events = vec![Event::at(
            old_ts,
            EventKind::Initializing {
                branch: "main".into(),
                base: "main".into(),
                auto: false,
            },
        )];
        let config = HealthConfig {
            silence_threshold_seconds: 300,
            ..Default::default()
        };
        let state = WorkerState::reduce(&events, &config);
        assert_eq!(state.status, WorkerStatus::Initializing);
    }
}
