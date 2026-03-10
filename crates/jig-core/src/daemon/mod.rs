//! Daemon loop — the conductor that ties event derivation, dispatch, and execution together.
//!
//! Runs a periodic loop:
//! 1. Drain actor responses (non-blocking)
//! 2. Trigger background sync if interval elapsed
//! 3. For each worker: read events → derive state → compare → dispatch actions
//! 4. Execute actions (nudge via tmux, notify via hooks)
//! 5. Save updated state
//! 6. Trigger issue poll for auto-spawn
//! 7. Auto-spawn eligible workers

mod discovery;
pub mod github_actor;
pub mod issue_actor;
pub mod messages;
mod pr;
pub mod prune_actor;
pub mod runtime;
pub mod sync_actor;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::context::RepoContext;
use crate::dispatch::{dispatch_actions, Action};
use crate::error::Result;
use crate::events::{Event, EventLog, EventType, WorkerState};
use crate::global::{GlobalConfig, WorkerEntry, WorkersState};
use crate::notify::{NotificationEvent, Notifier};
use crate::nudge::{execute_nudge, NudgeType};
use crate::registry::{RepoEntry, RepoRegistry};
use crate::spawn::TaskStatus;
use crate::templates::TemplateEngine;
use crate::tmux::{TmuxClient, TmuxTarget};
use crate::worker::WorkerStatus;

use discovery::discover_workers;
use pr::{make_github_client, PrMonitor};

pub use messages::SpawnableIssue;
pub use runtime::{DaemonRuntime, RuntimeConfig};

/// Extract the branch name from a worker's event log (looks for Spawn event),
/// falling back to worker_name if no Spawn event exists.
fn extract_branch_name(events: &[Event], worker_name: &str) -> String {
    events
        .iter()
        .find(|e| e.event_type == EventType::Spawn)
        .and_then(|e| e.data.get("branch").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| worker_name.to_string())
}

/// Pre-computed display data for a worker, populated during tick so the render
/// callback can format output without any subprocess calls or file I/O.
#[derive(Debug, Clone)]
pub struct WorkerDisplayInfo {
    pub repo: String,
    pub name: String,
    pub branch: String,
    pub tmux_status: TaskStatus,
    pub worker_status: Option<WorkerStatus>,
    pub nudge_count: u32,
    pub commits_ahead: usize,
    pub is_dirty: bool,
    pub pr_url: Option<String>,
    pub issue_ref: Option<String>,
    pub pr_health: WorkerTickInfo,
    /// Whether the worker's PR is a draft (affects display and nudge behavior).
    pub is_draft: bool,
}

/// Per-worker PR health info collected during a tick.
#[derive(Debug, Clone, Default)]
pub struct WorkerTickInfo {
    /// Per-check outcomes: (check_name, has_problem).
    pub pr_checks: Vec<(String, bool)>,
    /// Error message if the GitHub client failed entirely.
    pub pr_error: Option<String>,
    /// Whether the worker has a PR at all.
    pub has_pr: bool,
}

/// Configuration for the daemon loop.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// How often to poll, in seconds.
    pub interval_seconds: u64,
    /// Whether to run once and exit (vs. looping).
    pub once: bool,
    /// Tmux session prefix (default: "jig-").
    pub session_prefix: String,
    /// Skip `git fetch` on each tick (unused with actors — kept for API compat).
    pub skip_sync: bool,
    /// If set, only process workers for this repo name.
    pub repo_filter: Option<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            once: false,
            session_prefix: "jig-".to_string(),
            skip_sync: false,
            repo_filter: None,
        }
    }
}

/// Result of a single daemon tick.
#[derive(Debug, Default)]
pub struct TickResult {
    pub workers_checked: usize,
    pub actions_dispatched: usize,
    pub nudges_sent: usize,
    pub notifications_sent: usize,
    pub errors: Vec<String>,
    /// Per-worker PR health info, keyed by "repo/worker".
    pub worker_info: HashMap<String, WorkerTickInfo>,
    /// Issues auto-spawned this tick.
    pub auto_spawned: Vec<String>,
    /// Workers pruned (worktree removed) this tick.
    pub pruned: Vec<String>,
    /// Pre-computed display data for the render callback (zero I/O).
    pub worker_display: Vec<WorkerDisplayInfo>,
}

/// The daemon orchestrator — holds references to shared infrastructure.
pub struct Daemon<'a> {
    config: &'a GlobalConfig,
    tmux: &'a TmuxClient,
    engine: &'a TemplateEngine<'a>,
    notifier: &'a Notifier,
    daemon_config: &'a DaemonConfig,
}

impl<'a> Daemon<'a> {
    pub fn new(
        config: &'a GlobalConfig,
        tmux: &'a TmuxClient,
        engine: &'a TemplateEngine<'a>,
        notifier: &'a Notifier,
        daemon_config: &'a DaemonConfig,
    ) -> Self {
        Self {
            config,
            tmux,
            engine,
            notifier,
            daemon_config,
        }
    }

    /// Look up the repo path from the registry by repo name.
    fn find_repo_path<'r>(registry: &'r RepoRegistry, repo_name: &str) -> Option<&'r RepoEntry> {
        registry.repos().iter().find(|e| {
            e.path
                .file_name()
                .map(|n| n.to_string_lossy() == repo_name)
                .unwrap_or(false)
        })
    }

    /// Get tmux status for a worker (session:window alive check).
    fn get_tmux_status(&self, repo_name: &str, worker_name: &str) -> TaskStatus {
        let session = format!("{}{}", self.daemon_config.session_prefix, repo_name);
        let target = TmuxTarget::new(&session, worker_name);
        if !self.tmux.has_session(&session) {
            return TaskStatus::NoSession;
        }
        if !self.tmux.has_window(&target) {
            return TaskStatus::NoWindow;
        }
        if self.tmux.pane_is_running(&target) {
            TaskStatus::Running
        } else {
            TaskStatus::Exited
        }
    }

    /// Execute a single tick of the daemon using actor-based runtime.
    /// If `quit` is set, the tick will bail early between workers.
    pub fn tick(&self, runtime: &mut DaemonRuntime, quit: &AtomicBool) -> Result<TickResult> {
        let mut result = TickResult::default();

        // 1. Drain all pending actor responses (non-blocking)
        runtime.drain_sync();
        runtime.drain_github();
        let spawnable = runtime.drain_issues();

        // Drain prune results from previous tick
        if let Some(prune_complete) = runtime.drain_prune() {
            for pr in prune_complete.results {
                if let Some(err) = pr.error {
                    result.errors.push(format!("prune {}: {}", pr.key, err));
                } else {
                    result.pruned.push(pr.key);
                }
            }
        }

        // Load current global state (previous worker states)
        let mut workers_state = WorkersState::load().unwrap_or_default();

        // Discover workers from repo registry
        let registry = RepoRegistry::load().unwrap_or_default();

        // 2. Trigger background sync if interval elapsed
        if !self.daemon_config.skip_sync {
            runtime.maybe_trigger_sync(&registry);
        }

        let mut worker_list = discover_workers(&registry);

        // Filter to single repo if configured
        if let Some(ref filter) = self.daemon_config.repo_filter {
            worker_list.retain(|(repo_name, _)| repo_name == filter);
        }

        tracing::debug!(count = worker_list.len(), "discovered workers");

        // 3. Process each worker
        let mut live_prune_targets = Vec::new();
        for (repo_name, worker_name) in &worker_list {
            if quit.load(Ordering::Relaxed) {
                break;
            }
            result.workers_checked += 1;
            let key = format!("{}/{}", repo_name, worker_name);

            match self.process_worker(
                repo_name,
                worker_name,
                &key,
                &mut workers_state,
                &registry,
                runtime,
            ) {
                Ok((actions, nudges, notifs, worker_tick_info, display_info, prune_targets)) => {
                    result.actions_dispatched += actions;
                    result.nudges_sent += nudges;
                    result.notifications_sent += notifs;
                    result.worker_info.insert(key.clone(), worker_tick_info);
                    result.worker_display.push(display_info);
                    live_prune_targets.extend(prune_targets);
                }
                Err(e) => {
                    result.errors.push(format!("{}: {}", key, e));
                }
            }
        }

        // Live path: send prune targets from Cleanup actions
        if !live_prune_targets.is_empty() {
            runtime.send_prune(live_prune_targets);
        }

        // Filter out terminal workers and workers with no tmux session
        result.worker_display.retain(|w| {
            let is_terminal = w
                .worker_status
                .as_ref()
                .map(|s| s.is_terminal())
                .unwrap_or(false);
            let tmux_dead = matches!(w.tmux_status, TaskStatus::NoSession | TaskStatus::NoWindow);
            !is_terminal && !tmux_dead
        });
        result.worker_display.sort_by(|a, b| a.name.cmp(&b.name));

        // Save updated state
        workers_state.save().unwrap_or_else(|e| {
            tracing::warn!("failed to save workers state: {}", e);
        });

        // Recovery path: scan github cache for merged/closed PRs with worktrees still on disk.
        // This catches workers whose PRs were merged/closed while the daemon was off.
        if !runtime.prune_pending() {
            let mut prune_targets = Vec::new();
            for (repo_name, worker_name) in &worker_list {
                let key = format!("{}/{}", repo_name, worker_name);
                if let Some(cached) = runtime.get_cached_pr(&key) {
                    if cached.pr_merged || cached.pr_closed {
                        if let Some(entry) = Self::find_repo_path(&registry, repo_name) {
                            let worktree_path =
                                crate::config::worktree_path(&entry.path, worker_name);
                            if worktree_path.exists() {
                                prune_targets.push(messages::PruneTarget {
                                    repo_path: entry.path.clone(),
                                    repo_name: repo_name.clone(),
                                    worker_name: worker_name.clone(),
                                });
                            }
                        }
                    }
                }
            }
            runtime.send_prune(prune_targets);
        }

        // 4. Trigger issue poll if auto-spawn enabled (polls all repos)
        runtime.maybe_trigger_issue_poll(&registry, &worker_list);

        // 5. Auto-spawn from drained spawnable issues
        for issue in spawnable {
            match self.auto_spawn_worker(&issue) {
                Ok(()) => {
                    tracing::info!(
                        worker = %issue.worker_name,
                        issue = %issue.issue_id,
                        "auto-spawned worker"
                    );
                    result.auto_spawned.push(issue.worker_name.clone());
                }
                Err(e) => {
                    result
                        .errors
                        .push(format!("auto-spawn {}: {}", issue.issue_id, e));
                }
            }
        }

        Ok(result)
    }

    /// Execute a single tick without a runtime (legacy path for non-watch mode).
    pub fn tick_once(&self) -> Result<TickResult> {
        let mut result = TickResult::default();

        let mut workers_state = WorkersState::load().unwrap_or_default();
        let registry = RepoRegistry::load().unwrap_or_default();

        if !self.daemon_config.skip_sync {
            self.sync_repos(&registry);
        }

        let mut worker_list = discover_workers(&registry);

        if let Some(ref filter) = self.daemon_config.repo_filter {
            worker_list.retain(|(repo_name, _)| repo_name == filter);
        }

        tracing::debug!(count = worker_list.len(), "discovered workers");

        for (repo_name, worker_name) in &worker_list {
            result.workers_checked += 1;
            let key = format!("{}/{}", repo_name, worker_name);

            match self.process_worker_blocking(
                repo_name,
                worker_name,
                &key,
                &mut workers_state,
                &registry,
            ) {
                Ok((actions, nudges, notifs, worker_tick_info)) => {
                    result.actions_dispatched += actions;
                    result.nudges_sent += nudges;
                    result.notifications_sent += notifs;
                    result.worker_info.insert(key.clone(), worker_tick_info);
                }
                Err(e) => {
                    result.errors.push(format!("{}: {}", key, e));
                }
            }
        }

        workers_state.save().unwrap_or_else(|e| {
            tracing::warn!("failed to save workers state: {}", e);
        });

        // Auto-spawn: poll all repos for spawnable issues (blocking).
        // Each repo's jig.toml controls auto_spawn and max_concurrent_workers.
        {
            let repos: Vec<(std::path::PathBuf, String)> = registry
                .repos()
                .iter()
                .map(|entry| {
                    let base = RepoContext::resolve_base_branch_for(&entry.path)
                        .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
                    (entry.path.clone(), base)
                })
                .collect();

            if !repos.is_empty() {
                let req = messages::IssueRequest {
                    repos,
                    existing_workers: worker_list.clone(),
                };

                for issue in issue_actor::process_request(&req) {
                    match self.auto_spawn_worker(&issue) {
                        Ok(()) => {
                            tracing::info!(
                                worker = %issue.worker_name,
                                issue = %issue.issue_id,
                                "auto-spawned worker"
                            );
                            result.auto_spawned.push(issue.worker_name.clone());
                        }
                        Err(e) => {
                            result
                                .errors
                                .push(format!("auto-spawn {}: {}", issue.issue_id, e));
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Process a single worker using cached PR data from the runtime.
    fn process_worker(
        &self,
        repo_name: &str,
        worker_name: &str,
        key: &str,
        workers_state: &mut WorkersState,
        registry: &RepoRegistry,
        runtime: &DaemonRuntime,
    ) -> Result<(
        usize,
        usize,
        usize,
        WorkerTickInfo,
        WorkerDisplayInfo,
        Vec<messages::PruneTarget>,
    )> {
        let event_log = EventLog::for_worker(repo_name, worker_name)?;
        let events = event_log.read_all()?;
        let new_state = WorkerState::reduce(&events, &self.config.health);
        let branch_name = extract_branch_name(&events, worker_name);

        // Use cached GitHub data — request a check for next tick if needed
        let mut worker_tick_info = WorkerTickInfo::default();
        let mut is_draft = false;

        if let Some(cached) = runtime.get_cached_pr(key) {
            worker_tick_info.has_pr = cached.pr_url.is_some();
            if let Some(ref err) = cached.pr_error {
                worker_tick_info.pr_error = Some(err.clone());
            }
            worker_tick_info.pr_checks = cached.pr_checks.clone();
            is_draft = cached.is_draft;

            // If PR was discovered by the actor but we don't have it in events, emit PrOpened
            if cached.pr_url.is_some() && new_state.pr_url.is_none() {
                if let Some(ref url) = cached.pr_url {
                    let pr_number = url.rsplit('/').next().unwrap_or("0");
                    let event = Event::new(EventType::PrOpened)
                        .with_field("pr_url", url.as_str())
                        .with_field("pr_number", pr_number);
                    if let Err(e) = event_log.append(&event) {
                        tracing::warn!(worker = key, error = %e, "failed to emit PrOpened event");
                    }
                }
            }
        }

        // Request PR check for next tick if worker is active
        if !new_state.status.is_terminal() {
            runtime.request_pr_check(key, repo_name, &branch_name, new_state.pr_url.as_deref());
        }

        // Re-read state with potential PrOpened event
        let events = event_log.read_all()?;
        let mut new_state = WorkerState::reduce(&events, &self.config.health);

        let old_state = workers_state
            .get_worker(key)
            .map(entry_to_worker_state)
            .unwrap_or_default();

        tracing::debug!(
            worker = key,
            old_status = old_state.status.as_str(),
            new_status = new_state.status.as_str(),
            "worker state"
        );

        let mut actions = dispatch_actions(worker_name, &old_state, &new_state, self.config);

        // Track review feedback count for nudge reset logic
        let mut current_review_feedback_count: Option<u32> = None;

        // Handle merged/closed PR from cached data
        if let Some(cached) = runtime.get_cached_pr(key) {
            current_review_feedback_count = cached.review_feedback_count;

            if cached.pr_merged && self.config.github.auto_cleanup_merged {
                actions.push(Action::Cleanup {
                    worker_id: worker_name.to_string(),
                });
                actions.push(Action::Notify {
                    worker_id: worker_name.to_string(),
                    message: "PR merged, worker cleaned up".to_string(),
                });
            } else if cached.pr_closed {
                actions.push(Action::Notify {
                    worker_id: worker_name.to_string(),
                    message: "PR closed without merge".to_string(),
                });
                if self.config.github.auto_cleanup_closed {
                    actions.push(Action::Cleanup {
                        worker_id: worker_name.to_string(),
                    });
                }
            } else if cached.is_draft {
                // Reset review nudge count if new feedback arrived
                let stored_count = workers_state
                    .get_worker(key)
                    .and_then(|e| e.review_feedback_count);
                if let Some(current) = cached.review_feedback_count {
                    let previous = stored_count.unwrap_or(0);
                    if current > previous {
                        tracing::info!(
                            worker = key,
                            previous,
                            current,
                            "new review feedback detected, resetting review nudge count"
                        );
                        new_state.nudge_counts.remove("review");
                    }
                }

                // Draft PR — dispatch nudges from cached check results
                // Non-draft PRs are in human review, skip nudges.
                for (check_name, has_problem) in &cached.pr_checks {
                    if !has_problem {
                        continue;
                    }
                    let nudge_type = match check_name.as_str() {
                        "ci" => NudgeType::Ci,
                        "conflicts" => NudgeType::Conflict,
                        "reviews" => NudgeType::Review,
                        "commits" => NudgeType::BadCommits,
                        _ => continue,
                    };
                    let count = new_state
                        .nudge_counts
                        .get(nudge_type.count_key())
                        .copied()
                        .unwrap_or(0);
                    if count >= self.config.health.max_nudges {
                        tracing::debug!(
                            worker = key,
                            nudge_type = nudge_type.count_key(),
                            count,
                            "PR nudge limit reached, skipping"
                        );
                        continue;
                    }
                    actions.push(Action::Nudge {
                        worker_id: worker_name.to_string(),
                        nudge_type,
                    });
                }
            }
        }

        let action_count = actions.len();
        let (nudge_count, notif_count, cleanup_prune_targets) = self.execute_actions(
            &actions,
            repo_name,
            &branch_name,
            key,
            &new_state,
            &event_log,
            registry,
        );

        // Update workers.json
        workers_state.set_worker(
            key,
            WorkerEntry {
                repo: repo_name.to_string(),
                branch: worker_name.to_string(),
                status: new_state.status.as_str().to_string(),
                issue: new_state.issue_ref.clone(),
                pr_url: new_state.pr_url.clone(),
                started_at: new_state.started_at.unwrap_or(0),
                last_event_at: new_state.last_event_at.unwrap_or(0),
                nudge_counts: new_state.nudge_counts.clone(),
                review_feedback_count: current_review_feedback_count,
            },
        );

        // Build display info — git checks are fast local ops
        let tmux_status = self.get_tmux_status(repo_name, worker_name);
        let (commits_ahead, is_dirty) =
            if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                let worktree_path = crate::config::worktree_path(&entry.path, worker_name);
                if worktree_path.exists() {
                    let base = RepoContext::resolve_base_branch_for(&entry.path)
                        .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
                    let ahead = crate::git::Repo::commits_ahead(&worktree_path, &base)
                        .unwrap_or_default()
                        .len();
                    let dirty =
                        crate::git::Repo::has_uncommitted_changes(&worktree_path).unwrap_or(false);
                    (ahead, dirty)
                } else {
                    (0, false)
                }
            } else {
                (0, false)
            };

        let nudges_total: u32 = new_state.nudge_counts.values().sum();
        let display_info = WorkerDisplayInfo {
            repo: repo_name.to_string(),
            name: worker_name.to_string(),
            branch: branch_name.clone(),
            tmux_status,
            worker_status: Some(new_state.status),
            nudge_count: nudges_total,
            commits_ahead,
            is_dirty,
            pr_url: new_state.pr_url.clone(),
            issue_ref: new_state.issue_ref.clone(),
            pr_health: worker_tick_info.clone(),
            is_draft,
        };

        Ok((
            action_count,
            nudge_count,
            notif_count,
            worker_tick_info,
            display_info,
            cleanup_prune_targets,
        ))
    }

    /// Process a single worker with blocking I/O (legacy path for one-shot mode).
    fn process_worker_blocking(
        &self,
        repo_name: &str,
        worker_name: &str,
        key: &str,
        workers_state: &mut WorkersState,
        registry: &RepoRegistry,
    ) -> Result<(usize, usize, usize, WorkerTickInfo)> {
        let event_log = EventLog::for_worker(repo_name, worker_name)?;
        let events = event_log.read_all()?;
        let mut new_state = WorkerState::reduce(&events, &self.config.health);
        let branch_name = extract_branch_name(&events, worker_name);

        // Proactively discover PR if not already known
        if new_state.pr_url.is_none() && !new_state.status.is_terminal() {
            if let Some(client) = make_github_client(repo_name, registry) {
                match client.get_pr_for_branch(&branch_name) {
                    Ok(Some(pr_info)) => {
                        let event = Event::new(EventType::PrOpened)
                            .with_field("pr_url", pr_info.url.as_str())
                            .with_field("pr_number", pr_info.number.to_string());
                        if let Err(e) = event_log.append(&event) {
                            tracing::warn!(worker = key, error = %e, "failed to emit PrOpened event");
                        } else {
                            tracing::info!(worker = key, pr_url = %pr_info.url, "discovered PR for branch");
                            if let Ok(updated_events) = event_log.read_all() {
                                new_state =
                                    WorkerState::reduce(&updated_events, &self.config.health);
                            }
                        }
                    }
                    Ok(None) => {
                        tracing::debug!(worker = key, branch = %branch_name, "no PR found for branch");
                    }
                    Err(e) => {
                        tracing::debug!(worker = key, error = %e, "PR discovery failed");
                    }
                }
            }
        }

        let old_state = workers_state
            .get_worker(key)
            .map(entry_to_worker_state)
            .unwrap_or_default();

        tracing::debug!(
            worker = key,
            old_status = old_state.status.as_str(),
            new_status = new_state.status.as_str(),
            "worker state"
        );

        let mut actions = dispatch_actions(worker_name, &old_state, &new_state, self.config);

        // Check PR lifecycle
        let mut worker_tick_info = WorkerTickInfo::default();
        let mut current_review_feedback_count: Option<u32> = None;
        if !new_state.status.is_terminal() {
            if let Some(pr_url) = new_state.pr_url.clone() {
                worker_tick_info.has_pr = true;
                let stored_review_feedback_count = workers_state
                    .get_worker(key)
                    .and_then(|e| e.review_feedback_count);
                match make_github_client(repo_name, registry) {
                    Some(client) => {
                        let monitor = PrMonitor::new(&client, self.config);
                        let pr_result = monitor.check_lifecycle(
                            worker_name,
                            &branch_name,
                            &pr_url,
                            &mut new_state,
                            stored_review_feedback_count,
                            &mut actions,
                        );
                        current_review_feedback_count = pr_result.review_feedback_count;
                        worker_tick_info.pr_checks = pr_result
                            .checks
                            .into_iter()
                            .map(|c| (c.name.to_string(), c.has_problem))
                            .collect();
                    }
                    None => {
                        worker_tick_info.pr_error = Some("GitHub client unavailable".to_string());
                    }
                }
            }
        }

        let action_count = actions.len();
        let (nudge_count, notif_count, _prune_targets) = self.execute_actions(
            &actions,
            repo_name,
            &branch_name,
            key,
            &new_state,
            &event_log,
            registry,
        );

        workers_state.set_worker(
            key,
            WorkerEntry {
                repo: repo_name.to_string(),
                branch: worker_name.to_string(),
                status: new_state.status.as_str().to_string(),
                issue: new_state.issue_ref.clone(),
                pr_url: new_state.pr_url.clone(),
                started_at: new_state.started_at.unwrap_or(0),
                last_event_at: new_state.last_event_at.unwrap_or(0),
                nudge_counts: new_state.nudge_counts.clone(),
                review_feedback_count: current_review_feedback_count,
            },
        );

        Ok((action_count, nudge_count, notif_count, worker_tick_info))
    }

    /// Execute dispatched actions, returning (nudge_count, notif_count, prune_targets).
    #[allow(clippy::too_many_arguments)]
    fn execute_actions(
        &self,
        actions: &[Action],
        repo_name: &str,
        branch_name: &str,
        key: &str,
        new_state: &WorkerState,
        event_log: &EventLog,
        registry: &RepoRegistry,
    ) -> (usize, usize, Vec<messages::PruneTarget>) {
        let mut nudge_count = 0;
        let mut notif_count = 0;
        let mut prune_targets = Vec::new();

        for action in actions {
            match action {
                Action::Nudge {
                    worker_id: _,
                    nudge_type,
                } => {
                    let target = TmuxTarget::new(
                        format!("{}{}", self.daemon_config.session_prefix, repo_name),
                        branch_name.to_string(),
                    );

                    if self.tmux.has_window(&target) {
                        if !self.tmux.pane_is_running(&target) {
                            tracing::debug!(
                                worker = key,
                                "no command running in pane, skipping nudge"
                            );
                            continue;
                        }
                        if let Err(e) = execute_nudge(
                            &target,
                            *nudge_type,
                            new_state,
                            self.config,
                            self.engine,
                            self.tmux,
                            event_log,
                        ) {
                            tracing::warn!("nudge failed for {}: {}", key, e);
                        } else {
                            tracing::info!(
                                worker = key,
                                nudge_type = nudge_type.count_key(),
                                "nudge delivered"
                            );
                            nudge_count += 1;
                        }
                    } else {
                        tracing::debug!(
                            worker = key,
                            nudge_type = nudge_type.count_key(),
                            session = %target.session,
                            window = %target.window,
                            "tmux window not found, skipping nudge"
                        );
                    }
                }
                Action::Notify { worker_id, message } => {
                    tracing::info!(worker = key, message = %message, "notification sent");
                    let event = NotificationEvent::NeedsIntervention {
                        repo: repo_name.to_string(),
                        worker: worker_id.clone(),
                        reason: message.clone(),
                    };
                    if let Err(e) = self.notifier.emit(event) {
                        tracing::warn!("notification failed for {}: {}", worker_id, e);
                    }
                    notif_count += 1;
                }
                Action::Restart { worker_id, reason } => {
                    tracing::info!(
                        "restart requested for {}: {} (not yet implemented)",
                        worker_id,
                        reason
                    );
                }
                Action::Cleanup { worker_id } => {
                    let tmux_target = TmuxTarget::new(
                        format!("{}{}", self.daemon_config.session_prefix, repo_name),
                        branch_name.to_string(),
                    );

                    if self.tmux.has_window(&tmux_target) {
                        if let Err(e) = self.tmux.kill_window(&tmux_target) {
                            tracing::warn!("failed to kill window for {}: {}", worker_id, e);
                        }
                    }

                    let event = Event::new(EventType::Terminal).with_field("terminal", "archived");
                    if let Err(e) = event_log.append(&event) {
                        tracing::warn!("failed to emit cleanup event for {}: {}", key, e);
                    }

                    // Queue worktree for pruning
                    if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                        prune_targets.push(messages::PruneTarget {
                            repo_path: entry.path.clone(),
                            repo_name: repo_name.to_string(),
                            worker_name: worker_id.clone(),
                        });
                    }

                    tracing::info!("cleaned up worker {}", worker_id);
                }
            }
        }

        (nudge_count, notif_count, prune_targets)
    }

    /// Auto-spawn a worker for an issue.
    fn auto_spawn_worker(&self, issue: &SpawnableIssue) -> Result<()> {
        use crate::config::JIG_DIR;

        let repo_root = &issue.repo_root;
        let repo = crate::git::Repo::open(repo_root)?;
        let git_common_dir = repo.common_dir();
        let worktrees_dir = repo_root.join(JIG_DIR);
        let worktree_path = crate::config::worktree_path(repo_root, &issue.worker_name);

        if worktree_path.exists() {
            tracing::debug!(worker = %issue.worker_name, "worktree already exists, skipping");
            return Ok(());
        }

        // Ensure .jig is gitignored
        crate::git::ensure_worktrees_excluded(&git_common_dir)?;

        // Create parent directories
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Resolve base branch
        let base_branch = RepoContext::resolve_base_branch_for(repo_root)
            .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());

        // Create the worktree
        let branch = &issue.worker_name;
        repo.create_worktree(&worktree_path, branch, &base_branch)?;

        // Copy configured files
        let copy_files = crate::config::get_copy_files(repo_root)?;
        if !copy_files.is_empty() {
            crate::config::copy_worktree_files(repo_root, &worktree_path, &copy_files)?;
        }

        // Run on-create hook
        crate::config::run_on_create_hook_for_repo(repo_root, &worktree_path)?;

        // Build repo context for registration
        let repo_name = repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let session_name = format!("jig-{}", repo_name);
        let repo_ctx = RepoContext {
            repo_root: repo_root.clone(),
            worktrees_dir,
            git_common_dir,
            base_branch: base_branch.clone(),
            session_name,
        };

        // Register worker with issue context
        let context = format!("{}\n\n{}", issue.issue_title, issue.issue_body);
        crate::spawn::register(
            &repo_ctx,
            &issue.worker_name,
            branch,
            Some(&context),
            Some(&issue.issue_id),
        )?;

        // Launch tmux window — always auto-start for daemon-spawned workers,
        // since the whole point of auto-spawn is autonomous execution.
        crate::spawn::launch_tmux_window(
            &repo_ctx,
            &issue.worker_name,
            &worktree_path,
            true,
            Some(&context),
        )?;

        Ok(())
    }

    /// Fetch the configured base branch for each registered repo (blocking).
    fn sync_repos(&self, registry: &RepoRegistry) {
        for entry in registry.repos() {
            if !entry.path.exists() {
                continue;
            }
            let base = RepoContext::resolve_base_branch_for(&entry.path)
                .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
            let (remote, branch) = base.split_once('/').unwrap_or(("origin", &base));

            match std::process::Command::new("git")
                .args(["fetch", remote, branch])
                .current_dir(&entry.path)
                .stdin(std::process::Stdio::null())
                .output()
            {
                Ok(o) if o.status.success() => {
                    tracing::debug!(repo = %entry.path.display(), "fetched {}", base);
                }
                Ok(o) => {
                    tracing::debug!(
                        repo = %entry.path.display(),
                        "fetch failed: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    );
                }
                Err(e) => {
                    tracing::debug!(repo = %entry.path.display(), "fetch failed: {}", e);
                }
            }
        }
    }
}

/// Build a Notifier from global config.
fn make_notifier(global_config: &GlobalConfig) -> Result<Notifier> {
    let queue = crate::notify::NotificationQueue::global()?;
    Ok(Notifier::new(global_config.notify.clone(), queue))
}

/// Run the daemon loop with a per-tick callback and actor runtime.
///
/// The callback receives each `TickResult` and returns `true` to continue or `false` to stop.
/// The callback is responsible for any inter-tick delay (sleep, keypress polling, etc.).
///
/// A shared `quit` flag is provided so that external code (e.g. a key-polling thread)
/// can signal the tick to bail early between workers.
pub fn run_with<F>(
    daemon_config: &DaemonConfig,
    runtime_config: RuntimeConfig,
    mut on_tick: F,
) -> Result<Arc<AtomicBool>>
where
    F: FnMut(&TickResult, &Arc<AtomicBool>) -> bool,
{
    let global_config = GlobalConfig::load()?;
    let tmux = TmuxClient::new();
    let engine = TemplateEngine::new();
    let notifier = make_notifier(&global_config)?;
    let daemon = Daemon::new(&global_config, &tmux, &engine, &notifier, daemon_config);

    let mut runtime = DaemonRuntime::new(runtime_config);
    let quit = Arc::new(AtomicBool::new(false));

    loop {
        match daemon.tick(&mut runtime, &quit) {
            Ok(tick) => {
                if tick.workers_checked > 0 || !tick.errors.is_empty() {
                    tracing::info!(
                        workers = tick.workers_checked,
                        actions = tick.actions_dispatched,
                        nudges = tick.nudges_sent,
                        notifications = tick.notifications_sent,
                        errors = tick.errors.len(),
                        "tick complete"
                    );
                }
                for err in &tick.errors {
                    tracing::warn!("worker error: {}", err);
                }
                if quit.load(Ordering::Relaxed) {
                    return Ok(quit);
                }
                let keep_going = on_tick(&tick, &quit);
                if daemon_config.once || !keep_going {
                    return Ok(quit);
                }
            }
            Err(e) => {
                tracing::error!("tick failed: {}", e);
                if daemon_config.once {
                    return Err(e);
                }
                if quit.load(Ordering::Relaxed) {
                    return Ok(quit);
                }
                std::thread::sleep(Duration::from_secs(daemon_config.interval_seconds));
            }
        }
    }
}

/// Run the daemon loop (simple blocking mode). Returns after one pass if `config.once` is true.
pub fn run(daemon_config: &DaemonConfig) -> Result<()> {
    let global_config = GlobalConfig::load()?;
    let tmux = TmuxClient::new();
    let engine = TemplateEngine::new();
    let notifier = make_notifier(&global_config)?;
    let daemon = Daemon::new(&global_config, &tmux, &engine, &notifier, daemon_config);

    loop {
        match daemon.tick_once() {
            Ok(tick) => {
                if tick.workers_checked > 0 || !tick.errors.is_empty() {
                    eprintln!(
                        "[tick] {} workers, {} actions, {} nudges, {} notifications, {} errors",
                        tick.workers_checked,
                        tick.actions_dispatched,
                        tick.nudges_sent,
                        tick.notifications_sent,
                        tick.errors.len(),
                    );
                }
                if daemon_config.once {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_secs(daemon_config.interval_seconds));
            }
            Err(e) => {
                tracing::error!("tick failed: {}", e);
                if daemon_config.once {
                    return Err(e);
                }
                std::thread::sleep(Duration::from_secs(daemon_config.interval_seconds));
            }
        }
    }
}

/// Convert a WorkerEntry (from workers.json) back to a WorkerState for comparison.
fn entry_to_worker_state(entry: &WorkerEntry) -> WorkerState {
    use crate::worker::WorkerStatus;

    let status = WorkerStatus::from_legacy(&entry.status);

    WorkerState {
        status,
        commit_count: 0,
        last_commit_at: None,
        pr_url: entry.pr_url.clone(),
        nudge_counts: entry.nudge_counts.clone(),
        issue_ref: entry.issue.clone(),
        started_at: Some(entry.started_at),
        last_event_at: Some(entry.last_event_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
        };
        let state = entry_to_worker_state(&entry);
        assert_eq!(state.status, crate::worker::WorkerStatus::Running);
        assert_eq!(state.pr_url.as_deref(), Some("https://github.com/pr/1"));
        assert_eq!(state.issue_ref.as_deref(), Some("features/my-task"));
    }

    #[test]
    fn daemon_config_defaults() {
        let config = DaemonConfig::default();
        assert_eq!(config.interval_seconds, 30);
        assert!(!config.once);
        assert_eq!(config.session_prefix, "jig-");
    }

    #[test]
    fn tick_result_defaults() {
        let result = TickResult::default();
        assert_eq!(result.workers_checked, 0);
        assert_eq!(result.actions_dispatched, 0);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn runtime_config_defaults() {
        let config = RuntimeConfig::default();
        assert!(!config.auto_spawn);
        assert_eq!(config.max_concurrent_workers, 3);
        assert_eq!(config.auto_spawn_interval, 120);
        assert_eq!(config.sync_interval, 60);
    }

    #[test]
    fn review_nudge_count_resets_on_new_feedback() {
        // Simulate: stored feedback count is 2, current is 5 (new feedback arrived)
        let mut nudge_counts: HashMap<String, u32> = HashMap::new();
        nudge_counts.insert("review".to_string(), 3); // exhausted

        let stored_review_feedback_count: Option<u32> = Some(2);
        let current_review_feedback_count: u32 = 5;

        // This mirrors the reset logic in process_worker
        let previous = stored_review_feedback_count.unwrap_or(0);
        if current_review_feedback_count > previous {
            nudge_counts.remove("review");
        }

        assert_eq!(nudge_counts.get("review"), None);
    }

    #[test]
    fn review_nudge_count_unchanged_when_no_new_feedback() {
        // Simulate: stored feedback count equals current (no new feedback)
        let mut nudge_counts: HashMap<String, u32> = HashMap::new();
        nudge_counts.insert("review".to_string(), 2);

        let stored_review_feedback_count: Option<u32> = Some(3);
        let current_review_feedback_count: u32 = 3;

        let previous = stored_review_feedback_count.unwrap_or(0);
        if current_review_feedback_count > previous {
            nudge_counts.remove("review");
        }

        assert_eq!(nudge_counts.get("review"), Some(&2));
    }

    #[test]
    fn review_nudge_count_resets_from_none_stored() {
        // Simulate: no stored count (first check), current has feedback
        let mut nudge_counts: HashMap<String, u32> = HashMap::new();
        nudge_counts.insert("review".to_string(), 1);

        let stored_review_feedback_count: Option<u32> = None;
        let current_review_feedback_count: u32 = 2;

        let previous = stored_review_feedback_count.unwrap_or(0);
        if current_review_feedback_count > previous {
            nudge_counts.remove("review");
        }

        assert_eq!(nudge_counts.get("review"), None);
    }
}
