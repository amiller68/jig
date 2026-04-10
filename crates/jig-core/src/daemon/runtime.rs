//! DaemonRuntime — owns actor channels and thread handles.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::context::RepoContext;
use crate::registry::RepoRegistry;

use super::messages::*;
use super::triage_tracker::TriageTracker;
use super::{github_actor, issue_actor, nudge_actor, prune_actor, spawn_actor, sync_actor};

/// Timer info for display in the ps watch footer.
#[derive(Debug, Clone)]
pub struct TimerInfo {
    /// Seconds until the next git sync fires.
    pub sync_remaining: u64,
    /// Seconds until the next issue poll fires (None if auto-spawn disabled).
    pub poll_remaining: Option<u64>,
}

/// Runtime configuration for the daemon actors.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Max concurrent workers for auto-spawn.
    pub max_concurrent_workers: usize,
    /// Seconds between issue polls.
    pub auto_spawn_interval: u64,
    /// Seconds between git syncs.
    pub sync_interval: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_workers: 3,
            auto_spawn_interval: 120,
            sync_interval: 60,
        }
    }
}

/// Owns actor channels and thread handles for the non-blocking daemon loop.
pub struct DaemonRuntime {
    // Sync actor
    sync_tx: flume::Sender<SyncRequest>,
    sync_rx: flume::Receiver<SyncComplete>,
    sync_pending: bool,
    last_sync: Instant,

    // GitHub actor
    github_tx: flume::Sender<GitHubRequest>,
    github_rx: flume::Receiver<GitHubResponse>,
    github_cache: HashMap<String, GitHubResponse>,
    /// When each worker's GitHub request was last queued, to throttle API calls.
    github_last_requested: HashMap<String, Instant>,

    // Issue actor
    issue_tx: flume::Sender<IssueRequest>,
    issue_rx: flume::Receiver<IssueResponse>,
    issue_pending: bool,
    last_issue_poll: Instant,

    // Prune actor
    prune_tx: flume::Sender<PruneRequest>,
    prune_rx: flume::Receiver<PruneComplete>,
    prune_pending: bool,

    // Spawn actor
    spawn_tx: flume::Sender<SpawnRequest>,
    spawn_rx: flume::Receiver<SpawnComplete>,
    spawn_pending: bool,
    /// Worker names currently being spawned in the background (for display).
    spawning_workers: Vec<String>,

    // Nudge actor
    nudge_tx: flume::Sender<NudgeRequest>,
    nudge_rx: flume::Receiver<NudgeComplete>,

    config: RuntimeConfig,

    /// Tracks in-flight triage workers to prevent duplicate spawns.
    triage_tracker: TriageTracker,

    /// Whether the first inline issue poll has been performed.
    first_poll_done: bool,

    // Thread handles (kept alive for clean shutdown)
    _handles: Vec<std::thread::JoinHandle<()>>,
}

impl DaemonRuntime {
    /// Create a new runtime, spawning all actor threads.
    pub fn new(config: RuntimeConfig) -> Self {
        let (sync_req_tx, sync_req_rx) = flume::bounded(1);
        let (sync_resp_tx, sync_resp_rx) = flume::bounded(1);
        let sync_handle = sync_actor::spawn(sync_req_rx, sync_resp_tx);

        let (gh_req_tx, gh_req_rx) = flume::bounded(16);
        let (gh_resp_tx, gh_resp_rx) = flume::bounded(16);
        let gh_handle = github_actor::spawn(gh_req_rx, gh_resp_tx);

        let (issue_req_tx, issue_req_rx) = flume::bounded(1);
        let (issue_resp_tx, issue_resp_rx) = flume::bounded(1);
        let issue_handle = issue_actor::spawn(issue_req_rx, issue_resp_tx);

        let (prune_req_tx, prune_req_rx) = flume::bounded(1);
        let (prune_resp_tx, prune_resp_rx) = flume::bounded(1);
        let prune_handle = prune_actor::spawn(prune_req_rx, prune_resp_tx);

        let (spawn_req_tx, spawn_req_rx) = flume::bounded(1);
        let (spawn_resp_tx, spawn_resp_rx) = flume::bounded(1);
        let spawn_handle = spawn_actor::spawn(spawn_req_rx, spawn_resp_tx);

        let (nudge_req_tx, nudge_req_rx) = flume::bounded(16);
        let (nudge_resp_tx, nudge_resp_rx) = flume::bounded(16);
        let nudge_handle = nudge_actor::spawn(nudge_req_rx, nudge_resp_tx);

        // Start with past timestamps so first tick triggers sync/poll immediately
        let past = Instant::now();

        Self {
            sync_tx: sync_req_tx,
            sync_rx: sync_resp_rx,
            sync_pending: false,
            last_sync: past - std::time::Duration::from_secs(config.sync_interval + 1),

            github_tx: gh_req_tx,
            github_rx: gh_resp_rx,
            github_cache: HashMap::new(),
            github_last_requested: HashMap::new(),

            issue_tx: issue_req_tx,
            issue_rx: issue_resp_rx,
            issue_pending: false,
            last_issue_poll: past - std::time::Duration::from_secs(config.auto_spawn_interval + 1),

            prune_tx: prune_req_tx,
            prune_rx: prune_resp_rx,
            prune_pending: false,

            spawn_tx: spawn_req_tx,
            spawn_rx: spawn_resp_rx,
            spawn_pending: false,
            spawning_workers: Vec::new(),

            nudge_tx: nudge_req_tx,
            nudge_rx: nudge_resp_rx,

            config,
            triage_tracker: TriageTracker::new(),
            first_poll_done: false,
            _handles: vec![
                sync_handle,
                gh_handle,
                issue_handle,
                prune_handle,
                spawn_handle,
                nudge_handle,
            ],
        }
    }

    /// Trigger a git sync if the interval has elapsed and no sync is pending.
    ///
    /// When `repo_filter` is set, only sync repos matching that name.
    pub fn maybe_trigger_sync(&mut self, registry: &RepoRegistry, repo_filter: Option<&str>) {
        if self.sync_pending {
            return;
        }
        if self.last_sync.elapsed().as_secs() < self.config.sync_interval {
            return;
        }
        let repos: Vec<(String, PathBuf, String)> = registry
            .filtered_repos(repo_filter)
            .into_iter()
            .filter_map(|entry| {
                let name = entry.path.file_name()?.to_string_lossy().to_string();
                let base = RepoContext::resolve_base_branch_for(&entry.path)
                    .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
                Some((name, entry.path.clone(), base))
            })
            .collect();

        if repos.is_empty() {
            return;
        }

        if self.sync_tx.try_send(SyncRequest { repos }).is_ok() {
            self.sync_pending = true;
            tracing::debug!("triggered sync");
        }
    }

    /// Drain any completed sync response (non-blocking).
    pub fn drain_sync(&mut self) -> Option<SyncComplete> {
        match self.sync_rx.try_recv() {
            Ok(result) => {
                self.sync_pending = false;
                self.last_sync = Instant::now();
                for (repo, err) in &result.errors {
                    tracing::debug!(repo = %repo, "sync error: {}", err);
                }
                Some(result)
            }
            Err(_) => None,
        }
    }

    /// Send a PR check request to the GitHub actor (non-blocking).
    ///
    /// Throttles requests to once per 60s per worker to align with `gh api --cache 60s`.
    /// This prevents spamming the GitHub API on every 2-second tick.
    pub fn request_pr_check(
        &mut self,
        worker_key: &str,
        repo_name: &str,
        branch: &str,
        pr_url: Option<&str>,
    ) {
        // Throttle: skip if we requested this worker's PR check within the last 60s
        const GITHUB_POLL_INTERVAL: Duration = Duration::from_secs(60);
        if let Some(last) = self.github_last_requested.get(worker_key) {
            if last.elapsed() < GITHUB_POLL_INTERVAL {
                return;
            }
        }

        let previous_is_draft = self
            .github_cache
            .get(worker_key)
            .map(|r| r.is_draft)
            .unwrap_or(false);
        if self
            .github_tx
            .try_send(GitHubRequest {
                worker_key: worker_key.to_string(),
                repo_name: repo_name.to_string(),
                branch: branch.to_string(),
                pr_url: pr_url.map(|s| s.to_string()),
                previous_is_draft,
            })
            .is_ok()
        {
            self.github_last_requested
                .insert(worker_key.to_string(), Instant::now());
        }
    }

    /// Drain all pending GitHub responses into the cache (non-blocking).
    pub fn drain_github(&mut self) {
        while let Ok(resp) = self.github_rx.try_recv() {
            self.github_cache.insert(resp.worker_key.clone(), resp);
        }
    }

    /// Get cached PR info for a worker.
    pub fn get_cached_pr(&self, worker_key: &str) -> Option<&GitHubResponse> {
        self.github_cache.get(worker_key)
    }

    /// Trigger an issue poll if the interval has elapsed and no poll is pending.
    ///
    /// When `repo_filter` is set, only poll repos matching that name. Each repo's
    /// own `jig.toml` controls whether auto-spawn is enabled (via
    /// `issues.auto_spawn_labels`) and the worker budget.
    pub fn maybe_trigger_issue_poll(
        &mut self,
        registry: &RepoRegistry,
        existing_workers: &[(String, String)],
        repo_filter: Option<&str>,
    ) {
        if self.issue_pending {
            return;
        }
        if self.last_issue_poll.elapsed().as_secs() < self.config.auto_spawn_interval {
            return;
        }

        let repos: Vec<(std::path::PathBuf, String)> = registry
            .filtered_repos(repo_filter)
            .into_iter()
            .map(|entry| {
                let base = RepoContext::resolve_base_branch_for(&entry.path)
                    .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
                (entry.path.clone(), base)
            })
            .collect();

        if repos.is_empty() {
            return;
        }

        let repo_count = repos.len();
        let req = IssueRequest {
            repos,
            existing_workers: existing_workers.to_vec(),
        };

        if self.issue_tx.try_send(req).is_ok() {
            self.issue_pending = true;
            tracing::debug!(repos = repo_count, "triggered issue poll");
        }
    }

    /// Drain any completed issue poll response (non-blocking).
    pub fn drain_issues(&mut self) -> Option<IssueResponse> {
        match self.issue_rx.try_recv() {
            Ok(response) => {
                self.issue_pending = false;
                self.last_issue_poll = Instant::now();
                if !response.spawnable.is_empty() {
                    tracing::info!(count = response.spawnable.len(), "found spawnable issues");
                }
                if !response.triageable.is_empty() {
                    tracing::info!(count = response.triageable.len(), "found triageable issues");
                }
                Some(response)
            }
            Err(_) => None,
        }
    }

    /// Get a mutable reference to the triage tracker.
    pub fn triage_tracker_mut(&mut self) -> &mut TriageTracker {
        &mut self.triage_tracker
    }

    /// Get a reference to the triage tracker.
    pub fn triage_tracker(&self) -> &TriageTracker {
        &self.triage_tracker
    }

    /// Send prune targets to the prune actor (non-blocking).
    pub fn send_prune(&mut self, targets: Vec<PruneTarget>) {
        if self.prune_pending || targets.is_empty() {
            return;
        }
        if self.prune_tx.try_send(PruneRequest { targets }).is_ok() {
            self.prune_pending = true;
            tracing::debug!("triggered prune");
        }
    }

    /// Drain any completed prune response (non-blocking).
    pub fn drain_prune(&mut self) -> Option<PruneComplete> {
        match self.prune_rx.try_recv() {
            Ok(result) => {
                self.prune_pending = false;
                Some(result)
            }
            Err(_) => None,
        }
    }

    /// Whether a prune request is currently in flight.
    pub fn prune_pending(&self) -> bool {
        self.prune_pending
    }

    /// Send spawnable issues to the spawn actor (non-blocking).
    pub fn send_spawn(&mut self, issues: Vec<SpawnableIssue>) {
        if self.spawn_pending || issues.is_empty() {
            return;
        }
        self.spawning_workers = issues.iter().map(|i| i.worker_name.clone()).collect();
        if self.spawn_tx.try_send(SpawnRequest { issues }).is_ok() {
            self.spawn_pending = true;
            tracing::debug!("triggered spawn");
        } else {
            self.spawning_workers.clear();
        }
    }

    /// Drain any completed spawn response (non-blocking).
    pub fn drain_spawn(&mut self) -> Option<SpawnComplete> {
        match self.spawn_rx.try_recv() {
            Ok(result) => {
                self.spawn_pending = false;
                self.spawning_workers.clear();
                Some(result)
            }
            Err(_) => None,
        }
    }

    /// Whether a spawn request is currently in flight.
    pub fn spawn_pending(&self) -> bool {
        self.spawn_pending
    }

    /// Worker names currently being spawned in the background.
    pub fn spawning_workers(&self) -> &[String] {
        &self.spawning_workers
    }

    /// Send a nudge request to the nudge actor (non-blocking).
    /// Drops the request on backpressure (nudges are best-effort).
    pub fn send_nudge(&self, req: NudgeRequest) {
        if self.nudge_tx.try_send(req).is_err() {
            tracing::debug!("nudge channel full, dropping nudge request");
        }
    }

    /// Drain all completed nudge responses (non-blocking).
    pub fn drain_nudges(&self) -> Vec<NudgeComplete> {
        let mut results = Vec::new();
        while let Ok(resp) = self.nudge_rx.try_recv() {
            results.push(resp);
        }
        results
    }

    /// Get runtime config reference.
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Whether the first inline issue poll still needs to run.
    pub fn should_first_poll(&self) -> bool {
        !self.first_poll_done
    }

    /// Mark the first inline issue poll as complete.
    pub fn mark_first_poll_done(&mut self) {
        self.first_poll_done = true;
    }

    /// Compute timer info for display.
    pub fn timer_info(&self) -> TimerInfo {
        let sync_elapsed = self.last_sync.elapsed().as_secs();
        let sync_remaining = self.config.sync_interval.saturating_sub(sync_elapsed);

        let poll_elapsed = self.last_issue_poll.elapsed().as_secs();
        let poll_remaining = Some(self.config.auto_spawn_interval.saturating_sub(poll_elapsed));

        TimerInfo {
            sync_remaining,
            poll_remaining,
        }
    }
}
