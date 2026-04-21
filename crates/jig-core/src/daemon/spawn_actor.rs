//! Spawn actor — creates worktrees and launches agents in a background thread.
//!
//! This keeps the on-create hook (which can be slow, e.g. `pnpm install`)
//! off the main tick thread so the UI stays responsive.
//!
//! Delegates to [`crate::spawn::spawn_worker_for_issue`] and
//! [`crate::spawn::spawn_wrapup_for_issue`] for the actual spawn sequences,
//! ensuring consistent behavior with the blocking `tick_once` path.

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
                    results.push(run_and_record(issue, false));
                }
                for issue in &req.wrapup {
                    results.push(run_and_record(issue, true));
                }

                if tx.send(SpawnComplete { results }).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn spawn actor thread")
}

/// Dispatch a spawn via the appropriate shared codepath, log the outcome, and
/// build the [`SpawnResult`] for the reply channel.
fn run_and_record(issue: &SpawnableIssue, is_wrapup: bool) -> SpawnResult {
    let input = SpawnIssueInput {
        repo_root: &issue.repo_root,
        issue: &issue.issue,
        worker_name: &issue.worker_name,
        provider_kind: issue.provider_kind,
    };
    let result = if is_wrapup {
        spawn::spawn_wrapup_for_issue(&input)
    } else {
        spawn::spawn_worker_for_issue(&input)
    };
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
            SpawnResult {
                worker_name,
                repo_name,
                issue_id,
                error: None,
            }
        }
        Err(msg) => {
            tracing::warn!(worker = %worker_name, "auto-spawn failed: {}", msg);
            SpawnResult {
                worker_name,
                repo_name,
                issue_id,
                error: Some(msg),
            }
        }
    }
}
