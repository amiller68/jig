//! Sync actor — runs `git fetch` in a background thread.

use jig_core::git::Repo;

use super::{Actor, TickContext};

pub struct SyncRequest {
    pub ctx: TickContext,
}

#[derive(Default)]
pub struct SyncActor;

impl Actor for SyncActor {
    type Request = SyncRequest;
    type Response = ();

    const NAME: &'static str = "jig-sync";
    const QUEUE_SIZE: usize = 1;

    fn handle(&self, req: SyncRequest) {
        for entry in req.ctx.repos.iter() {
            if !entry.path.exists() {
                continue;
            }
            let name = entry
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let repo = match Repo::open(&entry.path) {
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
