//! DaemonRuntime — owns actors and thread handles.

use std::time::Instant;

use crate::actors::Actor;
use crate::actors::github::GitHubActor;
use crate::actors::issue::IssueActor;
use crate::actors::nudge::NudgeActor;
use crate::actors::prune::PruneActor;
use crate::actors::review::ReviewActor;
use crate::actors::spawn::SpawnActor;
use crate::actors::sync::SyncActor;
use crate::actors::triage::TriageActor;
use crate::triage_tracker::TriageTracker;

/// Runtime configuration for the daemon actors.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub max_concurrent_workers: usize,
    /// Seconds between sync + issue poll ticks.
    pub poll_interval: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_workers: 3,
            poll_interval: 60,
        }
    }
}

/// Owns actors and thread handles for the non-blocking daemon loop.
pub struct DaemonRuntime {
    pub sync: SyncActor,
    pub github: GitHubActor,
    pub issues: IssueActor,
    pub prune: PruneActor,
    pub spawn: SpawnActor,
    pub nudge: NudgeActor,
    pub review: ReviewActor,
    pub triage: TriageActor,

    pub triage_tracker: TriageTracker,
    pub config: RuntimeConfig,

    last_poll: Instant,
    _handles: Vec<std::thread::JoinHandle<()>>,
}

impl DaemonRuntime {
    pub fn new(config: RuntimeConfig) -> Self {
        let (sync, sync_h) = SyncActor::new();
        let (github, github_h) = GitHubActor::new();
        let (issues, issues_h) = IssueActor::new();
        let (prune, prune_h) = PruneActor::new();
        let (spawn, spawn_h) = SpawnActor::new();
        let (nudge, nudge_h) = NudgeActor::new();
        let (review, review_h) = ReviewActor::new();
        let (triage, triage_h) = TriageActor::new();

        Self {
            sync,
            github,
            issues,
            prune,
            spawn,
            nudge,
            review,
            triage,
            triage_tracker: TriageTracker::new(),
            last_poll: Instant::now() - std::time::Duration::from_secs(config.poll_interval + 1),
            config,
            _handles: vec![
                sync_h, github_h, issues_h, prune_h, spawn_h, nudge_h, review_h, triage_h,
            ],
        }
    }

    /// Whether both sync and issue poll are due.
    pub fn poll_is_due(&self) -> bool {
        !self.sync.is_pending()
            && !self.issues.is_pending()
            && self.last_poll.elapsed().as_secs() >= self.config.poll_interval
    }

    /// Mark that a poll tick just fired.
    pub fn mark_polled(&mut self) {
        self.last_poll = Instant::now();
    }

    /// Seconds until the next poll tick.
    pub fn poll_remaining_secs(&self) -> u64 {
        self.config
            .poll_interval
            .saturating_sub(self.last_poll.elapsed().as_secs())
    }
}
