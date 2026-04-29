//! Daemon loop — the conductor that ties actors together.
//!
//! Runs a periodic loop:
//! 1. First-tick inline spawn poll
//! 2. Send dispatch request (worker processing, nudges, notifications, state save)
//! 3. Trigger background sync + spawn + triage if poll interval elapsed
//! 4. Feed prune targets to prune actor

pub mod actors;
pub mod events;

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::config::registry::RepoRegistry;
use crate::config::JigToml;
use crate::config::{GlobalConfig, WorkersState};
use jig_core::error::Result;
use jig_core::prompt::Prompt;

type Worker = crate::worker::Worker<jig_core::mux::tmux::TmuxWindow>;

use actors::dispatch::DispatchActor;
use actors::prune::PruneActor;
use actors::spawn::SpawnActor;
use actors::sync::SyncActor;
use actors::triage::TriageActor;
use actors::{Actor, ActorHandle};

pub use actors::dispatch::{PrChecks, PrHealth, WorkerSnapshot};
pub use actors::triage::TriageEntry;

/// Configuration for the daemon.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// How often to poll, in seconds.
    pub interval_seconds: u64,
    /// Tmux session prefix (default: "jig-").
    pub session_prefix: String,
    /// If set, only process workers for this repo name.
    pub repo_filter: Option<String>,
    /// Maximum number of concurrent auto-spawned workers.
    pub max_concurrent_workers: usize,
    /// Seconds between sync + issue poll ticks.
    pub poll_interval: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            session_prefix: "jig-".to_string(),
            repo_filter: None,
            max_concurrent_workers: 3,
            poll_interval: 60,
        }
    }
}

/// The daemon — owns actors and drives the tick loop.
pub struct Daemon {
    pub sync: ActorHandle<SyncActor>,
    pub dispatch: ActorHandle<DispatchActor>,
    pub prune: ActorHandle<PruneActor>,
    pub spawn: ActorHandle<SpawnActor>,
    pub triage: ActorHandle<TriageActor>,

    config: DaemonConfig,
    last_poll: Instant,
}

impl Daemon {
    /// Create and start the daemon: runs recovery, logs startup event.
    pub fn start(config: DaemonConfig) -> Result<Self> {
        let global_config = GlobalConfig::load()?;
        startup_recovery(&global_config);
        let _notifier = make_notifier(&global_config)?;

        let last_poll = Instant::now() - Duration::from_secs(config.poll_interval + 1);
        Ok(Self {
            sync: ActorHandle::new(),
            dispatch: ActorHandle::new(),
            prune: ActorHandle::new(),
            spawn: ActorHandle::new(),
            triage: ActorHandle::new(),
            config,
            last_poll,
        })
    }

    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }

    /// Run the tick loop. Checks `quit` between ticks; calls `on_tick` after
    /// each successful tick — return `false` to stop.
    pub fn run<F>(&mut self, quit: &AtomicBool, mut on_tick: F)
    where
        F: FnMut(&Self) -> bool,
    {
        loop {
            match self.tick() {
                Ok(()) => {
                    if quit.load(Ordering::Relaxed) {
                        break;
                    }
                    if !on_tick(self) {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("tick failed: {}", e);
                    if quit.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(self.config.interval_seconds));
                }
            }
        }
        log_shutdown("normal");
    }

    /// Whether both sync and spawn poll are due.
    pub fn poll_is_due(&self) -> bool {
        !self.sync.is_pending()
            && !self.spawn.is_pending()
            && self.last_poll.elapsed().as_secs() >= self.config.poll_interval
    }

    /// Mark that a poll tick just fired.
    fn mark_polled(&mut self) {
        self.last_poll = Instant::now();
    }

    /// Seconds until the next poll tick.
    pub fn poll_remaining_secs(&self) -> u64 {
        self.config
            .poll_interval
            .saturating_sub(self.last_poll.elapsed().as_secs())
    }

    /// Fast-forward parent worktrees and nudge parent workers about new commits.
    fn update_parent_worktrees(&self, workers_state: &WorkersState, registry: &RepoRegistry) {
        let mut parent_branches: HashSet<(String, String)> = HashSet::new();
        for entry in workers_state.workers.values() {
            if entry.status == "merged" || entry.status == "archived" || entry.status == "failed" {
                continue;
            }
            if let Some(ref pb) = entry.parent_branch {
                parent_branches.insert((entry.repo.clone(), pb.clone()));
            }
        }

        if parent_branches.is_empty() {
            return;
        }

        for (repo_name, parent_branch) in &parent_branches {
            let parent_worker = workers_state.workers.iter().find_map(|(key, entry)| {
                if &entry.repo == repo_name && entry.branch == *parent_branch {
                    let worker_name = key.split('/').nth(1).unwrap_or(key);
                    Some((worker_name.to_string(), entry.branch.clone()))
                } else {
                    None
                }
            });

            let (worker_name, _branch_name) = match parent_worker {
                Some(pw) => pw,
                None => continue,
            };

            let repo_entry = match registry.repos().iter().find(|e| {
                e.path
                    .file_name()
                    .map(|n| n.to_string_lossy() == repo_name.as_str())
                    .unwrap_or(false)
            }) {
                Some(e) => e,
                None => continue,
            };

            let worktree_path = crate::config::worktree_path(&repo_entry.path, &worker_name);

            if worktree_path.exists() {
                let repo = match jig_core::git::Repo::open(&worktree_path) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(worker = %worker_name, repo = %repo_name,
                            "failed to open parent worktree repo: {}", e);
                        continue;
                    }
                };

                match repo.fast_forward_branch(&parent_branch.as_str().into(), true) {
                    Ok(true) => {
                        tracing::info!(worker = %worker_name, repo = %repo_name,
                            branch = %parent_branch, "pulled new commits into parent worktree");

                        let worker =
                            Worker::from_branch(&repo_entry.path, worker_name.as_str().into());
                        if worker.has_mux_window() {
                            let prompt = Prompt::new(
                                "Child work has been merged into your branch. \
                                 New commits are available. Run `git log --oneline -5` \
                                 to see what changed.",
                            )
                            .named("parent_update");
                            let wkey = worker.branch().to_string();
                            match worker.nudge(prompt) {
                                Ok(()) => {
                                    tracing::info!(worker = %wkey, "parent update nudge delivered")
                                }
                                Err(e) => {
                                    tracing::warn!(worker = %wkey, "parent update nudge failed: {}", e)
                                }
                            }
                        }
                    }
                    Ok(false) => {
                        tracing::debug!(worker = %worker_name, repo = %repo_name,
                            "parent worktree already up to date");
                    }
                    Err(e) => {
                        tracing::warn!(worker = %worker_name, repo = %repo_name,
                            "fast-forward failed in parent worktree: {}", e);
                    }
                }
            } else {
                let repo = match jig_core::git::Repo::open(&repo_entry.path) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(repo = %repo_name, branch = %parent_branch,
                            "failed to open main repo for bare branch update: {}", e);
                        continue;
                    }
                };

                match repo.fast_forward_branch(&parent_branch.as_str().into(), false) {
                    Ok(true) => {
                        tracing::info!(repo = %repo_name, branch = %parent_branch,
                            "fast-forwarded parent branch ref (no worktree)");
                        if let Err(e) = repo.push_branch(&parent_branch.as_str().into()) {
                            tracing::warn!(repo = %repo_name, branch = %parent_branch,
                                "push after bare fast-forward failed: {}", e);
                        }
                    }
                    Ok(false) => {
                        tracing::debug!(repo = %repo_name, branch = %parent_branch,
                            "parent branch ref already up to date (no worktree)");
                    }
                    Err(e) => {
                        tracing::warn!(repo = %repo_name, branch = %parent_branch,
                            "bare fast-forward failed for parent branch: {}", e);
                    }
                }
            }
        }
    }

    /// Execute a single tick of the daemon.
    pub fn tick(&mut self) -> Result<()> {
        let workers_state = WorkersState::load().unwrap_or_default();
        let registry = RepoRegistry::load().unwrap_or_default();

        self.update_parent_worktrees(&workers_state, &registry);

        // First-tick inline poll: run spawn synchronously so workers start immediately
        if self.spawn.actor().should_first_poll() {
            self.spawn.actor().mark_first_poll_done();

            let repos: Vec<jig_core::git::Repo> = registry
                .filtered_repos(self.config.repo_filter.as_deref())
                .into_iter()
                .filter_map(|entry| jig_core::git::Repo::open(&entry.path).ok())
                .collect();

            if !repos.is_empty() {
                let req = actors::spawn::SpawnRequest { repos };
                self.spawn.actor().handle(req);
            }
        }

        // Send dispatch request every tick (non-blocking — runs in background thread)
        if !self.dispatch.is_pending() {
            self.dispatch.send(actors::dispatch::DispatchRequest {
                session_prefix: self.config.session_prefix.clone(),
                repo_filter: self.config.repo_filter.clone(),
            });
        }

        // Feed prune targets from dispatch actor to prune actor
        let prune_targets = self.dispatch.actor().take_prune_targets();
        if !prune_targets.is_empty() && !self.prune.is_pending() {
            self.prune.send(actors::prune::PruneRequest {
                targets: prune_targets,
            });
        }

        // Trigger background sync + spawn + triage if interval elapsed
        if self.poll_is_due() {
            let filtered = registry.filtered_repos(self.config.repo_filter.as_deref());

            let sync_repos: Vec<(String, std::path::PathBuf)> = filtered
                .iter()
                .filter_map(|entry| {
                    let name = entry.path.file_name()?.to_string_lossy().to_string();
                    Some((name, entry.path.clone()))
                })
                .collect();
            if !sync_repos.is_empty() {
                self.sync
                    .send(actors::sync::SyncRequest { repos: sync_repos });
            }

            let spawn_repos: Vec<jig_core::git::Repo> = filtered
                .iter()
                .filter_map(|entry| jig_core::git::Repo::open(&entry.path).ok())
                .collect();
            let triage_repos: Vec<jig_core::git::Repo> = filtered
                .iter()
                .filter_map(|entry| jig_core::git::Repo::open(&entry.path).ok())
                .collect();
            if !spawn_repos.is_empty() {
                self.spawn
                    .send(actors::spawn::SpawnRequest { repos: spawn_repos });
            }
            if !triage_repos.is_empty() {
                self.triage.send(actors::triage::TriageRequest {
                    repos: triage_repos,
                });
            }

            self.mark_polled();
        }

        Ok(())
    }
}

/// Try to resume a worker whose tmux window is dead.
fn try_resume_worker(repo_root: &std::path::Path, worker_name: &str) -> Result<bool> {
    let worker = Worker::from_branch(repo_root, worker_name.into());
    if worker.has_mux_window() {
        return Ok(false);
    }
    let wt = worker.worktree()?;
    let jig_config = JigToml::load(repo_root)?.unwrap_or_default();
    let agent = jig_core::agents::Agent::from_name(&jig_config.agent.agent_type)
        .unwrap_or_else(|| jig_core::agents::Agent::from_kind(jig_core::agents::AgentKind::Claude))
        .with_disallowed_tools(jig_config.agent.disallowed_tools.clone());
    let prompt = Prompt::new(crate::worker::SPAWN_PREAMBLE).var(
        "task_context",
        "You were interrupted. Resume your previous task.",
    );
    Worker::resume(&wt, &agent, prompt)?;
    Ok(true)
}

/// Build a Notifier from global config.
fn make_notifier(global_config: &GlobalConfig) -> Result<crate::notify::Notifier> {
    let queue = crate::notify::NotificationQueue::global()?;
    Ok(crate::notify::Notifier::new(
        global_config.notify.clone(),
        queue,
    ))
}

/// Run startup recovery: log lifecycle event, detect crash, resume orphans.
fn startup_recovery(global_config: &GlobalConfig) {
    let log = match events::global() {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("failed to open daemon event log: {}", e);
            return;
        }
    };

    match log.read_all() {
        Ok(all) => {
            let state = events::DaemonState::reduce(&all);
            if state.previous_run_crashed() {
                tracing::warn!(
                    "previous daemon run did not shut down cleanly — checking for orphaned workers"
                );
            }
        }
        Err(e) => {
            tracing::warn!("failed to read daemon event log: {}", e);
        }
    }

    if let Err(e) = log.append(&events::Event::started()) {
        tracing::warn!("failed to write daemon Started event: {}", e);
    }

    if global_config.daemon.auto_recover {
        let registry = RepoRegistry::load().unwrap_or_default();
        let mut recovered = Vec::new();
        for entry in registry.repos() {
            let repo_name = entry
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let repo = match jig_core::git::Repo::open(&entry.path) {
                Ok(r) => r,
                Err(_) => continue,
            };
            for worker in Worker::discover(&repo) {
                if worker.is_orphaned() {
                    let branch = worker.branch().to_string();
                    match try_resume_worker(&entry.path, &branch) {
                        Ok(true) => {
                            tracing::info!(repo = %repo_name, worker = %branch, "recovered");
                            recovered.push((repo_name.clone(), branch));
                        }
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(repo = %repo_name, worker = %branch, error = %e, "recovery failed");
                        }
                    }
                }
            }
        }
        if !recovered.is_empty() {
            tracing::info!(
                count = recovered.len(),
                "recovered orphaned workers on startup"
            );
        }
    }
}

/// Log a graceful shutdown event.
fn log_shutdown(reason: &str) {
    let log = match events::global() {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("failed to open daemon event log: {}", e);
            return;
        }
    };
    if let Err(e) = log.append(&events::Event::stopped(reason)) {
        tracing::warn!("failed to write daemon Stopped event: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorkerEntry;
    use crate::worker::events::WorkerState;
    use crate::worker::WorkerStatus;
    use jig_core::issues::issue::IssueRef;
    use std::collections::HashMap;

    fn entry_to_worker_state(entry: &WorkerEntry) -> WorkerState {
        let status = WorkerStatus::from_legacy(&entry.status);
        WorkerState {
            status,
            branch: Some(entry.branch.clone()),
            commit_count: 0,
            last_commit_at: None,
            pr_url: entry.pr_url.clone(),
            nudge_counts: entry.nudge_counts.clone(),
            last_nudge_at: HashMap::new(),
            issue_ref: entry.issue.as_ref().map(IssueRef::new),
            started_at: Some(entry.started_at),
            last_event_at: Some(entry.last_event_at),
        }
    }

    #[test]
    fn entry_to_state_roundtrip() {
        let entry = WorkerEntry {
            repo: "test".to_string(),
            branch: "main".to_string(),
            status: "running".to_string(),
            issue: Some("features/my-task".to_string()),
            pr_url: Some("https://github.com/pr/1".to_string()),
            started_at: 1000,
            last_event_at: 2000,
            nudge_counts: HashMap::new(),
            review_feedback_count: None,
            parent_branch: None,
        };
        let state = entry_to_worker_state(&entry);
        assert_eq!(state.status, WorkerStatus::Running);
        assert_eq!(state.pr_url.as_deref(), Some("https://github.com/pr/1"));
        assert_eq!(state.issue_ref.as_deref(), Some("features/my-task"));
    }

    #[test]
    fn daemon_config_defaults() {
        let config = DaemonConfig::default();
        assert_eq!(config.interval_seconds, 30);
        assert_eq!(config.session_prefix, "jig-");
        assert_eq!(config.max_concurrent_workers, 3);
        assert_eq!(config.poll_interval, 60);
    }

    fn should_auto_complete(
        auto_complete_on_merge: bool,
        issue_ref: Option<&str>,
    ) -> Option<String> {
        if auto_complete_on_merge {
            issue_ref.map(|id| id.to_string())
        } else {
            None
        }
    }

    #[test]
    fn auto_complete_pushes_when_enabled_and_has_issue() {
        let result = should_auto_complete(true, Some("ENG-42"));
        assert_eq!(result, Some("ENG-42".to_string()));
    }

    #[test]
    fn auto_complete_skips_when_disabled() {
        let result = should_auto_complete(false, Some("ENG-42"));
        assert_eq!(result, None);
    }

    #[test]
    fn auto_complete_skips_when_no_issue() {
        let result = should_auto_complete(true, None);
        assert_eq!(result, None);
    }

    fn should_update_issue_status(current_status: jig_core::issues::issue::IssueStatus) -> bool {
        !matches!(
            current_status,
            jig_core::issues::issue::IssueStatus::Complete
        )
    }

    #[test]
    fn auto_complete_updates_in_progress_issue() {
        assert!(should_update_issue_status(
            jig_core::issues::issue::IssueStatus::InProgress
        ));
    }

    #[test]
    fn auto_complete_skips_already_complete_issue() {
        assert!(!should_update_issue_status(
            jig_core::issues::issue::IssueStatus::Complete
        ));
    }

    #[test]
    fn auto_complete_updates_planned_issue() {
        assert!(should_update_issue_status(
            jig_core::issues::issue::IssueStatus::Planned
        ));
    }
}
