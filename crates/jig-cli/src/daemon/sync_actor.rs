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

                // Fetch base branches for each repo
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

                // Fetch parent branches so remote refs are current for
                // parent worktree auto-update after child PR merges.
                for (name, path, parent_branch) in &req.parent_branches {
                    if !path.exists() {
                        continue;
                    }
                    let (remote, branch) = parent_branch
                        .split_once('/')
                        .unwrap_or(("origin", parent_branch));

                    match Command::new("git")
                        .args(["fetch", remote, branch])
                        .current_dir(path)
                        .stdin(std::process::Stdio::null())
                        .output()
                    {
                        Ok(o) if o.status.success() => {
                            tracing::debug!(repo = %name, "fetched parent branch {}", parent_branch);
                        }
                        Ok(o) => {
                            let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
                            tracing::debug!(repo = %name, "parent branch fetch failed: {}", msg);
                            // Parent branch fetch failures are non-fatal, just log
                        }
                        Err(e) => {
                            tracing::debug!(repo = %name, "parent branch fetch failed: {}", e);
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
