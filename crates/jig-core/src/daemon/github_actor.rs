//! GitHub actor — runs PR discovery and lifecycle checks in a background thread.

use crate::github::{self, GitHubClient, PrState};
use crate::registry::RepoRegistry;

use super::messages::{GitHubRequest, GitHubResponse};

/// Spawn the GitHub actor thread. Returns immediately.
///
/// Processes one `GitHubRequest` at a time (sequential to respect API rate limits).
pub fn spawn(
    rx: flume::Receiver<GitHubRequest>,
    tx: flume::Sender<GitHubResponse>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-github".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let response = process_request(&req);
                if tx.send(response).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn github actor thread")
}

fn process_request(req: &GitHubRequest) -> GitHubResponse {
    let registry = RepoRegistry::load().unwrap_or_default();
    let client = match make_client(&req.repo_name, &registry) {
        Some(c) => c,
        None => {
            return GitHubResponse {
                worker_key: req.worker_key.clone(),
                pr_url: req.pr_url.clone(),
                pr_checks: vec![],
                pr_error: Some("GitHub client unavailable".to_string()),
                pr_merged: false,
                pr_closed: false,
            };
        }
    };

    // PR discovery if no URL known
    let pr_url = match &req.pr_url {
        Some(url) => Some(url.clone()),
        None => match client.get_pr_for_branch(&req.branch) {
            Ok(Some(pr_info)) => {
                tracing::info!(
                    worker = %req.worker_key,
                    pr_url = %pr_info.url,
                    "discovered PR for branch"
                );
                Some(pr_info.url)
            }
            Ok(None) => None,
            Err(e) => {
                tracing::debug!(worker = %req.worker_key, error = %e, "PR discovery failed");
                None
            }
        },
    };

    let Some(pr_url) = &pr_url else {
        return GitHubResponse {
            worker_key: req.worker_key.clone(),
            pr_url: None,
            pr_checks: vec![],
            pr_error: None,
            pr_merged: false,
            pr_closed: false,
        };
    };

    // Extract PR number
    let pr_number = match pr_url
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
    {
        Some(n) => n,
        None => {
            return GitHubResponse {
                worker_key: req.worker_key.clone(),
                pr_url: Some(pr_url.clone()),
                pr_checks: vec![],
                pr_error: Some("invalid PR URL".to_string()),
                pr_merged: false,
                pr_closed: false,
            };
        }
    };

    // Check PR state
    let pr_state = match client.get_pr_state(pr_number) {
        Ok(s) => s,
        Err(e) => {
            return GitHubResponse {
                worker_key: req.worker_key.clone(),
                pr_url: Some(pr_url.clone()),
                pr_checks: vec![],
                pr_error: Some(e.to_string()),
                pr_merged: false,
                pr_closed: false,
            };
        }
    };

    match pr_state {
        PrState::Merged => GitHubResponse {
            worker_key: req.worker_key.clone(),
            pr_url: Some(pr_url.clone()),
            pr_checks: vec![],
            pr_error: None,
            pr_merged: true,
            pr_closed: false,
        },
        PrState::Closed => GitHubResponse {
            worker_key: req.worker_key.clone(),
            pr_url: Some(pr_url.clone()),
            pr_checks: vec![],
            pr_error: None,
            pr_merged: false,
            pr_closed: true,
        },
        PrState::Open => {
            let checks: Vec<(&str, Result<github::PrCheck, _>)> = vec![
                ("ci", github::check_ci(&client, &req.branch)),
                ("conflicts", github::check_conflicts(&client, pr_number)),
                ("reviews", github::check_reviews(&client, pr_number)),
                ("commits", github::check_commits(&client, pr_number)),
            ];

            let pr_checks: Vec<(String, bool)> = checks
                .into_iter()
                .filter_map(|(name, result)| match result {
                    Ok(check) => Some((name.to_string(), check.nudge.is_some())),
                    Err(e) => {
                        tracing::debug!(check = name, error = %e, "PR check failed");
                        None
                    }
                })
                .collect();

            GitHubResponse {
                worker_key: req.worker_key.clone(),
                pr_url: Some(pr_url.clone()),
                pr_checks,
                pr_error: None,
                pr_merged: false,
                pr_closed: false,
            }
        }
    }
}

fn make_client(repo_name: &str, registry: &RepoRegistry) -> Option<GitHubClient> {
    registry
        .repos()
        .iter()
        .find(|e| {
            e.path
                .file_name()
                .map(|n| n.to_string_lossy() == repo_name)
                .unwrap_or(false)
        })
        .and_then(|entry| GitHubClient::from_repo_path(&entry.path).ok())
}
