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
                issue_id: issue.id,
                issue_title: issue.title,
                issue_body: issue.body,
                worker_name,
                provider_kind,
                branch_name: issue.branch_name,
            });
            repo_spawned += 1;
        }
    }

    all_spawnable
}

/// Derive a worker name from an issue ID and optional branch name.
///
/// When a branch name is available (e.g. Linear's `branchName` field like
/// `feature/aut-4969-spawn-agent-thread-is-broken`), it is used as-is since
/// it is already a valid git branch name.
///
/// For file-based issues (no branch name), the ID is lowercased and used
/// directly — it already contains a descriptive slug.
fn derive_worker_name(issue_id: &str, branch_name: Option<&str>) -> String {
    match branch_name {
        Some(name) if !name.is_empty() => sanitize_worker_name(name),
        _ => issue_id.to_lowercase(),
    }
}

/// Sanitize a branch name for use as a git worktree/branch name.
///
/// Applies git ref naming rules: no leading dots, no `.lock` suffix,
/// no `..`, no ASCII control chars, no `\`, no spaces.
fn sanitize_worker_name(name: &str) -> String {
    let mut result: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_control() || c == '\\' || c == ' ' || c == '~' || c == '^' || c == ':' {
                '-'
            } else {
                c
            }
        })
        .collect();

    // Collapse consecutive dots (no "..")
    while result.contains("..") {
        result = result.replace("..", ".");
    }

    // Strip leading dots
    result = result.trim_start_matches('.').to_string();

    // Strip trailing ".lock"
    if result.ends_with(".lock") {
        result.truncate(result.len() - 5);
    }

    // Strip trailing dots and slashes
    result = result.trim_end_matches(&['.', '/'][..]).to_string();

    if result.is_empty() {
        "worker".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_worker_name_linear_no_branch() {
        assert_eq!(derive_worker_name("ENG-123", None), "eng-123");
    }

    #[test]
    fn derive_worker_name_linear_with_branch() {
        assert_eq!(
            derive_worker_name(
                "AUT-4969",
                Some("feature/aut-4969-spawn-agent-thread-is-broken")
            ),
            "feature/aut-4969-spawn-agent-thread-is-broken"
        );
    }

    #[test]
    fn derive_worker_name_linear_empty_branch() {
        assert_eq!(derive_worker_name("ENG-123", Some("")), "eng-123");
    }

    #[test]
    fn derive_worker_name_preserves_category_prefix() {
        assert_eq!(
            derive_worker_name("features/my-feature", None),
            "features/my-feature"
        );
    }

    #[test]
    fn derive_worker_name_preserves_nested_prefix() {
        assert_eq!(
            derive_worker_name("features/global-attach", None),
            "features/global-attach"
        );
    }

    #[test]
    fn derive_worker_name_preserves_bugs_prefix() {
        assert_eq!(derive_worker_name("bugs/fix-foo", None), "bugs/fix-foo");
    }

    #[test]
    fn derive_worker_name_simple() {
        assert_eq!(derive_worker_name("fix-bug", None), "fix-bug");
    }

    #[test]
    fn sanitize_worker_name_strips_leading_dot() {
        assert_eq!(sanitize_worker_name(".hidden"), "hidden");
    }

    #[test]
    fn sanitize_worker_name_strips_dot_lock() {
        assert_eq!(sanitize_worker_name("branch.lock"), "branch");
    }

    #[test]
    fn sanitize_worker_name_collapses_double_dots() {
        assert_eq!(sanitize_worker_name("a..b"), "a.b");
    }

    #[test]
    fn sanitize_worker_name_replaces_control_chars() {
        assert_eq!(sanitize_worker_name("a\tb"), "a-b");
    }

    #[test]
    fn sanitize_worker_name_replaces_spaces() {
        assert_eq!(sanitize_worker_name("a b"), "a-b");
    }

    #[test]
    fn sanitize_worker_name_empty_fallback() {
        assert_eq!(sanitize_worker_name("..."), "worker");
    }
}
