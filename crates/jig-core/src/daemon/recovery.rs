//! Startup recovery — detect and resume orphaned workers after a daemon crash.
//!
//! Uses `Worktree::list()` + event log replay to find workers that were active
//! when the daemon died, then calls `Worktree::resume()` to relaunch them.

use std::path::Path;

use crate::config::JIG_DIR;
use crate::error::Result;
use crate::events::{EventLog, WorkerState};
use crate::global::{GlobalConfig, HealthConfig};
use crate::registry::RepoRegistry;
use crate::worker::WorkerStatus;
use crate::worktree::Worktree;

/// Information about an orphaned worker found during recovery scan.
#[derive(Debug)]
pub struct OrphanedWorker {
    pub repo_name: String,
    pub worker_name: String,
    pub status: WorkerStatus,
    pub worktree: Worktree,
}

/// Find orphaned workers across all registered repos.
///
/// An orphaned worker is one whose:
/// - Worktree exists on disk
/// - Event log shows a non-terminal, active state (Spawned/Running/Stalled)
/// - Tmux window is gone (no live agent process)
pub fn find_orphaned_workers(registry: &RepoRegistry) -> Vec<OrphanedWorker> {
    let health = GlobalConfig::load().map(|c| c.health).unwrap_or_default();
    let mut orphans = Vec::new();

    for entry in registry.repos() {
        if !entry.path.exists() {
            continue;
        }
        let repo_name = match entry.path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };

        let worktrees_dir = entry.path.join(JIG_DIR);
        let worktrees = match Worktree::list(&entry.path, &worktrees_dir) {
            Ok(wts) => wts,
            Err(_) => continue,
        };

        for wt in worktrees {
            if let Some(orphan) = check_worker_orphaned(&repo_name, &wt, &health) {
                orphans.push(orphan);
            }
        }
    }

    orphans
}

/// Check if a single worker is orphaned and eligible for recovery.
fn check_worker_orphaned(
    repo_name: &str,
    wt: &Worktree,
    health: &HealthConfig,
) -> Option<OrphanedWorker> {
    // Must have no tmux window
    if wt.has_tmux_window() {
        return None;
    }

    // Must have event log with non-terminal state
    let event_log = EventLog::for_worker(repo_name, &wt.name).ok()?;
    let events = event_log.read_all().ok()?;
    if events.is_empty() {
        return None;
    }

    let state = WorkerState::reduce(&events, health);

    // Only recover active workers — skip terminal and "done" states
    if should_recover(state.status) {
        Some(OrphanedWorker {
            repo_name: repo_name.to_string(),
            worker_name: wt.name.clone(),
            status: state.status,
            worktree: wt.clone(),
        })
    } else {
        None
    }
}

/// Whether a worker in this status should be auto-recovered.
fn should_recover(status: WorkerStatus) -> bool {
    matches!(
        status,
        WorkerStatus::Spawned
            | WorkerStatus::Running
            | WorkerStatus::Stalled
            | WorkerStatus::Initializing
    )
}

/// Recover all orphaned workers by resuming them.
///
/// Returns a list of (repo_name, worker_name) pairs that were successfully resumed.
pub fn recover_orphaned_workers(registry: &RepoRegistry) -> Vec<(String, String)> {
    let orphans = find_orphaned_workers(registry);
    let mut recovered = Vec::new();

    for orphan in orphans {
        tracing::info!(
            repo = %orphan.repo_name,
            worker = %orphan.worker_name,
            status = %orphan.status.as_str(),
            "recovering orphaned worker"
        );

        // Read original context from spawn event
        let context = read_spawn_context(&orphan.repo_name, &orphan.worker_name);

        match orphan.worktree.resume(context.as_deref()) {
            Ok(()) => {
                tracing::info!(
                    repo = %orphan.repo_name,
                    worker = %orphan.worker_name,
                    "worker recovered successfully"
                );
                recovered.push((orphan.repo_name, orphan.worker_name));
            }
            Err(e) => {
                tracing::warn!(
                    repo = %orphan.repo_name,
                    worker = %orphan.worker_name,
                    error = %e,
                    "failed to recover worker"
                );
            }
        }
    }

    recovered
}

/// Read the original spawn context from the worker's event log.
fn read_spawn_context(repo_name: &str, worker_name: &str) -> Option<String> {
    let event_log = EventLog::for_worker(repo_name, worker_name).ok()?;
    let events = event_log.read_all().ok()?;
    events
        .iter()
        .find(|e| e.event_type == crate::events::EventType::Spawn)
        .and_then(|e| e.data.get("context").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

/// Check if a specific worker (by repo path and name) is orphaned and resume it.
///
/// Used by the daemon during steady-state ticks when it detects a dead tmux window.
pub fn try_resume_worker(repo_root: &Path, repo_name: &str, worker_name: &str) -> Result<bool> {
    let worktrees_dir = repo_root.join(JIG_DIR);
    let wt = Worktree::open(repo_root, &worktrees_dir, worker_name)?;

    if wt.has_tmux_window() {
        return Ok(false);
    }

    let context = read_spawn_context(repo_name, worker_name);
    wt.resume(context.as_deref())?;

    tracing::info!(
        repo = repo_name,
        worker = worker_name,
        "resumed dead worker during steady-state tick"
    );

    Ok(true)
}
