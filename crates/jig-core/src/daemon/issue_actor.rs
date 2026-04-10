//! Issue actor — polls for auto-spawnable issues in a background thread.

use std::path::Path;
use std::process::Command;

use crate::context::RepoContext;
use crate::issues::naming::derive_worker_name;
use crate::issues::types::{Issue, IssueStatus};

use super::messages::{IssueRequest, SpawnableIssue};

/// Spawn the issue actor thread. Returns immediately.
pub fn spawn(
    rx: flume::Receiver<IssueRequest>,
    tx: flume::Sender<Vec<SpawnableIssue>>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-issues".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let result = process_request(&req);
                if tx.send(result).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn issue actor thread")
}

/// Process an issue request synchronously. Used by both the actor thread and
/// the blocking `tick_once` path.
///
/// Each repo is checked independently: its own `jig.toml` controls whether
/// auto-spawn is enabled and the per-repo worker budget.
pub(crate) fn process_request(req: &IssueRequest) -> Vec<SpawnableIssue> {
    let mut all_spawnable = Vec::new();

    for (repo_root, base_branch) in &req.repos {
        let ctx = match RepoContext::from_path(repo_root) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::debug!(error = %e, "failed to load repo context for issue poll");
                continue;
            }
        };

        // Skip repos that don't have auto-spawn enabled (no auto_spawn_labels)
        let Some(labels) = &ctx.jig_toml.issues.auto_spawn_labels else {
            continue;
        };

        // Count existing workers for this repo
        let repo_name = repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let repo_worker_count = req
            .existing_workers
            .iter()
            .filter(|(rn, _)| rn == &repo_name)
            .count();
        let max_workers = ctx
            .jig_toml
            .spawn
            .resolve_max_concurrent_workers(&ctx.global_config.spawn);
        let budget = max_workers.saturating_sub(repo_worker_count);

        if budget == 0 {
            tracing::debug!(
                repo = %repo_name,
                active = repo_worker_count,
                max = max_workers,
                "repo at worker capacity, skipping"
            );
            continue;
        }

        let provider = match ctx.issue_provider_with_ref(base_branch) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(repo = %repo_name, error = %e, "failed to create issue provider");
                continue;
            }
        };

        let issues = match provider.list_spawnable(labels) {
            Ok(issues) => issues,
            Err(e) => {
                tracing::debug!(repo = %repo_name, error = %e, "failed to list spawnable issues");
                continue;
            }
        };

        // Filter out child issues whose parent isn't ready
        let issues: Vec<_> = issues
            .into_iter()
            .filter(|issue| is_child_spawnable(issue, repo_root))
            .collect();

        let provider_kind = provider.kind();

        let mut repo_spawned = 0;
        for issue in issues {
            if repo_spawned >= budget {
                break;
            }
            let worker_name = derive_worker_name(&issue.id, issue.branch_name.as_deref());
            // Skip if a worker already exists for this issue
            if req
                .existing_workers
                .iter()
                .any(|(_, wn)| wn == &worker_name)
            {
                continue;
            }
            all_spawnable.push(SpawnableIssue {
                repo_root: repo_root.clone(),
                issue,
                worker_name,
                provider_kind,
            });
            repo_spawned += 1;
        }
    }

    all_spawnable
}

/// Returns `true` if the issue is spawnable with respect to its parent.
///
/// Non-child issues always pass. Child issues require their parent to be
/// `InProgress` and to have pushed a branch to the remote.
fn is_child_spawnable(issue: &Issue, repo_root: &Path) -> bool {
    let Some(parent) = &issue.parent else {
        return true;
    };

    // Parent must be InProgress
    if parent.status.as_ref() != Some(&IssueStatus::InProgress) {
        tracing::debug!(
            issue = %issue.id,
            parent = %parent.id,
            parent_status = ?parent.status,
            "child not spawnable: parent not InProgress"
        );
        return false;
    }

    // Parent must have a branch that exists on the remote
    let Some(branch) = &parent.branch_name else {
        tracing::debug!(
            issue = %issue.id,
            parent = %parent.id,
            "child not spawnable: parent has no branch name"
        );
        return false;
    };

    if !remote_branch_exists(repo_root, branch) {
        tracing::debug!(
            issue = %issue.id,
            parent = %parent.id,
            branch = %branch,
            "child not spawnable: parent branch not on remote"
        );
        return false;
    }

    true
}

/// Checks whether a branch exists on the `origin` remote by verifying the ref.
fn remote_branch_exists(repo_root: &Path, branch: &str) -> bool {
    Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            &format!("refs/remotes/origin/{branch}"),
        ])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
