//! Sync actor — runs `git fetch` in a background thread.

use std::path::PathBuf;

use jig_core::git::Repo;

use crate::actors::Actor;

pub struct SyncRequest {
    pub repos: Vec<(String, PathBuf, String)>,
    pub parent_branches: Vec<(String, PathBuf, String)>,
}

pub struct SyncComplete {
    pub errors: Vec<(String, String)>,
}

pub struct SyncActor {
    tx: flume::Sender<SyncRequest>,
    rx: flume::Receiver<SyncComplete>,
    pending: bool,
}

impl Actor for SyncActor {
    type Request = SyncRequest;
    type Response = SyncComplete;

    const NAME: &'static str = "jig-sync";
    const QUEUE_SIZE: usize = 1;

    fn handle(req: SyncRequest) -> SyncComplete {
        let mut errors = Vec::new();

        for (name, path, base_branch) in &req.repos {
            if !path.exists() {
                continue;
            }
            let repo = match Repo::open(path) {
                Ok(r) => r,
                Err(e) => {
                    errors.push((name.clone(), e.to_string()));
                    continue;
                }
            };
            let (remote, branch) = base_branch
                .split_once('/')
                .unwrap_or(("origin", base_branch));
            match repo.fetch(remote, &[branch]) {
                Ok(()) => tracing::debug!(repo = %name, "fetched {}", base_branch),
                Err(e) => {
                    tracing::debug!(repo = %name, "fetch failed: {}", e);
                    errors.push((name.clone(), e.to_string()));
                }
            }
        }

        for (name, path, parent_branch) in &req.parent_branches {
            if !path.exists() {
                continue;
            }
            let repo = match Repo::open(path) {
                Ok(r) => r,
                Err(e) => {
                    tracing::debug!(repo = %name, "parent fetch: failed to open repo: {}", e);
                    continue;
                }
            };
            let (remote, branch) = parent_branch
                .split_once('/')
                .unwrap_or(("origin", parent_branch));
            match repo.fetch(remote, &[branch]) {
                Ok(()) => tracing::debug!(repo = %name, "fetched parent branch {}", parent_branch),
                Err(e) => tracing::debug!(repo = %name, "parent branch fetch failed: {}", e),
            }
        }

        SyncComplete { errors }
    }

    fn send(&mut self, req: SyncRequest) -> bool {
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

    fn drain(&mut self) -> Vec<SyncComplete> {
        match self.rx.try_recv() {
            Ok(result) => {
                self.pending = false;
                for (repo, err) in &result.errors {
                    tracing::debug!(repo = %repo, "sync error: {}", err);
                }
                vec![result]
            }
            Err(_) => vec![],
        }
    }

    fn from_channels(
        tx: flume::Sender<SyncRequest>,
        rx: flume::Receiver<SyncComplete>,
    ) -> Self {
        Self {
            tx,
            rx,
            pending: false,
        }
    }
}

impl SyncActor {
    pub fn is_pending(&self) -> bool {
        self.pending
    }
}
