//! Spawn actor — creates worktrees and launches agents in a background thread.
//!
//! This keeps the on-create hook (which can be slow, e.g. `pnpm install`)
//! off the main tick thread so the UI stays responsive.
//!
//! Delegates to [`crate::spawn::spawn_worker_for_issue`] for the actual spawn
//! sequence, ensuring consistent behavior with the blocking `tick_once` path.

use crate::spawn::{self, SpawnIssueInput};

use super::messages::{SpawnComplete, SpawnRequest, SpawnResult, SpawnableIssue};

/// Spawn the spawn actor thread. Returns immediately.
pub fn spawn(
    rx: flume::Receiver<SpawnRequest>,
    tx: flume::Sender<SpawnComplete>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-spawn".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let mut results = Vec::new();

                for issue in &req.issues {
                    let result = spawn_single(issue);
                    let worker_name = issue.worker_name.clone();
                    let repo_name = issue
                        .repo_root
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let issue_id = Some(issue.issue.id.clone());
                    match result {
                        Ok(()) => {
                            tracing::info!(worker = %worker_name, "auto-spawned worker");
                            results.push(SpawnResult {
                                worker_name,
                                repo_name,
                                issue_id,
                                error: None,
                            });
                        }
                        Err(msg) => {
                            tracing::warn!(worker = %worker_name, "auto-spawn failed: {}", msg);
                            results.push(SpawnResult {
                                worker_name,
                                repo_name,
                                issue_id,
                                error: Some(msg),
                            });
                        }
                    }
                }

                if tx.send(SpawnComplete { results }).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn spawn actor thread")
}

/// Spawn a single worker via the shared [`spawn::spawn_worker_for_issue`] codepath.
fn spawn_single(issue: &SpawnableIssue) -> std::result::Result<(), String> {
    let input = SpawnIssueInput {
        repo_root: &issue.repo_root,
        issue: &issue.issue,
        worker_name: &issue.worker_name,
        provider_kind: issue.provider_kind,
        kind: issue.kind,
    };
    spawn::spawn_worker_for_issue(&input)
}
