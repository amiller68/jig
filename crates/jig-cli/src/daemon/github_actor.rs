//! GitHub actor — runs PR discovery and lifecycle checks in a background thread.

use jig_core::config::registry::RepoRegistry;
use jig_core::github::{self, GitHubClient};

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
                is_draft: req.previous_is_draft,
                review_feedback_count: None,
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
            is_draft: false,
            review_feedback_count: None,
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
                is_draft: req.previous_is_draft,
                review_feedback_count: None,
            };
        }
    };

    // Check PR state
    let pr_state_info = match client.get_pr_state(pr_number) {
        Ok(s) => s,
        Err(e) => {
            return GitHubResponse {
                worker_key: req.worker_key.clone(),
                pr_url: Some(pr_url.clone()),
                pr_checks: vec![],
                pr_error: Some(e.to_string()),
                pr_merged: false,
                pr_closed: false,
                is_draft: req.previous_is_draft,
                review_feedback_count: None,
            };
        }
    };

    match pr_state_info.state {
        github::PrState::Merged => GitHubResponse {
            worker_key: req.worker_key.clone(),
            pr_url: Some(pr_url.clone()),
            pr_checks: vec![],
            pr_error: None,
            pr_merged: true,
            pr_closed: false,
            is_draft: false,
            review_feedback_count: None,
        },
        github::PrState::Closed => GitHubResponse {
            worker_key: req.worker_key.clone(),
            pr_url: Some(pr_url.clone()),
            pr_checks: vec![],
            pr_error: None,
            pr_merged: false,
            pr_closed: true,
            is_draft: false,
            review_feedback_count: None,
        },
        github::PrState::Open => {
            let checks: Vec<(&str, Result<github::PrCheck, _>)> = vec![
                ("ci", github::check_ci(&client, &req.branch)),
                ("conflicts", github::check_conflicts(&client, pr_number)),
                ("reviews", github::check_reviews(&client, pr_number)),
                ("commits", github::check_commits(&client, pr_number)),
            ];

            let mut pr_checks: Vec<(String, bool)> = Vec::new();
            let mut review_feedback_count: Option<u32> = None;

            for (name, result) in checks {
                match result {
                    Ok(check) => {
                        // Extract review feedback count from the review check
                        if name == "reviews" {
                            let comments = check.review_comment_count.unwrap_or(0);
                            let changes_req = check.changes_requested_count.unwrap_or(0);
                            review_feedback_count = Some(comments + changes_req);
                        }
                        pr_checks.push((name.to_string(), check.has_problem));
                    }
                    Err(e) => {
                        tracing::debug!(check = name, error = %e, "PR check failed");
                    }
                }
            }

            GitHubResponse {
                worker_key: req.worker_key.clone(),
                pr_url: Some(pr_url.clone()),
                pr_checks,
                pr_error: None,
                pr_merged: false,
                pr_closed: false,
                is_draft: pr_state_info.is_draft,
                review_feedback_count,
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
