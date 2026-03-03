//! Sync actor — runs `git fetch` in a background thread.

use std::process::Command;

use super::messages::{SyncComplete, SyncRequest};

/// Spawn the sync actor thread. Returns immediately.
///
/// The actor blocks on `rx.recv()` waiting for work, runs git fetch for each
/// repo, and sends `SyncComplete` back on the response channel.
pub fn spawn(
    rx: flume::Receiver<SyncRequest>,
    tx: flume::Sender<SyncComplete>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-sync".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let mut errors = Vec::new();

                for (name, path, base_branch) in &req.repos {
                    if !path.exists() {
                        continue;
                    }
                    let (remote, branch) = base_branch
                        .split_once('/')
                        .unwrap_or(("origin", base_branch));

                    match Command::new("git")
                        .args(["fetch", remote, branch])
                        .current_dir(path)
                        .stdin(std::process::Stdio::null())
                        .output()
                    {
                        Ok(o) if o.status.success() => {
                            tracing::debug!(repo = %name, "fetched {}", base_branch);
                        }
                        Ok(o) => {
                            let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
                            tracing::debug!(repo = %name, "fetch failed: {}", msg);
                            errors.push((name.clone(), msg));
                        }
                        Err(e) => {
                            tracing::debug!(repo = %name, "fetch failed: {}", e);
                            errors.push((name.clone(), e.to_string()));
                        }
                    }
                }

                if tx.send(SyncComplete { errors }).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn sync actor thread")
}
