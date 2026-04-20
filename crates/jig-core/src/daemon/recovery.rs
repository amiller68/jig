//! Startup recovery — detect and resume orphaned workers after a daemon crash.
//!
//! Uses `Worker::list()` + event log replay to find workers that were active
//! when the daemon died, then calls `Worker::resume()` to relaunch them.

use std::path::Path;

use crate::config::JIG_DIR;
use crate::error::Result;
use crate::events::{EventLog, EventType, WorkerState};
use crate::global::HealthConfig;
use crate::registry::RepoRegistry;
use crate::worker::{Worker, WorkerStatus};

/// Information about an orphaned worker found during recovery scan.
pub struct OrphanedWorker {
    pub repo_name: String,
    pub worker_name: String,
    pub status: WorkerStatus,
    pub worker: Worker,
}

/// Scans for and recovers orphaned workers across registered repos.
///
/// An orphaned worker is one whose:
/// - Worktree exists on disk
/// - Event log shows a non-terminal, active state (Spawned/Running/Stalled/Initializing)
/// - Tmux window is gone (no live agent process)
pub struct RecoveryScanner<'a> {
    registry: &'a RepoRegistry,
    health: HealthConfig,
}

impl<'a> RecoveryScanner<'a> {
    /// Create a new scanner for the given registry and health config.
    pub fn new(registry: &'a RepoRegistry, health: &HealthConfig) -> Self {
        Self {
            registry,
            health: health.clone(),
        }
    }

    /// Find all orphaned workers across registered repos.
    pub fn find_orphaned(&self) -> Vec<OrphanedWorker> {
        let mut orphans = Vec::new();

        for entry in self.registry.repos() {
            if !entry.path.exists() {
                continue;
            }
            let repo_name = match entry.path.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };

            let worktrees_path = entry.path.join(JIG_DIR);
            let workers = match Worker::list(&entry.path, &worktrees_path) {
                Ok(ws) => ws,
                Err(_) => continue,
            };

            for w in workers {
                if let Some(orphan) = self.check_worker_orphaned(&repo_name, &w) {
                    orphans.push(orphan);
                }
            }
        }

        orphans
    }

    /// Recover all orphaned workers by resuming them.
    ///
    /// Returns a list of (repo_name, worker_name) pairs that were successfully resumed.
    pub fn recover_all(&self) -> Vec<(String, String)> {
        let orphans = self.find_orphaned();
        let mut recovered = Vec::new();

        for orphan in orphans {
            tracing::info!(
                repo = %orphan.repo_name,
                worker = %orphan.worker_name,
                status = %orphan.status.as_str(),
                "recovering orphaned worker"
            );

            let context = Self::read_spawn_context(&orphan.repo_name, &orphan.worker_name);

            match orphan.worker.resume(context.as_deref()) {
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

    /// Read the original spawn context from a worker's event log.
    pub fn read_spawn_context(repo_name: &str, worker_name: &str) -> Option<String> {
        let event_log = EventLog::for_worker(repo_name, worker_name).ok()?;
        let events = event_log.read_all().ok()?;
        events
            .iter()
            .find(|e| e.event_type == EventType::Spawn)
            .and_then(|e| e.data.get("context").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
    }

    /// Check if a specific worker (by repo path and name) is orphaned and resume it.
    ///
    /// Used by the daemon during steady-state ticks when it detects a dead tmux window.
    pub fn try_resume_worker(repo_root: &Path, repo_name: &str, worker_name: &str) -> Result<bool> {
        let worktrees_path = repo_root.join(JIG_DIR);
        let w = Worker::open(repo_root, &worktrees_path, worker_name)?;

        if w.has_tmux_window() {
            return Ok(false);
        }

        let context = Self::read_spawn_context(repo_name, worker_name);
        w.resume(context.as_deref())?;

        tracing::info!(
            repo = repo_name,
            worker = worker_name,
            "resumed dead worker during steady-state tick"
        );

        Ok(true)
    }

    /// Check if a single worker is orphaned and eligible for recovery.
    fn check_worker_orphaned(&self, repo_name: &str, w: &Worker) -> Option<OrphanedWorker> {
        if w.has_tmux_window() {
            return None;
        }

        let event_log = EventLog::for_worker(repo_name, w.name()).ok()?;
        let events = event_log.read_all().ok()?;
        if events.is_empty() {
            return None;
        }

        let state = WorkerState::reduce(&events, &self.health);

        if Self::should_recover(state.status) {
            Some(OrphanedWorker {
                repo_name: repo_name.to_string(),
                worker_name: w.name().to_string(),
                status: state.status,
                worker: w.clone(),
            })
        } else {
            None
        }
    }

    /// Whether a worker in this status should be auto-recovered.
    fn should_recover(status: WorkerStatus) -> bool {
        !status.is_terminal()
            && status != WorkerStatus::Initializing
            && status != WorkerStatus::Created
    }
}
