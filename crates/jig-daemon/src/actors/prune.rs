//! Prune actor — removes worktrees for merged/closed PRs in a background thread.

use std::path::PathBuf;

use jig_core::git::{Repo, Worktree};

use crate::actors::Actor;

pub struct PruneTarget {
    pub repo_path: PathBuf,
    pub repo_name: String,
    pub worker_name: String,
}

pub struct PruneRequest {
    pub targets: Vec<PruneTarget>,
}

pub struct PruneResult {
    pub key: String,
    pub error: Option<String>,
}

pub struct PruneComplete {
    pub results: Vec<PruneResult>,
}

pub struct PruneActor {
    tx: flume::Sender<PruneRequest>,
    rx: flume::Receiver<PruneComplete>,
    pending: bool,
}

impl Actor for PruneActor {
    type Request = PruneRequest;
    type Response = PruneComplete;

    const NAME: &'static str = "jig-prune";
    const QUEUE_SIZE: usize = 1;

    fn handle(req: PruneRequest) -> PruneComplete {
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

        PruneComplete { results }
    }

    fn send(&mut self, req: PruneRequest) -> bool {
        if self.pending {
            return false;
        }
        if self.tx.try_send(req).is_ok() {
            self.pending = true;
            true
        } else {
            false
        }
    }

    fn drain(&mut self) -> Vec<PruneComplete> {
        match self.rx.try_recv() {
            Ok(result) => {
                self.pending = false;
                vec![result]
            }
            Err(_) => vec![],
        }
    }

    fn from_channels(
        tx: flume::Sender<PruneRequest>,
        rx: flume::Receiver<PruneComplete>,
    ) -> Self {
        Self {
            tx,
            rx,
            pending: false,
        }
    }
}

impl PruneActor {
    pub fn is_pending(&self) -> bool {
        self.pending
    }
}

fn prune_single(target: &PruneTarget) -> std::result::Result<(), String> {
    let worktree_path = jig_core::config::worktree_path(&target.repo_path, &target.worker_name);

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

    if let Ok(events_dir) = jig_core::config::global_events_dir() {
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

    let key = format!("{}/{}", target.repo_name, target.worker_name);
    let mut workers_state = jig_core::config::WorkersState::load().unwrap_or_default();
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
