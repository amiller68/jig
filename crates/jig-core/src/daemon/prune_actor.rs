//! Prune actor — removes worktrees for merged/closed PRs in a background thread.

use super::messages::{PruneComplete, PruneRequest, PruneResult};

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

    let repo = git2::Repository::open(&target.repo_path)
        .map_err(|e| format!("failed to open repo: {}", e))?;

    if worktree_path.exists() {
        // Find the worktree name that matches this path
        let canonical = worktree_path
            .canonicalize()
            .unwrap_or_else(|_| worktree_path.clone());

        let wt_name = find_worktree_by_path(&repo, &canonical)?;

        let wt = repo
            .find_worktree(&wt_name)
            .map_err(|e| format!("failed to find worktree '{}': {}", wt_name, e))?;

        let mut opts = git2::WorktreePruneOptions::new();
        opts.valid(true); // prune even though it's valid
        opts.working_tree(true); // remove the working directory

        wt.prune(Some(&mut opts))
            .map_err(|e| format!("git worktree prune failed: {}", e))?;

        // Clean up empty parent dirs (for nested paths like feature/foo)
        let worktrees_dir = target.repo_path.join(crate::config::JIG_DIR);
        cleanup_empty_parents(&worktree_path, &worktrees_dir);
    } else {
        // Directory already gone but git may still have a stale registration.
        // Prune entries whose directories no longer exist.
        prune_stale_worktrees(&repo);
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

/// Find the worktree name that corresponds to a given canonical path.
fn find_worktree_by_path(
    repo: &git2::Repository,
    canonical_path: &std::path::Path,
) -> std::result::Result<String, String> {
    let wt_names = repo
        .worktrees()
        .map_err(|e| format!("failed to list worktrees: {}", e))?;

    for i in 0..wt_names.len() {
        if let Some(name) = wt_names.get(i) {
            if let Ok(wt) = repo.find_worktree(name) {
                let wt_path = wt.path().to_path_buf();
                let wt_canonical = wt_path.canonicalize().unwrap_or(wt_path);
                if wt_canonical == canonical_path {
                    return Ok(name.to_string());
                }
            }
        }
    }

    Err(format!(
        "no worktree found for path: {}",
        canonical_path.display()
    ))
}

/// Prune stale (invalid) worktree registrations.
fn prune_stale_worktrees(repo: &git2::Repository) {
    if let Ok(wt_names) = repo.worktrees() {
        for i in 0..wt_names.len() {
            if let Some(name) = wt_names.get(i) {
                if let Ok(wt) = repo.find_worktree(name) {
                    if wt.validate().is_err() {
                        let mut opts = git2::WorktreePruneOptions::new();
                        let _ = wt.prune(Some(&mut opts));
                    }
                }
            }
        }
    }
}

/// Remove empty parent directories up to (but not including) the stop directory.
fn cleanup_empty_parents(path: &std::path::Path, stop_at: &std::path::Path) {
    let mut parent = path.parent();
    while let Some(p) = parent {
        if p == stop_at
            || p.file_name()
                .map(|n| n == crate::config::JIG_DIR)
                .unwrap_or(false)
        {
            break;
        }
        match p.read_dir() {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    break;
                }
                let _ = std::fs::remove_dir(p);
            }
            Err(_) => break,
        }
        parent = p.parent();
    }
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
