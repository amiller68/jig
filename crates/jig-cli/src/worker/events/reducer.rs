//! State reducer — builds rich WorkerState from events.

use std::collections::HashMap;

use url::Url;

use jig_core::Reducible;

use crate::context::{Config, RepoEntry};
use crate::worker::{MuxStatus, WorkerStatus};
use crate::worker::checks::PrChecks;
use jig_core::git::Branch;
use jig_core::issues::issue::IssueRef;

use super::schema::{Event, EventKind, TerminalKind};

/// Per-worker PR health info collected during a tick.
#[derive(Debug, Clone, Default)]
pub struct PrHealth {
    pub pr_checks: PrChecks,
    pub pr_error: Option<String>,
    pub has_pr: bool,
}

/// Full worker state — event-log reduction + runtime enrichment.
///
/// Core fields are set by [`Reducible::apply`]. Runtime fields (`repo`,
/// `mux_status`, `commits_ahead`, etc.) are filled in by [`Worker::tick()`].
#[derive(Debug, Clone)]
pub struct WorkerState {
    // ── Event-derived ───────────────────────────────────────────
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
    pub review_feedback_count: u32,
    pub pr_ci_passed: Option<bool>,
    pub pr_ci_failures: Vec<String>,
    pub pr_has_conflicts: Option<bool>,
    pub pr_review_comment_count: u32,
    pub pr_changes_requested: u32,
    pub pr_bad_commits: Vec<String>,
    pub is_draft: bool,

    // ── Runtime (set by tick) ───────────────────────────────────
    pub repo: Option<RepoEntry>,
    pub name: String,
    pub resolved_branch: Branch,
    pub mux_status: MuxStatus,
    pub commits_ahead: usize,
    pub is_dirty: bool,
    pub parsed_pr_url: Option<Url>,
    pub pr_health: PrHealth,
    pub max_nudges: u32,
    pub nudge_cooldown_remaining: Option<u64>,
}

impl WorkerState {
    pub fn repo_name(&self) -> String {
        self.repo
            .as_ref()
            .and_then(|r| r.path.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn nudge_count(&self) -> u32 {
        self.nudge_counts.values().sum()
    }
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
            review_feedback_count: 0,
            pr_ci_passed: None,
            pr_ci_failures: Vec::new(),
            pr_has_conflicts: None,
            pr_review_comment_count: 0,
            pr_changes_requested: 0,
            pr_bad_commits: Vec::new(),
            is_draft: false,

            repo: None,
            name: String::new(),
            resolved_branch: Branch::new("unknown"),
            mux_status: MuxStatus::default(),
            commits_ahead: 0,
            is_dirty: false,
            parsed_pr_url: None,
            pr_health: PrHealth::default(),
            max_nudges: 0,
            nudge_cooldown_remaining: None,
        }
    }
}

impl WorkerState {
    pub fn check_silence(&mut self, config: &Config) {
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

impl Reducible for Event {
    type State = WorkerState;

    fn apply(state: &mut WorkerState, event: &Event) {
        if state.started_at.is_none() {
            state.started_at = Some(event.ts);
        }
        state.last_event_at = Some(event.ts);

        // Terminal events are sticky
        if let EventKind::Terminal { terminal, .. } = &event.kind {
            state.status = match terminal {
                TerminalKind::Merged => WorkerStatus::Merged,
                TerminalKind::Approved => WorkerStatus::Approved,
                TerminalKind::Failed => WorkerStatus::Failed,
                TerminalKind::Archived => WorkerStatus::Archived,
            };
            return;
        }

        if state.status.is_terminal() {
            return;
        }

        match &event.kind {
            EventKind::Create { .. } => {
                state.status = WorkerStatus::Created;
            }
            EventKind::Initializing { branch, .. } => {
                state.status = WorkerStatus::Initializing;
                state.branch = Some(branch.clone());
            }
            EventKind::Spawn { branch, issue, .. } => {
                state.status = WorkerStatus::Spawned;
                state.branch = Some(branch.clone());
                state.issue_ref = Some(issue.clone());
            }
            EventKind::Resume => {
                state.status = WorkerStatus::Spawned;
            }
            EventKind::ToolUseStart | EventKind::ToolUseEnd => {
                state.status = WorkerStatus::Running;
            }
            EventKind::Commit { .. } => {
                state.status = WorkerStatus::Running;
                state.commit_count += 1;
                state.last_commit_at = Some(event.ts);
            }
            EventKind::Push { .. } => {
                state.status = WorkerStatus::Running;
            }
            EventKind::Notification => {
                state.status = WorkerStatus::WaitingInput;
            }
            EventKind::Stop => {
                state.status = WorkerStatus::Idle;
            }
            EventKind::PrOpened { pr_url, .. } => {
                state.status = WorkerStatus::WaitingReview;
                state.pr_url = Some(pr_url.clone());
            }
            EventKind::Nudge { nudge_type, .. } => {
                *state.nudge_counts.entry(nudge_type.clone()).or_insert(0) += 1;
                state.last_nudge_at.insert(nudge_type.clone(), event.ts);
            }
            EventKind::CiStatus => {}
            EventKind::PrCiStatus { passed, failures } => {
                state.pr_ci_passed = Some(*passed);
                state.pr_ci_failures = failures.clone();
            }
            EventKind::PrConflict { has_conflict } => {
                state.pr_has_conflicts = Some(*has_conflict);
            }
            EventKind::PrReviewFeedback {
                comment_count,
                changes_requested,
            } => {
                state.pr_review_comment_count = *comment_count;
                state.pr_changes_requested = *changes_requested;
                state.review_feedback_count = comment_count + changes_requested;
            }
            EventKind::PrCommitLint { bad_commits } => {
                state.pr_bad_commits = bad_commits.clone();
            }
            EventKind::PrMerged { pr_url } => {
                state.pr_url = Some(pr_url.clone());
                state.status = WorkerStatus::Merged;
            }
            EventKind::PrClosed { .. } => {
                state.status = WorkerStatus::Failed;
            }
            EventKind::Terminal { .. } => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> Config {
        Config::default()
    }

    fn reduce(events: &[Event], config: &Config) -> WorkerState {
        let mut state = WorkerState::default();
        for event in events {
            Event::apply(&mut state, event);
        }
        state.check_silence(config);
        state
    }

    #[test]
    fn empty_events_returns_created() {
        let state = reduce(&[], &default_config());
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
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
        assert_eq!(state.status, WorkerStatus::Spawned);
    }

    #[test]
    fn silence_triggers_stalled() {
        let old_ts = chrono::Utc::now().timestamp() - 600;
        let events = vec![Event::at(old_ts, EventKind::ToolUseEnd)];
        let config = Config {
            silence_threshold_seconds: 300,
            ..Default::default()
        };
        let state = reduce(&events, &config);
        assert_eq!(state.status, WorkerStatus::Stalled);
    }

    #[test]
    fn initializing_event_sets_status() {
        let events = vec![Event::now(EventKind::Initializing {
            branch: "main".into(),
            base: "main".into(),
            auto: false,
        })];
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
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
        let state = reduce(&events, &default_config());
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
        let config = Config {
            silence_threshold_seconds: 300,
            ..Default::default()
        };
        let state = reduce(&events, &config);
        assert_eq!(state.status, WorkerStatus::Initializing);
    }
}
