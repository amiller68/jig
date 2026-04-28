//! Startup recovery — detect and resume orphaned workers after a daemon crash.
//!
//! Uses worktree listing + event log replay to find workers that were active
//! when the daemon died, then calls `Worker::resume()` to relaunch them.

use std::path::Path;

use jig_core::agents;
use jig_core::config::registry::RepoRegistry;
use jig_core::config::HealthConfig;
use jig_core::config::{self, JIG_DIR};
use jig_core::error::Result;
use jig_core::git::{Repo, Worktree};
use jig_core::prompt::Prompt;
use jig_core::worker::events::{EventLog, WorkerState};
use jig_core::worker::{Worker, WorkerStatus};

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

            let repo = match Repo::open(&entry.path) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let worktrees = match repo.list_worktrees() {
                Ok(wts) => wts,
                Err(_) => continue,
            };

            for wt in worktrees {
                let w = Worker::from(&wt);
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

            let wt = match orphan.worker.worktree() {
                Ok(wt) => wt,
                Err(e) => {
                    tracing::warn!(
                        repo = %orphan.repo_name,
                        worker = %orphan.worker_name,
                        error = %e,
                        "failed to open worktree for recovery"
                    );
                    continue;
                }
            };

            let repo_root = wt.repo_root();
            let jig_config = config::JigToml::load(&repo_root)
                .unwrap_or(None)
                .unwrap_or_default();
            let agent = agents::Agent::from_name(&jig_config.agent.agent_type)
                .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude))
                .with_disallowed_tools(jig_config.agent.disallowed_tools.clone());

            let context = Self::read_spawn_context(&orphan.repo_name, &orphan.worker_name)
                .unwrap_or_else(|| "You were interrupted. Resume your previous task.".to_string());
            let prompt = Prompt::new(jig_core::worker::SPAWN_PREAMBLE).task(&context);

            match Worker::resume(&wt, &agent, prompt) {
                Ok(_) => {
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
    ///
    /// Currently returns None — context is not stored in events.
    /// Recovery uses a fallback prompt instead.
    pub fn read_spawn_context(_repo_name: &str, _worker_name: &str) -> Option<String> {
        None
    }

    /// Check if a specific worker (by repo path and name) is orphaned and resume it.
    ///
    /// Used by the daemon during steady-state ticks when it detects a dead tmux window.
    pub fn try_resume_worker(repo_root: &Path, repo_name: &str, worker_name: &str) -> Result<bool> {
        let wt_path = repo_root.join(JIG_DIR).join(worker_name);
        let wt = Worktree::open(&wt_path)?;
        let w = Worker::from(&wt);

        if w.has_tmux_window() {
            return Ok(false);
        }

        let jig_config = config::JigToml::load(repo_root)?.unwrap_or_default();
        let agent = agents::Agent::from_name(&jig_config.agent.agent_type)
            .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude))
            .with_disallowed_tools(jig_config.agent.disallowed_tools.clone());

        let context = Self::read_spawn_context(repo_name, worker_name)
            .unwrap_or_else(|| "You were interrupted. Resume your previous task.".to_string());
        let prompt = Prompt::new(jig_core::worker::SPAWN_PREAMBLE).task(&context);

        Worker::resume(&wt, &agent, prompt)?;

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

        let event_log = EventLog::for_worker(repo_name, w.branch()).ok()?;
        let events = event_log.read_all().ok()?;
        if events.is_empty() {
            return None;
        }

        let state = WorkerState::reduce(&events, &self.health);

        if Self::should_recover(state.status) {
            Some(OrphanedWorker {
                repo_name: repo_name.to_string(),
                worker_name: w.branch().to_string(),
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
