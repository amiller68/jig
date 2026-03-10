//! Daemon crash recovery — detect and re-launch orphaned workers.

use crate::config::JIG_DIR;
use crate::context::RepoContext;
use crate::error::Result;
use crate::events::{EventLog, WorkerState};
use crate::global::GlobalConfig;
use crate::registry::RepoRegistry;
use crate::worker::WorkerStatus;

use super::discovery::discover_workers;

/// An orphaned worker that can be recovered.
#[derive(Debug)]
pub struct OrphanedWorker {
    pub repo_name: String,
    pub worker_name: String,
    pub repo_root: std::path::PathBuf,
    pub status: WorkerStatus,
}

/// Find workers whose worktrees and event logs exist but whose tmux windows are dead.
pub fn find_orphaned_workers(registry: &RepoRegistry, session_prefix: &str) -> Vec<OrphanedWorker> {
    let workers = discover_workers(registry);
    let mut orphans = Vec::new();

    for (repo_name, worker_name) in &workers {
        let session = format!("{}{}", session_prefix, repo_name);

        // Check if tmux window is alive
        let tmux_alive = crate::session::session_exists(&session)
            && crate::session::window_exists(&session, worker_name);

        if tmux_alive {
            continue; // Window exists, not orphaned
        }

        // Read event log and derive state
        let event_log = match EventLog::for_worker(repo_name, worker_name) {
            Ok(log) => log,
            Err(_) => continue,
        };

        if !event_log.exists() {
            continue;
        }

        let events = match event_log.read_all() {
            Ok(e) => e,
            Err(_) => continue,
        };

        if events.is_empty() {
            continue;
        }

        let global_config = GlobalConfig::load().unwrap_or_default();
        let state = WorkerState::reduce(&events, &global_config.health);

        // Only recover active workers (Spawned/Running/Stalled)
        // Skip terminal states and states that don't need recovery
        let should_recover = matches!(
            state.status,
            WorkerStatus::Spawned | WorkerStatus::Running | WorkerStatus::Stalled
        );

        if !should_recover {
            continue;
        }

        // Find the repo root
        let repo_root = registry
            .repos()
            .iter()
            .find(|e| {
                e.path
                    .file_name()
                    .map(|n| n.to_string_lossy() == *repo_name)
                    .unwrap_or(false)
            })
            .map(|e| e.path.clone());

        if let Some(repo_root) = repo_root {
            let worktree_path = repo_root.join(JIG_DIR).join(worker_name);
            if worktree_path.exists() {
                orphans.push(OrphanedWorker {
                    repo_name: repo_name.clone(),
                    worker_name: worker_name.clone(),
                    repo_root,
                    status: state.status,
                });
            }
        }
    }

    orphans
}

/// Recover orphaned workers by re-launching them in tmux.
///
/// Returns the list of workers that were successfully recovered.
pub fn recover_orphaned_workers(orphans: &[OrphanedWorker], session_prefix: &str) -> Vec<String> {
    let mut recovered = Vec::new();

    for orphan in orphans {
        tracing::info!(
            worker = %orphan.worker_name,
            repo = %orphan.repo_name,
            status = orphan.status.as_str(),
            "recovering orphaned worker"
        );

        let repo_ctx =
            match build_repo_context(&orphan.repo_root, &orphan.repo_name, session_prefix) {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::warn!(
                        worker = %orphan.worker_name,
                        error = %e,
                        "failed to build repo context for recovery"
                    );
                    continue;
                }
            };

        // Get original context from event log
        let context = crate::spawn::extract_spawn_context(&orphan.repo_name, &orphan.worker_name);

        // All recovery is done in auto mode
        match crate::spawn::resume_worker(&repo_ctx, &orphan.worker_name, true, context.as_deref())
        {
            Ok(()) => {
                let key = format!("{}/{}", orphan.repo_name, orphan.worker_name);
                tracing::info!(worker = %key, "worker recovered");
                recovered.push(key);
            }
            Err(e) => {
                tracing::warn!(
                    worker = %orphan.worker_name,
                    error = %e,
                    "failed to recover worker"
                );
            }
        }
    }

    recovered
}

/// Build a RepoContext from a repo root path (public, for use by other daemon modules).
pub fn build_repo_context_pub(
    repo_root: &std::path::Path,
    repo_name: &str,
    session_prefix: &str,
) -> Result<RepoContext> {
    build_repo_context(repo_root, repo_name, session_prefix)
}

/// Build a RepoContext from a repo root path (for recovery, where we don't have a CWD context).
fn build_repo_context(
    repo_root: &std::path::Path,
    repo_name: &str,
    session_prefix: &str,
) -> Result<RepoContext> {
    let repo = crate::git::Repo::open(repo_root)?;
    let git_common_dir = repo.common_dir();
    let worktrees_dir = repo_root.join(JIG_DIR);
    let base_branch = RepoContext::resolve_base_branch_for(repo_root)
        .unwrap_or_else(|_| crate::config::DEFAULT_BASE_BRANCH.to_string());
    let session_name = format!("{}{}", session_prefix, repo_name);

    Ok(RepoContext {
        repo_root: repo_root.to_path_buf(),
        worktrees_dir,
        git_common_dir,
        base_branch,
        session_name,
    })
}
