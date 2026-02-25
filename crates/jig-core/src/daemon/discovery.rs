//! Worker discovery — scanning events directory for active workers.

use crate::registry::RepoRegistry;

/// Discover all workers by scanning the events directory.
pub(crate) fn discover_workers(registry: &RepoRegistry) -> Vec<(String, String)> {
    let mut workers = vec![];

    // Scan the events directory for worker event logs
    let events_dir = match crate::global::global_state_dir() {
        Ok(dir) => dir.join("events"),
        Err(_) => return workers,
    };

    if !events_dir.is_dir() {
        return workers;
    }

    // Each subdirectory is named "<repo>-<worker>" and contains events.jsonl
    let entries = match std::fs::read_dir(&events_dir) {
        Ok(entries) => entries,
        Err(_) => return workers,
    };

    // Build a set of known repo names from registry for matching
    let repo_names: Vec<String> = registry
        .repos()
        .iter()
        .filter_map(|entry| {
            entry
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .collect();

    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let events_file = entry.path().join("events.jsonl");

        if !events_file.exists() {
            continue;
        }

        // Try to split "repo-worker" — match longest registered repo name prefix
        if let Some((repo, worker)) = split_worker_dir(&dir_name, &repo_names) {
            workers.push((repo, worker));
        }
    }

    workers
}

/// Split a directory name like "myrepo-feat-branch" into (repo, worker).
/// Uses known repo names to find the correct split point.
fn split_worker_dir(dir_name: &str, repo_names: &[String]) -> Option<(String, String)> {
    // Try each known repo name as a prefix
    for repo_name in repo_names {
        let prefix = format!("{}-", repo_name);
        if let Some(worker) = dir_name.strip_prefix(&prefix) {
            if !worker.is_empty() {
                return Some((repo_name.clone(), worker.to_string()));
            }
        }
    }

    // Fallback: split on first dash
    let dash = dir_name.find('-')?;
    let repo = &dir_name[..dash];
    let worker = &dir_name[dash + 1..];
    if worker.is_empty() {
        return None;
    }
    Some((repo.to_string(), worker.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_worker_dir_with_known_repo() {
        let repos = vec!["myrepo".to_string(), "jig".to_string()];
        let result = split_worker_dir("myrepo-feat-branch", &repos);
        assert_eq!(
            result,
            Some(("myrepo".to_string(), "feat-branch".to_string()))
        );
    }

    #[test]
    fn split_worker_dir_fallback() {
        let repos: Vec<String> = vec![];
        let result = split_worker_dir("myrepo-feat", &repos);
        assert_eq!(result, Some(("myrepo".to_string(), "feat".to_string())));
    }

    #[test]
    fn split_worker_dir_no_dash() {
        let repos: Vec<String> = vec![];
        let result = split_worker_dir("nodash", &repos);
        assert_eq!(result, None);
    }

    #[test]
    fn split_worker_dir_prefers_known_repo() {
        let repos = vec!["my-repo".to_string()];
        let result = split_worker_dir("my-repo-feat", &repos);
        assert_eq!(result, Some(("my-repo".to_string(), "feat".to_string())));
    }
}
