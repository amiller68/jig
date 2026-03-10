//! Issue actor — polls for auto-spawnable issues in a background thread.

use crate::config::JigToml;
use crate::global::GlobalConfig;

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
    let global_config = match GlobalConfig::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(error = %e, "failed to load global config for issue poll");
            return vec![];
        }
    };

    let mut all_spawnable = Vec::new();

    for (repo_root, base_branch) in &req.repos {
        let jig_toml = match JigToml::load(repo_root) {
            Ok(Some(t)) => t,
            _ => continue,
        };

        // Skip repos that don't have auto-spawn enabled
        if !jig_toml.spawn.resolve_auto_spawn(&global_config.spawn) {
            continue;
        }

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
        let max_workers = jig_toml
            .spawn
            .resolve_max_concurrent_workers(&global_config.spawn);
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

        let provider = match crate::issues::make_provider_with_ref(
            repo_root,
            &jig_toml,
            &global_config,
            base_branch,
        ) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(repo = %repo_name, error = %e, "failed to create issue provider");
                continue;
            }
        };

        let issues = match provider.list_spawnable(&jig_toml.issues.spawn_labels) {
            Ok(issues) => issues,
            Err(e) => {
                tracing::debug!(repo = %repo_name, error = %e, "failed to list spawnable issues");
                continue;
            }
        };

        let mut repo_spawned = 0;
        for issue in issues {
            if repo_spawned >= budget {
                break;
            }
            let worker_name = derive_worker_name(&issue.id);
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
                issue_id: issue.id,
                issue_title: issue.title,
                issue_body: issue.body,
                worker_name,
            });
            repo_spawned += 1;
        }
    }

    all_spawnable
}

/// Derive a worker name from an issue ID.
/// "ENG-123" → "eng-123", "features/my-feature" → "my-feature"
fn derive_worker_name(issue_id: &str) -> String {
    let name = issue_id.rsplit('/').next().unwrap_or(issue_id);
    name.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_worker_name_linear() {
        assert_eq!(derive_worker_name("ENG-123"), "eng-123");
    }

    #[test]
    fn derive_worker_name_file() {
        assert_eq!(derive_worker_name("features/my-feature"), "my-feature");
    }

    #[test]
    fn derive_worker_name_simple() {
        assert_eq!(derive_worker_name("fix-bug"), "fix-bug");
    }
}
