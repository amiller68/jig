//! Prune actor — removes worktrees for merged/closed PRs in a background thread.

use super::messages::{PruneComplete, PruneRequest, PruneResult};
use crate::git::{Repo, Worktree};

/// Spawn the prune actor thread. Returns immediately.
///
/// The actor blocks on `rx.recv()` waiting for work, removes worktrees
/// and cleans up state for each target, then sends `PruneComplete` back.
pub fn spawn(
    rx: flume::Receiver<PruneRequest>,
    tx: flume::Sender<PruneComplete>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-prune".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let mut results = Vec::new();

                for target in &req.targets {
                    let key = format!("{}/{}", target.repo_name, target.worker_name);
                    let result = prune_single(target);
                    match result {
                        Ok(()) => {
                            tracing::info!(worker = %key, "pruned worktree");
                            results.push(PruneResult { key, error: None });
                        }
                        Err(msg) => {
                            tracing::warn!(worker = %key, "prune failed: {}", msg);
                            results.push(PruneResult {
                                key,
                                error: Some(msg),
                            });
                        }
                    }
                }

                if tx.send(PruneComplete { results }).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn prune actor thread")
}

/// Prune a single worker: git worktree remove, clean up events and state.
fn prune_single(target: &super::messages::PruneTarget) -> std::result::Result<(), String> {
    let worktree_path = crate::config::worktree_path(&target.repo_path, &target.worker_name);

    if worktree_path.exists() {
        let wt = Worktree::open(&worktree_path)
            .map_err(|e| format!("failed to open worktree: {}", e))?;
        wt.remove(false)
            .map_err(|e| format!("git worktree prune failed: {}", e))?;
    } else {
        let repo =
            Repo::open(&target.repo_path).map_err(|e| format!("failed to open repo: {}", e))?;
        repo.prune_stale_worktrees();
    }

    // Remove event logs
    if let Ok(events_dir) = crate::global::global_state_dir().map(|d| d.join("events")) {
        let sanitized = format!(
            "{}-{}",
            target.repo_name,
            target.worker_name.replace('/', "-")
        );
        let event_dir = events_dir.join(&sanitized);
        if event_dir.is_dir() {
            let _ = std::fs::remove_dir_all(&event_dir);
        }
    }

    // Remove global state entry.
    // NOTE: This load/save runs concurrently with the main tick thread's
    // WorkersState operations. This is safe because prune only removes keys
    // and a lost removal is re-applied on the next tick's recovery scan.
    let key = format!("{}/{}", target.repo_name, target.worker_name);
    let mut workers_state = crate::global::WorkersState::load().unwrap_or_default();
    workers_state.remove_worker(&key);
    workers_state.save().unwrap_or_else(|e| {
        tracing::warn!("failed to save workers state after prune: {}", e);
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_single_missing_worktree_succeeds() {
        // When the worktree path doesn't exist, prune_single should succeed as a no-op
        // (it still tries to clean events/state, which may also be absent).
        let tmp = tempfile::tempdir().unwrap();

        // Initialize a git repo so git2 can open it
        git2::Repository::init(tmp.path()).unwrap();

        let target = super::super::messages::PruneTarget {
            repo_path: tmp.path().to_path_buf(),
            repo_name: "test-repo".to_string(),
            worker_name: "nonexistent-worker".to_string(),
        };
        // Should not panic; may return Ok or Err depending on state dir access,
        // but must not panic on absent worktree.
        let _ = prune_single(&target);
    }

    #[test]
    fn prune_single_absent_event_log_no_panic() {
        let tmp = tempfile::tempdir().unwrap();

        // Initialize a git repo so git2 can open it
        git2::Repository::init(tmp.path()).unwrap();

        let target = super::super::messages::PruneTarget {
            repo_path: tmp.path().to_path_buf(),
            repo_name: "repo".to_string(),
            worker_name: "worker".to_string(),
        };
        // Must not panic even when event log dirs don't exist
        let _ = prune_single(&target);
    }
}
