//! DaemonRuntime — owns actor channels and thread handles.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::context::RepoContext;
use crate::registry::RepoRegistry;

use super::messages::*;
use super::{github_actor, issue_actor, prune_actor, sync_actor};

/// Runtime configuration for the daemon actors.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Whether auto-spawn is enabled.
    pub auto_spawn: bool,
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
            auto_spawn: false,
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

    // Issue actor
    issue_tx: flume::Sender<IssueRequest>,
    issue_rx: flume::Receiver<Vec<SpawnableIssue>>,
    issue_pending: bool,
    last_issue_poll: Instant,

    // Prune actor
    prune_tx: flume::Sender<PruneRequest>,
    prune_rx: flume::Receiver<PruneComplete>,
    prune_pending: bool,

    config: RuntimeConfig,

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

            issue_tx: issue_req_tx,
            issue_rx: issue_resp_rx,
            issue_pending: false,
            last_issue_poll: past - std::time::Duration::from_secs(config.auto_spawn_interval + 1),

            prune_tx: prune_req_tx,
            prune_rx: prune_resp_rx,
            prune_pending: false,

            config,
            _handles: vec![sync_handle, gh_handle, issue_handle, prune_handle],
        }
    }

    /// Trigger a git sync if the interval has elapsed and no sync is pending.
    pub fn maybe_trigger_sync(&mut self, registry: &RepoRegistry) {
        if self.sync_pending {
            return;
        }
        if self.last_sync.elapsed().as_secs() < self.config.sync_interval {
            return;
        }
        let repos: Vec<(String, PathBuf, String)> = registry
            .repos()
            .iter()
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
    pub fn request_pr_check(
        &self,
        worker_key: &str,
        repo_name: &str,
        branch: &str,
        pr_url: Option<&str>,
    ) {
        let _ = self.github_tx.try_send(GitHubRequest {
            worker_key: worker_key.to_string(),
            repo_name: repo_name.to_string(),
            branch: branch.to_string(),
            pr_url: pr_url.map(|s| s.to_string()),
        });
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

    /// Trigger an issue poll across all repos if auto-spawn is enabled and interval elapsed.
    pub fn maybe_trigger_issue_poll(
        &mut self,
        registry: &RepoRegistry,
        existing_workers: &[String],
    ) {
        if !self.config.auto_spawn {
            return;
        }
        if self.issue_pending {
            return;
        }
        if self.last_issue_poll.elapsed().as_secs() < self.config.auto_spawn_interval {
            return;
        }

        let repos: Vec<(std::path::PathBuf, String)> = registry
            .repos()
            .iter()
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
            max_concurrent_workers: self.config.max_concurrent_workers,
        };

        if self.issue_tx.try_send(req).is_ok() {
            self.issue_pending = true;
            tracing::debug!(repos = repo_count, "triggered issue poll");
        }
    }

    /// Drain any completed issue poll response (non-blocking).
    pub fn drain_issues(&mut self) -> Vec<SpawnableIssue> {
        match self.issue_rx.try_recv() {
            Ok(issues) => {
                self.issue_pending = false;
                self.last_issue_poll = Instant::now();
                if !issues.is_empty() {
                    tracing::info!(count = issues.len(), "found spawnable issues");
                }
                issues
            }
            Err(_) => vec![],
        }
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

    /// Get runtime config reference.
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }
}
