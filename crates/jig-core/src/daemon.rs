//! Daemon loop — the conductor that ties event derivation, dispatch, and execution together.
//!
//! Runs a periodic loop:
//! 1. Load repo registry → discover all workers
//! 2. For each worker: read events → derive state → compare → dispatch actions
//! 3. Execute actions (nudge via tmux, notify via hooks)
//! 4. Save updated state
//! 5. Sleep and repeat

use std::time::Duration;

use crate::dispatch::{dispatch_actions, Action};
use crate::error::Result;
use crate::events::{EventLog, WorkerState};
use crate::global::{GlobalConfig, WorkerEntry, WorkersState};
use crate::notify::{NotificationEvent, Notifier};
use crate::nudge::execute_nudge;
use crate::registry::RepoRegistry;
use crate::templates::TemplateEngine;
use crate::tmux::{TmuxClient, TmuxTarget};

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

    loop {
        let result = tick(&global_config, &tmux, &engine, &notifier, daemon_config);

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
                }
                for err in &tick.errors {
                    tracing::warn!("worker error: {}", err);
                }
            }
            Err(e) => {
                tracing::error!("tick failed: {}", e);
            }
        }

        if daemon_config.once {
            return result.map(|_| ());
        }

        std::thread::sleep(Duration::from_secs(daemon_config.interval_seconds));
    }
}

/// Execute a single tick of the daemon: check all workers and dispatch actions.
pub fn tick(
    global_config: &GlobalConfig,
    tmux: &TmuxClient,
    engine: &TemplateEngine<'_>,
    notifier: &Notifier,
    daemon_config: &DaemonConfig,
) -> Result<TickResult> {
    let mut result = TickResult::default();

    // Load current global state (previous worker states)
    let mut workers_state = WorkersState::load().unwrap_or_default();

    // Discover workers from repo registry
    let registry = RepoRegistry::load().unwrap_or_default();
    let worker_list = discover_workers(&registry);

    for (repo_name, worker_name) in &worker_list {
        result.workers_checked += 1;
        let key = format!("{}/{}", repo_name, worker_name);

        match process_worker(
            repo_name,
            worker_name,
            &key,
            &mut workers_state,
            global_config,
            tmux,
            engine,
            notifier,
            daemon_config,
        ) {
            Ok((actions, nudges, notifs)) => {
                result.actions_dispatched += actions;
                result.nudges_sent += nudges;
                result.notifications_sent += notifs;
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
#[allow(clippy::too_many_arguments)]
fn process_worker(
    repo_name: &str,
    worker_name: &str,
    key: &str,
    workers_state: &mut WorkersState,
    global_config: &GlobalConfig,
    tmux: &TmuxClient,
    engine: &TemplateEngine<'_>,
    notifier: &Notifier,
    daemon_config: &DaemonConfig,
) -> Result<(usize, usize, usize)> {
    // Read event log
    let event_log = EventLog::for_worker(repo_name, worker_name)?;
    let events = event_log.read_all()?;

    // Derive new state
    let new_state = WorkerState::reduce(&events, &global_config.health);

    // Get old state from workers.json
    let old_state = workers_state
        .get_worker(key)
        .map(entry_to_worker_state)
        .unwrap_or_default();

    // Dispatch actions based on state transition
    let actions = dispatch_actions(worker_name, &old_state, &new_state, global_config);
    let action_count = actions.len();
    let mut nudge_count = 0;
    let mut notif_count = 0;

    // Execute actions
    for action in &actions {
        match action {
            Action::Nudge {
                worker_id,
                nudge_type,
            } => {
                let target = TmuxTarget::new(
                    format!("{}{}", daemon_config.session_prefix, repo_name),
                    worker_id.to_string(),
                );

                if tmux.has_window(&target) {
                    if let Err(e) = execute_nudge(
                        &target,
                        *nudge_type,
                        &new_state,
                        global_config,
                        engine,
                        tmux,
                        &event_log,
                    ) {
                        tracing::warn!("nudge failed for {}: {}", key, e);
                    } else {
                        nudge_count += 1;
                    }
                }
            }
            Action::Notify { worker_id, message } => {
                let event = NotificationEvent::NeedsIntervention {
                    repo: repo_name.to_string(),
                    worker: worker_id.clone(),
                    reason: message.clone(),
                };
                if let Err(e) = notifier.emit(event) {
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
                tracing::info!("cleanup requested for {} (not yet implemented)", worker_id);
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
            issue: None,
            pr_url: new_state.pr_url.clone(),
            started_at: new_state.started_at.unwrap_or(0),
            last_event_at: new_state.last_event_at.unwrap_or(0),
            nudge_counts: new_state.nudge_counts.clone(),
        },
    );

    Ok((action_count, nudge_count, notif_count))
}

/// Discover all workers by scanning the events directory.
fn discover_workers(registry: &RepoRegistry) -> Vec<(String, String)> {
    let mut workers = vec![];

    // Scan the events directory for worker event logs
    let events_dir = match crate::global::global_state_dir() {
        Ok(dir) => dir.join("events"),
        Err(_) => return workers,
    };

    if !events_dir.is_dir() {
        return workers;
    }

    // Each subdirectory is named "<repo>-<worker>" and contains events.jsonl
    let entries = match std::fs::read_dir(&events_dir) {
        Ok(entries) => entries,
        Err(_) => return workers,
    };

    // Build a set of known repo names from registry for matching
    let repo_names: Vec<String> = registry
        .repos()
        .iter()
        .filter_map(|entry| {
            entry
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .collect();

    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let events_file = entry.path().join("events.jsonl");

        if !events_file.exists() {
            continue;
        }

        // Try to split "repo-worker" — match longest registered repo name prefix
        if let Some((repo, worker)) = split_worker_dir(&dir_name, &repo_names) {
            workers.push((repo, worker));
        }
    }

    workers
}

/// Split a directory name like "myrepo-feat-branch" into (repo, worker).
/// Uses known repo names to find the correct split point.
fn split_worker_dir(dir_name: &str, repo_names: &[String]) -> Option<(String, String)> {
    // Try each known repo name as a prefix
    for repo_name in repo_names {
        let prefix = format!("{}-", repo_name);
        if let Some(worker) = dir_name.strip_prefix(&prefix) {
            if !worker.is_empty() {
                return Some((repo_name.clone(), worker.to_string()));
            }
        }
    }

    // Fallback: split on first dash
    let dash = dir_name.find('-')?;
    let repo = &dir_name[..dash];
    let worker = &dir_name[dash + 1..];
    if worker.is_empty() {
        return None;
    }
    Some((repo.to_string(), worker.to_string()))
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
        started_at: Some(entry.started_at),
        last_event_at: Some(entry.last_event_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn split_worker_dir_with_known_repo() {
        let repos = vec!["myrepo".to_string(), "jig".to_string()];
        let result = split_worker_dir("myrepo-feat-branch", &repos);
        assert_eq!(
            result,
            Some(("myrepo".to_string(), "feat-branch".to_string()))
        );
    }

    #[test]
    fn split_worker_dir_fallback() {
        let repos: Vec<String> = vec![];
        let result = split_worker_dir("myrepo-feat", &repos);
        assert_eq!(result, Some(("myrepo".to_string(), "feat".to_string())));
    }

    #[test]
    fn split_worker_dir_no_dash() {
        let repos: Vec<String> = vec![];
        let result = split_worker_dir("nodash", &repos);
        assert_eq!(result, None);
    }

    #[test]
    fn split_worker_dir_prefers_known_repo() {
        let repos = vec!["my-repo".to_string()];
        let result = split_worker_dir("my-repo-feat", &repos);
        assert_eq!(result, Some(("my-repo".to_string(), "feat".to_string())));
    }

    #[test]
    fn entry_to_state_roundtrip() {
        let entry = WorkerEntry {
            repo: "test".to_string(),
            branch: "main".to_string(),
            status: "running".to_string(),
            issue: None,
            pr_url: Some("https://github.com/pr/1".to_string()),
            started_at: 1000,
            last_event_at: 2000,
            nudge_counts: HashMap::new(),
        };
        let state = entry_to_worker_state(&entry);
        assert_eq!(state.status, crate::worker::WorkerStatus::Running);
        assert_eq!(state.pr_url.as_deref(), Some("https://github.com/pr/1"));
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
