//! Issue actor — polls for auto-spawnable and triageable issues in a background thread.

use crate::context::RepoContext;

use crate::issues::naming::derive_worker_name;

use super::messages::{IssueRequest, IssueResponse, SpawnableIssue};

/// Spawn the issue actor thread. Returns immediately.
pub fn spawn(
    rx: flume::Receiver<IssueRequest>,
    tx: flume::Sender<IssueResponse>,
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
/// auto-spawn is enabled and the per-repo worker budget. Triage-eligible
/// issues (status=Triage, repo has `[triage] enabled = true`) are returned
/// separately from normal spawnable issues.
pub(crate) fn process_request(req: &IssueRequest) -> IssueResponse {
    let mut all_spawnable = Vec::new();
    let mut all_triageable = Vec::new();

    for (repo_root, base_branch) in &req.repos {
        let ctx = match RepoContext::from_path(repo_root) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::debug!(error = %e, "failed to load repo context for issue poll");
                continue;
            }
        };

        let repo_name = repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Count existing workers for this repo (shared budget for spawn + triage)
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

        // Triage path: check for triage-eligible issues if enabled
        if ctx.jig_toml.triage.enabled && budget > 0 {
            let provider = match ctx.issue_provider_with_ref(base_branch) {
                Ok(p) => p,
                Err(e) => {
                    tracing::debug!(repo = %repo_name, error = %e, "failed to create issue provider for triage");
                    // Fall through to spawnable path below
                    continue;
                }
            };

            let triageable = match provider.list_triageable() {
                Ok(issues) => issues,
                Err(e) => {
                    tracing::debug!(repo = %repo_name, error = %e, "failed to list triageable issues");
                    vec![]
                }
            };

            let provider_kind = provider.kind();

            for issue in triageable {
                let worker_name = format!("triage-{}", issue.id.to_lowercase());
                // Skip if a worker already exists for this issue
                if req
                    .existing_workers
                    .iter()
                    .any(|(_, wn)| wn == &worker_name)
                {
                    continue;
                }
                all_triageable.push(SpawnableIssue {
                    repo_root: repo_root.clone(),
                    issue,
                    worker_name,
                    provider_kind,
                });
            }

            // Auto-spawn path: check for spawnable issues
            let Some(labels) = &ctx.jig_toml.issues.auto_spawn_labels else {
                continue;
            };

            let spawnable_issues = match provider.list_spawnable(labels) {
                Ok(issues) => issues,
                Err(e) => {
                    tracing::debug!(repo = %repo_name, error = %e, "failed to list spawnable issues");
                    continue;
                }
            };

            let mut repo_spawned = 0;
            for issue in spawnable_issues {
                if repo_spawned >= budget {
                    break;
                }
                let worker_name = derive_worker_name(&issue.id, issue.branch_name.as_deref());
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
        } else {
            // No triage — original auto-spawn-only path
            let Some(labels) = &ctx.jig_toml.issues.auto_spawn_labels else {
                continue;
            };

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

            let provider_kind = provider.kind();

            let mut repo_spawned = 0;
            for issue in issues {
                if repo_spawned >= budget {
                    break;
                }
                let worker_name = derive_worker_name(&issue.id, issue.branch_name.as_deref());
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
    }

    IssueResponse {
        spawnable: all_spawnable,
        triageable: all_triageable,
    }
}
