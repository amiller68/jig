//! Issue actor — polls for auto-spawnable and triageable issues in a background thread.

use std::path::Path;

use crate::context::RepoContext;
use crate::issues::naming::derive_worker_name;
use crate::issues::provider::IssueProvider;
use crate::issues::ProviderKind;
use crate::spawn::SpawnKind;

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

/// Collect spawnable issues from a provider, respecting the budget and skipping
/// workers that already exist.
fn collect_spawnable(
    provider: &dyn IssueProvider,
    labels: &[String],
    repo_root: &Path,
    repo_name: &str,
    budget: usize,
    existing_workers: &[(String, String)],
) -> Vec<SpawnableIssue> {
    let issues = match provider.list_spawnable(labels) {
        Ok(issues) => issues,
        Err(e) => {
            tracing::debug!(repo = %repo_name, error = %e, "failed to list spawnable issues");
            return vec![];
        }
    };

    let provider_kind = provider.kind();
    let mut result = Vec::new();
    let mut repo_spawned = 0;

    for issue in issues {
        if repo_spawned >= budget {
            break;
        }
        let worker_name = derive_worker_name(&issue.id, issue.branch_name.as_deref());
        if existing_workers.iter().any(|(_, wn)| wn == &worker_name) {
            continue;
        }
        result.push(SpawnableIssue {
            repo_root: repo_root.to_path_buf(),
            issue,
            worker_name,
            provider_kind,
            kind: SpawnKind::Normal,
        });
        repo_spawned += 1;
    }

    result
}

/// Collect triageable issues from a provider, skipping workers that already exist.
fn collect_triageable(
    provider: &dyn IssueProvider,
    provider_kind: ProviderKind,
    repo_root: &Path,
    repo_name: &str,
    budget: usize,
    existing_workers: &[(String, String)],
) -> Vec<SpawnableIssue> {
    let triageable = match provider.list_triageable() {
        Ok(issues) => issues,
        Err(e) => {
            tracing::debug!(repo = %repo_name, error = %e, "failed to list triageable issues");
            return vec![];
        }
    };

    let mut result = Vec::new();
    let mut count = 0;

    for issue in triageable {
        if count >= budget {
            break;
        }
        let worker_name = format!("triage-{}", issue.id.to_lowercase());
        if existing_workers.iter().any(|(_, wn)| wn == &worker_name) {
            continue;
        }
        result.push(SpawnableIssue {
            repo_root: repo_root.to_path_buf(),
            issue,
            worker_name,
            provider_kind,
            kind: SpawnKind::Triage,
        });
        count += 1;
    }

    result
}

/// Process an issue request synchronously. Used by both the actor thread and
/// the blocking `tick_once` path.
///
/// Each repo is checked independently: its own `jig.toml` controls whether
/// auto-spawn is enabled and the per-repo worker budget. Triage-eligible
/// issues (status=Triage, repo has `[triage] enabled = true`) are returned
/// separately from normal spawnable issues. Both triage and spawn share
/// the worker budget.
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

        let provider_kind = provider.kind();
        let mut remaining_budget = budget;

        // Triage path: collect triage-eligible issues first (they share the budget)
        if ctx.jig_toml.triage.enabled {
            let triage_issues = collect_triageable(
                provider.as_ref(),
                provider_kind,
                repo_root,
                &repo_name,
                remaining_budget,
                &req.existing_workers,
            );
            remaining_budget = remaining_budget.saturating_sub(triage_issues.len());
            all_triageable.extend(triage_issues);
        }

        // Auto-spawn path: collect spawnable issues with remaining budget
        if let Some(labels) = &ctx.jig_toml.issues.auto_spawn_labels {
            let spawnable = collect_spawnable(
                provider.as_ref(),
                labels,
                repo_root,
                &repo_name,
                remaining_budget,
                &req.existing_workers,
            );
            all_spawnable.extend(spawnable);
        }
    }

    IssueResponse {
        spawnable: all_spawnable,
        triageable: all_triageable,
    }
}
