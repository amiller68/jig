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

fn process_request(req: &IssueRequest) -> Vec<SpawnableIssue> {
    let jig_toml = match JigToml::load(&req.repo_root) {
        Ok(Some(t)) => t,
        _ => return vec![],
    };

    let global_config = match GlobalConfig::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(error = %e, "failed to load global config for issue poll");
            return vec![];
        }
    };

    let provider = match crate::issues::make_provider_with_ref(
        &req.repo_root,
        &jig_toml,
        &global_config,
        &req.base_branch,
    ) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(error = %e, "failed to create issue provider");
            return vec![];
        }
    };

    let issues = match provider.list_spawnable() {
        Ok(issues) => issues,
        Err(e) => {
            tracing::debug!(error = %e, "failed to list spawnable issues");
            return vec![];
        }
    };

    // How many more workers can we spawn?
    let active_count = req.existing_workers.len();
    let budget = req.max_concurrent_workers.saturating_sub(active_count);
    if budget == 0 {
        tracing::debug!(
            active = active_count,
            max = req.max_concurrent_workers,
            "at worker capacity, skipping auto-spawn"
        );
        return vec![];
    }

    issues
        .into_iter()
        .filter_map(|issue| {
            let worker_name = derive_worker_name(&issue.id);
            // Skip if a worker already exists for this issue
            if req.existing_workers.contains(&worker_name) {
                return None;
            }
            Some(SpawnableIssue {
                repo_root: req.repo_root.clone(),
                issue_id: issue.id,
                issue_title: issue.title,
                issue_body: issue.body,
                worker_name,
            })
        })
        .take(budget)
        .collect()
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
