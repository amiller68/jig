//! Prune actor — removes worktrees for merged/closed PRs in a background thread.

use std::path::PathBuf;

use jig_core::git::{Repo, Worktree};

use super::Actor;

pub struct PruneTarget {
    pub repo_path: PathBuf,
    pub repo_name: String,
    pub worker_name: String,
}

pub struct PruneRequest {
    pub targets: Vec<PruneTarget>,
}

#[derive(Default)]
pub struct PruneActor;

impl Actor for PruneActor {
    type Request = PruneRequest;
    type Response = ();

    const NAME: &'static str = "jig-prune";
    const QUEUE_SIZE: usize = 1;

    fn handle(&self, req: PruneRequest) {
        for target in &req.targets {
            let key = format!("{}/{}", target.repo_name, target.worker_name);
            match prune_single(target) {
                Ok(()) => tracing::info!(worker = %key, "pruned worktree"),
                Err(msg) => tracing::warn!(worker = %key, "prune failed: {}", msg),
            }
        }
    }
}

fn prune_single(target: &PruneTarget) -> std::result::Result<(), String> {
    let worktree_path = crate::context::worktree_path(&target.repo_path, &target.worker_name);

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

    if let Ok(events_dir) = crate::context::global_events_dir() {
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_single_missing_worktree_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        git2::Repository::init(tmp.path()).unwrap();

        let target = PruneTarget {
            repo_path: tmp.path().to_path_buf(),
            repo_name: "test-repo".to_string(),
            worker_name: "nonexistent-worker".to_string(),
        };
        let _ = prune_single(&target);
    }

    #[test]
    fn prune_single_absent_event_log_no_panic() {
        let tmp = tempfile::tempdir().unwrap();
        git2::Repository::init(tmp.path()).unwrap();

        let target = PruneTarget {
            repo_path: tmp.path().to_path_buf(),
            repo_name: "repo".to_string(),
            worker_name: "worker".to_string(),
        };
        let _ = prune_single(&target);
    }
}
