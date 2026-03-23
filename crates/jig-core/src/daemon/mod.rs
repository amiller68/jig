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
pub mod lifecycle;
pub mod messages;
pub mod nudge_actor;
mod pr;
pub mod prune_actor;
pub mod recovery;
pub mod runtime;
pub mod spawn_actor;
pub mod sync_actor;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::config::{JigToml, RepoHealthConfig, ResolvedNudgeConfig};
use crate::context::RepoContext;
use crate::dispatch::{dispatch_actions, Action, NotifyKind};
use crate::error::Result;
use crate::events::{Event, EventLog, EventType, WorkerState};
use crate::global::{GlobalConfig, HealthConfig, WorkerEntry, WorkersState};
use crate::notify::{NotificationEvent, Notifier};
use crate::nudge::{build_nudge_context, NudgeType};
use crate::registry::{RepoEntry, RepoRegistry};
use crate::spawn::TaskStatus;
use crate::templates::TemplateEngine;
use crate::tmux::{TmuxClient, TmuxTarget};
use crate::worker::WorkerStatus;

use discovery::discover_workers;
use pr::{make_github_client, PrMonitor};

pub use messages::SpawnableIssue;
pub use runtime::{DaemonRuntime, RuntimeConfig, TimerInfo};

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
    pub max_nudges: u32,
    pub commits_ahead: usize,
    pub is_dirty: bool,
    pub pr_url: Option<String>,
    pub issue_ref: Option<String>,
    pub pr_health: WorkerTickInfo,
    /// Whether the worker's PR is a draft (affects display and nudge behavior).
    pub is_draft: bool,
    /// Seconds until the next nudge cooldown expires (min across all active types).
    pub nudge_cooldown_remaining: Option<u64>,
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
    /// Issues auto-spawned this tick (completed).
    pub auto_spawned: Vec<String>,
    /// Worker names currently being spawned in the background.
    pub spawning: Vec<String>,
    /// Workers pruned (worktree removed) this tick.
    pub pruned: Vec<String>,
    /// Pre-computed display data for the render callback (zero I/O).
    pub worker_display: Vec<WorkerDisplayInfo>,
    /// Nudge messages delivered this tick: (worker_name, nudge_type, message_text).
    pub nudge_messages: Vec<(String, String, String)>,
    /// Timer info for the daemon's sync and poll intervals.
    pub timer_info: Option<TimerInfo>,
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

    /// Load per-repo health config from jig.toml, falling back to defaults.
    fn load_repo_health_config(registry: &RepoRegistry, repo_name: &str) -> RepoHealthConfig {
        Self::find_repo_path(registry, repo_name)
            .and_then(|entry| JigToml::load(&entry.path).ok().flatten())
            .map(|toml| toml.health)
            .unwrap_or_default()
    }

    /// Build a resolver closure for nudge type config resolution.
    fn make_nudge_resolver(
        repo_health: &RepoHealthConfig,
        global_health: &HealthConfig,
    ) -> impl Fn(&str) -> ResolvedNudgeConfig {
        let repo_health = repo_health.clone();
        let global_health = global_health.clone();
        move |key: &str| repo_health.resolve_for_nudge_type(key, &global_health)
    }

    /// Build a HealthConfig with per-repo silence threshold applied.
    fn effective_health_config(
        repo_health: &RepoHealthConfig,
        global_health: &HealthConfig,
    ) -> HealthConfig {
        HealthConfig {
            silence_threshold_seconds: repo_health.resolve_silence_threshold(global_health),
            max_nudges: repo_health.resolve_max_nudges(global_health),
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

        // Load current global state (previous worker states)
        let mut workers_state = WorkersState::load().unwrap_or_default();

        // Discover workers from repo registry (before draining actors so
        // existing_workers is available for the inline first poll)
        let registry = RepoRegistry::load().unwrap_or_default();

        let mut worker_list = discover_workers(&registry);

        // Filter to single repo if configured
        if let Some(ref filter) = self.daemon_config.repo_filter {
            worker_list.retain(|(repo_name, _)| repo_name == filter);
        }

        tracing::debug!(count = worker_list.len(), "discovered workers");

        // 1. Drain all pending actor responses (non-blocking)
        runtime.drain_sync();
        runtime.drain_github();
        let mut spawnable = runtime.drain_issues();

        // First-tick inline poll: run issue poll synchronously so that spawn
        // can happen in the same tick instead of waiting 3 ticks.
        //
        // Repo isolation: `filtered_repos` respects `repo_filter`, so when
        // `jig ps -w` runs within a single repo only that repo is polled.
        // Workers are never spawned for repos outside the filter scope.
        if spawnable.is_empty() && runtime.should_first_poll() {
            runtime.mark_first_poll_done();

            let repos: Vec<(std::path::PathBuf, String)> = registry
                .filtered_repos(self.daemon_config.repo_filter.as_deref())
                .into_iter()
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
                spawnable = issue_actor::process_request(&req);
                if !spawnable.is_empty() {
                    tracing::info!(
                        count = spawnable.len(),
                        "first-tick inline issue poll found spawnable issues"
                    );
                }
            }
        }

        // Drain nudge completions from previous tick
        for nudge_result in runtime.drain_nudges() {
            if let Some(err) = nudge_result.error {
                tracing::warn!(
                    worker = %nudge_result.worker_key,
                    nudge_type = %nudge_result.nudge_type_key,
                    "nudge delivery error: {}",
                    err
                );
            }
        }

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

        // Drain spawn results from previous tick
        if let Some(spawn_complete) = runtime.drain_spawn() {
            for sr in spawn_complete.results {
                if let Some(err) = sr.error {
                    result
                        .errors
                        .push(format!("auto-spawn {}: {}", sr.worker_name, err));
                } else {
                    // Emit WorkStarted notification for successfully spawned workers
                    let event = NotificationEvent::WorkStarted {
                        repo: sr.repo_name.clone(),
                        worker: sr.worker_name.clone(),
                        issue: sr.issue_id.clone(),
                    };
                    if let Err(e) = self.notifier.emit(event) {
                        tracing::warn!(
                            worker = %sr.worker_name,
                            "WorkStarted notification failed: {}", e
                        );
                    }
                    result.auto_spawned.push(sr.worker_name);
                }
            }
        }

        // 2. Trigger background sync if interval elapsed
        if !self.daemon_config.skip_sync {
            runtime.maybe_trigger_sync(&registry, self.daemon_config.repo_filter.as_deref());
        }

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
                Ok((
                    actions,
                    nudges,
                    notifs,
                    worker_tick_info,
                    display_info,
                    prune_targets,
                    nudge_msgs,
                )) => {
                    result.actions_dispatched += actions;
                    result.nudges_sent += nudges;
                    result.notifications_sent += notifs;
                    result.worker_info.insert(key.clone(), worker_tick_info);
                    result.worker_display.push(display_info);
                    live_prune_targets.extend(prune_targets);
                    for (ntype, msg) in nudge_msgs {
                        result
                            .nudge_messages
                            .push((worker_name.clone(), ntype, msg));
                    }
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

        // 4. Trigger issue poll if auto-spawn enabled (scoped to repo_filter)
        runtime.maybe_trigger_issue_poll(
            &registry,
            &worker_list,
            self.daemon_config.repo_filter.as_deref(),
        );

        // 5. Send spawnable issues to background spawn actor (non-blocking)
        if !spawnable.is_empty() {
            runtime.send_spawn(spawnable);
        }

        result.spawning = runtime.spawning_workers().to_vec();
        result.timer_info = Some(runtime.timer_info());

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

        // Auto-spawn: poll repos for spawnable issues (blocking).
        // Each repo's jig.toml controls auto_spawn and max_concurrent_workers.
        // When repo_filter is set, only poll that repo.
        {
            let repos: Vec<(std::path::PathBuf, String)> = registry
                .filtered_repos(self.daemon_config.repo_filter.as_deref())
                .into_iter()
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
                                issue = %issue.issue.id,
                                "auto-spawned worker"
                            );
                            result.auto_spawned.push(issue.worker_name.clone());
                        }
                        Err(e) => {
                            result
                                .errors
                                .push(format!("auto-spawn {}: {}", issue.issue.id, e));
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Process a single worker using cached PR data from the runtime.
    #[allow(clippy::type_complexity)]
    fn process_worker(
        &self,
        repo_name: &str,
        worker_name: &str,
        key: &str,
        workers_state: &mut WorkersState,
        registry: &RepoRegistry,
        runtime: &mut DaemonRuntime,
    ) -> Result<(
        usize,
        usize,
        usize,
        WorkerTickInfo,
        WorkerDisplayInfo,
        Vec<messages::PruneTarget>,
        Vec<(String, String)>,
    )> {
        // Load per-repo health config
        let repo_health = Self::load_repo_health_config(registry, repo_name);
        let effective_health = Self::effective_health_config(&repo_health, &self.config.health);
        let resolve = Self::make_nudge_resolver(&repo_health, &self.config.health);

        let event_log = EventLog::for_worker(repo_name, worker_name)?;
        let events = event_log.read_all()?;
        let new_state = WorkerState::reduce(&events, &effective_health);
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
        let mut new_state = WorkerState::reduce(&events, &effective_health);

        // Created workers (bare worktrees from `jig create`) are discovered for listing
        // but the daemon takes no actions on them.
        if new_state.status == WorkerStatus::Created {
            let display = WorkerDisplayInfo {
                repo: repo_name.to_string(),
                name: worker_name.to_string(),
                branch: branch_name,
                tmux_status: TaskStatus::NoWindow,
                worker_status: Some(new_state.status),
                nudge_count: 0,
                max_nudges: 0,
                commits_ahead: 0,
                is_dirty: false,
                pr_url: None,
                issue_ref: None,
                pr_health: WorkerTickInfo::default(),
                is_draft: false,
                nudge_cooldown_remaining: None,
            };
            return Ok((0, 0, 0, WorkerTickInfo::default(), display, vec![], vec![]));
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

        let mut actions = dispatch_actions(worker_name, &old_state, &new_state, &resolve);

        // Dead tmux detection: if worker is non-terminal but tmux window is gone,
        // resume instead of sending nudges to a dead window.
        // Skip Initializing workers — they're still running on-create hooks.
        if !new_state.status.is_terminal() && new_state.status != WorkerStatus::Initializing {
            let session = format!("{}{}", self.daemon_config.session_prefix, repo_name);
            let target = TmuxTarget::new(&session, worker_name);
            if !self.tmux.has_window(&target) {
                tracing::info!(
                    worker = key,
                    status = new_state.status.as_str(),
                    "active worker has no tmux window, attempting resume"
                );
                // Replace nudge actions with resume attempt
                actions.retain(|a| !matches!(a, Action::Nudge { .. }));
                if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                    match recovery::RecoveryScanner::try_resume_worker(
                        &entry.path,
                        repo_name,
                        worker_name,
                    ) {
                        Ok(true) => {
                            tracing::info!(worker = key, "worker resumed during steady-state tick");
                        }
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(
                                worker = key,
                                error = %e,
                                "failed to resume dead worker"
                            );
                        }
                    }
                }
            }
        }

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
                    kind: NotifyKind::WorkCompleted {
                        pr_url: cached.pr_url.clone(),
                    },
                });
            } else if cached.pr_closed {
                actions.push(Action::Notify {
                    worker_id: worker_name.to_string(),
                    message: "PR closed without merge".to_string(),
                    kind: NotifyKind::NeedsIntervention,
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
                        if let Some(ref pr_url) = cached.pr_url {
                            actions.push(Action::Notify {
                                worker_id: worker_name.to_string(),
                                message: format!(
                                    "New review feedback on PR ({}→{} items)",
                                    previous, current
                                ),
                                kind: NotifyKind::FeedbackReceived {
                                    pr_url: pr_url.clone(),
                                },
                            });
                        }
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
                    let resolved = resolve(nudge_type.count_key());
                    let count = new_state
                        .nudge_counts
                        .get(nudge_type.count_key())
                        .copied()
                        .unwrap_or(0);
                    if count >= resolved.max {
                        tracing::debug!(
                            worker = key,
                            nudge_type = nudge_type.count_key(),
                            count,
                            max = resolved.max,
                            "PR nudge limit reached, skipping"
                        );
                        continue;
                    }
                    // Cooldown: skip if last nudge of this type was too recent
                    if let Some(&last_ts) = new_state.last_nudge_at.get(nudge_type.count_key()) {
                        let now = chrono::Utc::now().timestamp();
                        let elapsed = now - last_ts;
                        if elapsed < resolved.cooldown_seconds as i64 {
                            tracing::debug!(
                                worker = key,
                                nudge_type = nudge_type.count_key(),
                                elapsed,
                                cooldown = resolved.cooldown_seconds,
                                "PR nudge cooldown active, skipping"
                            );
                            continue;
                        }
                    }
                    actions.push(Action::Nudge {
                        worker_id: worker_name.to_string(),
                        nudge_type,
                    });
                }
            }
        }

        // Resolve the repo's base branch for nudge templates
        let repo_base_branch = Self::find_repo_path(registry, repo_name)
            .and_then(|entry| RepoContext::resolve_base_branch_for(&entry.path).ok());

        let action_count = actions.len();
        let (nudge_count, notif_count, cleanup_prune_targets, tick_nudge_messages) = self
            .execute_actions(
                &actions,
                repo_name,
                worker_name,
                &branch_name,
                key,
                &new_state,
                &event_log,
                registry,
                &resolve,
                Some(runtime),
                repo_base_branch.as_deref(),
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

        // Compute minimum remaining cooldown across all active nudge types
        let nudge_cooldown_remaining = {
            let now = chrono::Utc::now().timestamp();
            let mut min_remaining: Option<u64> = None;
            for (nudge_key, &last_ts) in &new_state.last_nudge_at {
                let resolved = resolve(nudge_key);
                let elapsed = now - last_ts;
                if elapsed < resolved.cooldown_seconds as i64 {
                    let remaining = (resolved.cooldown_seconds as i64 - elapsed) as u64;
                    min_remaining =
                        Some(min_remaining.map_or(remaining, |cur: u64| cur.min(remaining)));
                }
            }
            min_remaining
        };

        let display_info = WorkerDisplayInfo {
            repo: repo_name.to_string(),
            name: worker_name.to_string(),
            branch: branch_name.clone(),
            tmux_status,
            worker_status: Some(new_state.status),
            nudge_count: nudges_total,
            max_nudges: effective_health.max_nudges,
            commits_ahead,
            is_dirty,
            pr_url: new_state.pr_url.clone(),
            issue_ref: new_state.issue_ref.clone(),
            pr_health: worker_tick_info.clone(),
            is_draft,
            nudge_cooldown_remaining,
        };

        Ok((
            action_count,
            nudge_count,
            notif_count,
            worker_tick_info,
            display_info,
            cleanup_prune_targets,
            tick_nudge_messages,
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
        // Load per-repo health config
        let repo_health = Self::load_repo_health_config(registry, repo_name);
        let effective_health = Self::effective_health_config(&repo_health, &self.config.health);
        let resolve = Self::make_nudge_resolver(&repo_health, &self.config.health);

        let event_log = EventLog::for_worker(repo_name, worker_name)?;
        let events = event_log.read_all()?;
        let mut new_state = WorkerState::reduce(&events, &effective_health);
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
                                new_state = WorkerState::reduce(&updated_events, &effective_health);
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

        let mut actions = dispatch_actions(worker_name, &old_state, &new_state, &resolve);

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
                        let monitor = PrMonitor::new(&client, self.config, &resolve);
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

        let repo_base_branch = Self::find_repo_path(registry, repo_name)
            .and_then(|entry| RepoContext::resolve_base_branch_for(&entry.path).ok());

        let action_count = actions.len();
        let (nudge_count, notif_count, _prune_targets, _nudge_messages) = self.execute_actions(
            &actions,
            repo_name,
            worker_name,
            &branch_name,
            key,
            &new_state,
            &event_log,
            registry,
            &resolve,
            None,
            repo_base_branch.as_deref(),
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
    ///
    /// When `runtime` is `Some`, nudges are dispatched to the nudge actor
    /// (non-blocking). When `None` (legacy one-shot path), nudges are
    /// delivered synchronously via `execute_nudge`.
    #[allow(clippy::too_many_arguments)]
    fn execute_actions<F>(
        &self,
        actions: &[Action],
        repo_name: &str,
        worker_name: &str,
        branch_name: &str,
        key: &str,
        new_state: &WorkerState,
        event_log: &EventLog,
        registry: &RepoRegistry,
        resolve: &F,
        runtime: Option<&DaemonRuntime>,
        base_branch: Option<&str>,
    ) -> (
        usize,
        usize,
        Vec<messages::PruneTarget>,
        Vec<(String, String)>,
    )
    where
        F: Fn(&str) -> ResolvedNudgeConfig,
    {
        let mut nudge_count = 0;
        let mut notif_count = 0;
        let mut prune_targets = Vec::new();
        let mut nudge_messages: Vec<(String, String)> = Vec::new();

        for action in actions {
            match action {
                Action::Nudge {
                    worker_id: _,
                    nudge_type,
                } => {
                    let session = format!("{}{}", self.daemon_config.session_prefix, repo_name);
                    let target = TmuxTarget::new(&session, branch_name.to_string());

                    if self.tmux.has_window(&target) {
                        // PR nudges (review, ci, conflict, bad commits) should always
                        // be delivered — the agent may be at its idle prompt, which
                        // tmux reports as a shell/version string (pane_is_running=false).
                        // Only skip idle/stuck nudges when the pane has no running command.
                        let is_pr_nudge = matches!(
                            nudge_type,
                            NudgeType::Review
                                | NudgeType::Ci
                                | NudgeType::Conflict
                                | NudgeType::BadCommits
                        );
                        if !is_pr_nudge && !self.tmux.pane_is_running(&target) {
                            tracing::debug!(
                                worker = key,
                                "no command running in pane, skipping nudge"
                            );
                            continue;
                        }
                        let resolved = resolve(nudge_type.count_key());

                        // Render template on the tick thread (TemplateEngine has lifetime)
                        let ctx =
                            build_nudge_context(*nudge_type, new_state, resolved, base_branch);
                        let message = match self.engine.render(nudge_type.template_name(), &ctx) {
                            Ok(msg) => msg,
                            Err(e) => {
                                tracing::warn!("nudge template render failed for {}: {}", key, e);
                                continue;
                            }
                        };

                        if let Some(rt) = runtime {
                            // Async path: dispatch to nudge actor
                            nudge_messages
                                .push((nudge_type.count_key().to_string(), message.clone()));
                            rt.send_nudge(messages::NudgeRequest {
                                session,
                                window: branch_name.to_string(),
                                message,
                                nudge_type_key: nudge_type.count_key().to_string(),
                                is_stuck: *nudge_type == NudgeType::Stuck,
                                repo_name: repo_name.to_string(),
                                worker_name: worker_name.to_string(),
                                worker_key: key.to_string(),
                            });
                            nudge_count += 1;
                        } else {
                            // Blocking path (tick_once): deliver synchronously
                            let delivery = if *nudge_type == NudgeType::Stuck {
                                self.tmux.auto_approve(&target).and_then(|()| {
                                    std::thread::sleep(std::time::Duration::from_millis(500));
                                    self.tmux.send_message(&target, &message)
                                })
                            } else {
                                self.tmux.send_message(&target, &message)
                            };

                            match delivery {
                                Ok(()) => {
                                    let event = Event::new(EventType::Nudge)
                                        .with_field("nudge_type", nudge_type.count_key())
                                        .with_field("message", message.as_str());
                                    if let Err(e) = event_log.append(&event) {
                                        tracing::warn!(
                                            "failed to append nudge event for {}: {}",
                                            key,
                                            e
                                        );
                                    }
                                    tracing::info!(
                                        worker = key,
                                        nudge_type = nudge_type.count_key(),
                                        "nudge delivered"
                                    );
                                    nudge_count += 1;
                                }
                                Err(e) => {
                                    tracing::warn!("nudge failed for {}: {}", key, e);
                                }
                            }
                        }
                    } else {
                        tracing::debug!(
                            worker = key,
                            nudge_type = nudge_type.count_key(),
                            session = %format!("{}{}", self.daemon_config.session_prefix, repo_name),
                            window = %branch_name,
                            "tmux window not found, skipping nudge"
                        );
                    }
                }
                Action::Notify {
                    worker_id,
                    message,
                    kind,
                } => {
                    tracing::info!(worker = key, message = %message, "notification sent");
                    let event = match kind {
                        NotifyKind::NeedsIntervention => NotificationEvent::NeedsIntervention {
                            repo: repo_name.to_string(),
                            worker: worker_id.clone(),
                            reason: message.clone(),
                        },
                        NotifyKind::PrOpened { pr_url } => NotificationEvent::PrOpened {
                            repo: repo_name.to_string(),
                            worker: worker_id.clone(),
                            pr_url: pr_url.clone(),
                        },
                        NotifyKind::WorkCompleted { pr_url } => NotificationEvent::WorkCompleted {
                            repo: repo_name.to_string(),
                            worker: worker_id.clone(),
                            pr_url: pr_url.clone(),
                        },
                        NotifyKind::FeedbackReceived { pr_url } => {
                            NotificationEvent::FeedbackReceived {
                                repo: repo_name.to_string(),
                                worker: worker_id.clone(),
                                pr_url: pr_url.clone(),
                            }
                        }
                    };
                    if let Err(e) = self.notifier.emit(event) {
                        tracing::warn!("notification failed for {}: {}", worker_id, e);
                    }
                    notif_count += 1;
                }
                Action::Restart { worker_id, reason } => {
                    tracing::info!(
                        worker = %worker_id,
                        reason = %reason,
                        "restart requested, attempting resume"
                    );
                    if let Some(entry) = Self::find_repo_path(registry, repo_name) {
                        match recovery::RecoveryScanner::try_resume_worker(
                            &entry.path,
                            repo_name,
                            worker_name,
                        ) {
                            Ok(true) => {
                                tracing::info!(worker = key, "worker resumed via restart action");
                            }
                            Ok(false) => {
                                tracing::debug!(
                                    worker = key,
                                    "worker still has tmux window, skip resume"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    worker = key,
                                    error = %e,
                                    "failed to resume worker via restart action"
                                );
                            }
                        }
                    }
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

        (nudge_count, notif_count, prune_targets, nudge_messages)
    }

    /// Auto-spawn a worker for an issue.
    ///
    /// Delegates to [`crate::spawn::spawn_worker_for_issue`] for the core spawn
    /// sequence, then emits the WorkStarted notification.
    fn auto_spawn_worker(&self, issue: &SpawnableIssue) -> Result<()> {
        use crate::spawn::{self, SpawnIssueInput};

        let input = SpawnIssueInput {
            repo_root: &issue.repo_root,
            issue: &issue.issue,
            worker_name: &issue.worker_name,
            provider_kind: issue.provider_kind,
        };
        spawn::spawn_worker_for_issue(&input).map_err(crate::error::Error::Custom)?;

        // Emit WorkStarted notification
        let repo_name = issue
            .repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let event = NotificationEvent::WorkStarted {
            repo: repo_name,
            worker: issue.worker_name.clone(),
            issue: Some(issue.issue.id.clone()),
        };
        if let Err(e) = self.notifier.emit(event) {
            tracing::warn!(worker = %issue.worker_name, "WorkStarted notification failed: {}", e);
        }

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

/// Install SIGINT/SIGTERM handler that sets the quit flag for graceful shutdown.
fn install_signal_handler(quit: &Arc<AtomicBool>) {
    let quit_flag = Arc::clone(quit);
    if let Err(e) = ctrlc::set_handler(move || {
        tracing::info!("received shutdown signal, finishing current tick...");
        quit_flag.store(true, Ordering::Relaxed);
    }) {
        tracing::warn!("failed to install signal handler: {}", e);
    }
}

/// Run startup recovery: log lifecycle event, detect crash, resume orphans.
fn startup_recovery(global_config: &GlobalConfig) {
    let log = match lifecycle::DaemonLifecycleLog::global() {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("failed to open daemon lifecycle log: {}", e);
            return;
        }
    };

    // Check for previous crash
    match log.previous_run_crashed() {
        Ok(true) => {
            tracing::warn!(
                "previous daemon run did not shut down cleanly — checking for orphaned workers"
            );
        }
        Ok(false) => {}
        Err(e) => {
            tracing::warn!("failed to check daemon lifecycle log: {}", e);
        }
    }

    // Log startup
    if let Err(e) = log.record_started() {
        tracing::warn!("failed to write daemon Started event: {}", e);
    }

    // Auto-recover orphaned workers if enabled
    if global_config.daemon.auto_recover {
        let registry = RepoRegistry::load().unwrap_or_default();
        let scanner = recovery::RecoveryScanner::new(&registry, &global_config.health);
        let recovered = scanner.recover_all();
        if !recovered.is_empty() {
            tracing::info!(
                count = recovered.len(),
                "recovered orphaned workers on startup"
            );
            for (repo, worker) in &recovered {
                tracing::info!(repo = %repo, worker = %worker, "recovered");
            }
        }
    }
}

/// Log a graceful shutdown event.
fn log_shutdown(reason: &str) {
    let log = match lifecycle::DaemonLifecycleLog::global() {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("failed to open daemon lifecycle log: {}", e);
            return;
        }
    };
    if let Err(e) = log.record_stopped(reason) {
        tracing::warn!("failed to write daemon Stopped event: {}", e);
    }
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

    // Startup: lifecycle logging + recovery
    startup_recovery(&global_config);

    let tmux = TmuxClient::new();
    let engine = TemplateEngine::new();
    let notifier = make_notifier(&global_config)?;
    let daemon = Daemon::new(&global_config, &tmux, &engine, &notifier, daemon_config);

    let mut runtime = DaemonRuntime::new(runtime_config);
    let quit = Arc::new(AtomicBool::new(false));

    // Install signal handler for graceful shutdown
    install_signal_handler(&quit);

    let result = (|| -> Result<Arc<AtomicBool>> {
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
                        return Ok(quit.clone());
                    }
                    let keep_going = on_tick(&tick, &quit);
                    if daemon_config.once || !keep_going {
                        return Ok(quit.clone());
                    }
                }
                Err(e) => {
                    tracing::error!("tick failed: {}", e);
                    if daemon_config.once {
                        return Err(e);
                    }
                    if quit.load(Ordering::Relaxed) {
                        return Ok(quit.clone());
                    }
                    std::thread::sleep(Duration::from_secs(daemon_config.interval_seconds));
                }
            }
        }
    })();

    // Log shutdown with appropriate reason
    log_shutdown(if result.is_ok() { "normal" } else { "error" });
    result
}

/// Run the daemon loop (simple blocking mode). Returns after one pass if `config.once` is true.
pub fn run(daemon_config: &DaemonConfig) -> Result<()> {
    let global_config = GlobalConfig::load()?;

    // Startup: lifecycle logging + recovery
    startup_recovery(&global_config);

    let tmux = TmuxClient::new();
    let engine = TemplateEngine::new();
    let notifier = make_notifier(&global_config)?;
    let daemon = Daemon::new(&global_config, &tmux, &engine, &notifier, daemon_config);

    let quit = Arc::new(AtomicBool::new(false));
    install_signal_handler(&quit);

    let result = (|| -> Result<()> {
        loop {
            if quit.load(Ordering::Relaxed) {
                return Ok(());
            }
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
    })();

    log_shutdown(if result.is_ok() { "normal" } else { "error" });
    result
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
        last_nudge_at: HashMap::new(),
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
        assert_eq!(config.max_concurrent_workers, 3);
        assert_eq!(config.auto_spawn_interval, 120);
        assert_eq!(config.sync_interval, 60);
    }

    /// Helper: mirrors the review nudge reset logic from process_worker.
    fn maybe_reset_review_nudges(
        nudge_counts: &mut HashMap<String, u32>,
        stored: Option<u32>,
        current: u32,
    ) {
        let previous = stored.unwrap_or(0);
        if current > previous {
            nudge_counts.remove("review");
        }
    }

    #[test]
    fn review_nudge_count_resets_on_new_feedback() {
        let mut nudge_counts: HashMap<String, u32> = HashMap::new();
        nudge_counts.insert("review".to_string(), 3); // exhausted

        maybe_reset_review_nudges(&mut nudge_counts, Some(2), 5);

        assert_eq!(nudge_counts.get("review"), None);
    }

    #[test]
    fn review_nudge_count_unchanged_when_no_new_feedback() {
        let mut nudge_counts: HashMap<String, u32> = HashMap::new();
        nudge_counts.insert("review".to_string(), 2);

        maybe_reset_review_nudges(&mut nudge_counts, Some(3), 3);

        assert_eq!(nudge_counts.get("review"), Some(&2));
    }

    #[test]
    fn review_nudge_count_resets_from_none_stored() {
        let mut nudge_counts: HashMap<String, u32> = HashMap::new();
        nudge_counts.insert("review".to_string(), 1);

        maybe_reset_review_nudges(&mut nudge_counts, None, 2);

        assert_eq!(nudge_counts.get("review"), None);
    }
}
