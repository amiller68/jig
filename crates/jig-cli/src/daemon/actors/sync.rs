//! Sync actor — runs `git fetch` in a background thread.

use std::path::PathBuf;

use jig_core::git::Repo;

use super::Actor;

pub struct SyncRequest {
    pub repos: Vec<(String, PathBuf)>,
}

#[derive(Default)]
pub struct SyncActor;

impl Actor for SyncActor {
    type Request = SyncRequest;
    type Response = ();

    const NAME: &'static str = "jig-sync";
    const QUEUE_SIZE: usize = 1;

    fn handle(&self, req: SyncRequest) {
        for (name, path) in &req.repos {
            if !path.exists() {
                continue;
            }
            let repo = match Repo::open(path) {
                Ok(r) => r,
                Err(e) => {
                    tracing::debug!(repo = %name, "fetch: failed to open repo: {}", e);
                    continue;
                }
            };
            match repo.fetch("origin", &[]) {
                Ok(()) => tracing::debug!(repo = %name, "fetched origin"),
                Err(e) => tracing::debug!(repo = %name, "fetch failed: {}", e),
            }
        }
    }
}
