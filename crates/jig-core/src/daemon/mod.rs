//! Daemon loop — the conductor that ties event derivation, dispatch, and execution together.
//!
//! Runs a periodic loop:
//! 1. Load repo registry → discover all workers
//! 2. For each worker: read events → derive state → compare → dispatch actions
//! 3. Execute actions (nudge via tmux, notify via hooks)
//! 4. Save updated state
//! 5. Sleep and repeat

mod discovery;
mod pr;

use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

use crate::context::RepoContext;
use crate::dispatch::{dispatch_actions, Action};
use crate::error::Result;
use crate::events::{Event, EventLog, EventType, WorkerState};
use crate::global::{GlobalConfig, WorkerEntry, WorkersState};
use crate::notify::{NotificationEvent, Notifier};
use crate::nudge::execute_nudge;
use crate::registry::RepoRegistry;
use crate::templates::TemplateEngine;
use crate::tmux::{TmuxClient, TmuxTarget};

use discovery::discover_workers;
use pr::{make_github_client, PrMonitor};

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
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            once: false,
            session_prefix: "jig-".to_string(),
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

    /// Execute a single tick of the daemon: check all workers and dispatch actions.
    pub fn tick(&self) -> Result<TickResult> {
        let mut result = TickResult::default();

        // Load current global state (previous worker states)
        let mut workers_state = WorkersState::load().unwrap_or_default();

        // Discover workers from repo registry
        let registry = RepoRegistry::load().unwrap_or_default();
        self.sync_repos(&registry);
        let worker_list = discover_workers(&registry);

        tracing::debug!(count = worker_list.len(), "discovered workers");

        for (repo_name, worker_name) in &worker_list {
            result.workers_checked += 1;
            let key = format!("{}/{}", repo_name, worker_name);

            match self.process_worker(repo_name, worker_name, &key, &mut workers_state, &registry) {
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

        // Save updated state
        workers_state.save().unwrap_or_else(|e| {
            tracing::warn!("failed to save workers state: {}", e);
        });

        Ok(result)
    }

    /// Process a single worker: derive state, dispatch, execute.
    fn process_worker(
        &self,
        repo_name: &str,
        worker_name: &str,
        key: &str,
        workers_state: &mut WorkersState,
        registry: &RepoRegistry,
    ) -> Result<(usize, usize, usize, WorkerTickInfo)> {
        // Read event log
        let event_log = EventLog::for_worker(repo_name, worker_name)?;
        let events = event_log.read_all()?;

        // Derive new state
        let mut new_state = WorkerState::reduce(&events, &self.config.health);

        // Extract the real branch name (with slashes) from the spawn event
        let branch_name = events
            .iter()
            .find(|e| e.event_type == EventType::Spawn)
            .and_then(|e| e.data.get("branch").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .unwrap_or_else(|| worker_name.to_string());

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
                            // Re-derive state with the new event included
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

        // Get old state from workers.json
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

        // Dispatch actions based on state transition
        let mut actions = dispatch_actions(worker_name, &old_state, &new_state, self.config);

        // Check PR lifecycle if worker has a PR URL and isn't already terminal
        let mut worker_tick_info = WorkerTickInfo::default();
        if !new_state.status.is_terminal() {
            if let Some(pr_url) = &new_state.pr_url {
                worker_tick_info.has_pr = true;
                match make_github_client(repo_name, registry) {
                    Some(client) => {
                        let monitor = PrMonitor::new(&client, self.config);
                        let pr_result = monitor.check_lifecycle(
                            worker_name,
                            &branch_name,
                            pr_url,
                            &new_state,
                            &mut actions,
                        );
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
        let mut nudge_count = 0;
        let mut notif_count = 0;

        // Execute actions
        for action in &actions {
            match action {
                Action::Nudge {
                    worker_id: _,
                    nudge_type,
                } => {
                    // Use branch_name (with slashes) for tmux window lookup,
                    // since spawn creates windows with the real branch name
                    let target = TmuxTarget::new(
                        format!("{}{}", self.daemon_config.session_prefix, repo_name),
                        branch_name.clone(),
                    );

                    if self.tmux.has_window(&target) {
                        let is_agent = self
                            .tmux
                            .pane_command(&target)
                            .map(|cmd| cmd == "claude" || cmd == "node")
                            .unwrap_or(false);
                        if !is_agent {
                            tracing::debug!(
                                worker = key,
                                "agent not running in pane, skipping nudge"
                            );
                            continue;
                        }
                        if let Err(e) = execute_nudge(
                            &target,
                            *nudge_type,
                            &new_state,
                            self.config,
                            self.engine,
                            self.tmux,
                            &event_log,
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
                    let target = TmuxTarget::new(
                        format!("{}{}", self.daemon_config.session_prefix, repo_name),
                        branch_name.clone(),
                    );

                    // Kill tmux window if it exists
                    if self.tmux.has_window(&target) {
                        if let Err(e) = self.tmux.kill_window(&target) {
                            tracing::warn!("failed to kill window for {}: {}", worker_id, e);
                        }
                    }

                    // Emit terminal event
                    let event = Event::new(EventType::Terminal).with_field("reason", "cleanup");
                    if let Err(e) = event_log.append(&event) {
                        tracing::warn!("failed to emit cleanup event for {}: {}", key, e);
                    }

                    tracing::info!("cleaned up worker {}", worker_id);
                }
            }
        }

        // Update workers.json with new state
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
            },
        );

        Ok((action_count, nudge_count, notif_count, worker_tick_info))
    }

    /// Fetch the configured base branch for each registered repo.
    fn sync_repos(&self, registry: &RepoRegistry) {
        for entry in registry.repos() {
            if !entry.path.exists() {
                continue;
            }
            let base = RepoContext::resolve_base_branch_for(&entry.path)
                .unwrap_or_else(|_| "origin/main".to_string());
            let (remote, branch) = base.split_once('/').unwrap_or(("origin", &base));

            match Command::new("git")
                .args(["fetch", remote, branch])
                .current_dir(&entry.path)
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

/// Run the daemon loop. Returns after one pass if `config.once` is true.
pub fn run(daemon_config: &DaemonConfig) -> Result<()> {
    let global_config = GlobalConfig::load()?;
    let tmux = TmuxClient::new();
    let engine = TemplateEngine::new();
    let notifier = make_notifier(&global_config)?;

    let daemon = Daemon::new(&global_config, &tmux, &engine, &notifier, daemon_config);

    loop {
        let result = daemon.tick();

        match &result {
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
                    // Always print summary to stderr so it's visible without RUST_LOG
                    if !daemon_config.once {
                        eprintln!(
                            "[tick] {} workers, {} actions, {} nudges, {} notifications, {} errors",
                            tick.workers_checked,
                            tick.actions_dispatched,
                            tick.nudges_sent,
                            tick.notifications_sent,
                            tick.errors.len(),
                        );
                    }
                }
                for err in &tick.errors {
                    tracing::warn!("worker error: {}", err);
                }
            }
            Err(e) => {
                tracing::error!("tick failed: {}", e);
                eprintln!("[tick] error: {}", e);
            }
        }

        if daemon_config.once {
            return result.map(|_| ());
        }

        std::thread::sleep(Duration::from_secs(daemon_config.interval_seconds));
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
}
