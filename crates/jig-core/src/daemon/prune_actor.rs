//! Prune actor — removes worktrees for merged/closed PRs in a background thread.

use std::process::{Command, Stdio};

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

    if worktree_path.exists() {
        // Remove the git worktree (no --force: fails safely on uncommitted changes)
        let worktree_str = worktree_path.to_string_lossy().to_string();
        let output = Command::new("git")
            .args(["worktree", "remove", &worktree_str])
            .current_dir(&target.repo_path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("failed to run git worktree remove: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!("git worktree remove failed: {}", stderr));
        }

        // Clean up empty parent dirs (for nested paths like feature/foo)
        let worktrees_dir = target.repo_path.join(crate::config::JIG_DIR);
        cleanup_empty_parents(&worktree_path, &worktrees_dir);
    } else {
        // Directory already gone but git may still have a stale registration.
        // Prune clears entries whose directories no longer exist.
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&target.repo_path)
            .stdin(Stdio::null())
            .output();
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
        let target = super::super::messages::PruneTarget {
            repo_path: tmp.path().to_path_buf(),
            repo_name: "repo".to_string(),
            worker_name: "worker".to_string(),
        };
        // Must not panic even when event log dirs don't exist
        let _ = prune_single(&target);
    }
}
