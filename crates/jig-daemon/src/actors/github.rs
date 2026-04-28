//! GitHub actor — runs PR discovery and lifecycle checks in a background thread.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use jig_core::config::registry::RepoRegistry;
use jig_core::github::{self, GitHubClient};

use crate::actors::Actor;

pub struct GitHubRequest {
    pub worker_key: String,
    pub repo_name: String,
    pub branch: String,
    pub pr_url: Option<String>,
    pub previous_is_draft: bool,
}

#[derive(Debug, Clone)]
pub struct GitHubResponse {
    pub worker_key: String,
    pub pr_url: Option<String>,
    pub pr_checks: Vec<(String, bool)>,
    pub pr_error: Option<String>,
    pub pr_merged: bool,
    pub pr_closed: bool,
    pub is_draft: bool,
    pub review_feedback_count: Option<u32>,
}

const GITHUB_POLL_INTERVAL: Duration = Duration::from_secs(60);

pub struct GitHubActor {
    tx: flume::Sender<GitHubRequest>,
    rx: flume::Receiver<GitHubResponse>,
    cache: HashMap<String, GitHubResponse>,
    last_requested: HashMap<String, Instant>,
}

impl Actor for GitHubActor {
    type Request = GitHubRequest;
    type Response = GitHubResponse;

    const NAME: &'static str = "jig-github";
    const QUEUE_SIZE: usize = 16;

    fn handle(req: GitHubRequest) -> GitHubResponse {
        process_request(&req)
    }

    fn send(&mut self, req: GitHubRequest) -> bool {
        if let Some(last) = self.last_requested.get(&req.worker_key) {
            if last.elapsed() < GITHUB_POLL_INTERVAL {
                return false;
            }
        }
        let key = req.worker_key.clone();
        if self.tx.try_send(req).is_ok() {
            self.last_requested.insert(key, Instant::now());
            true
        } else {
            false
        }
    }

    fn drain(&mut self) -> Vec<GitHubResponse> {
        let mut results = Vec::new();
        while let Ok(resp) = self.rx.try_recv() {
            self.cache.insert(resp.worker_key.clone(), resp.clone());
            results.push(resp);
        }
        results
    }

    fn from_channels(
        tx: flume::Sender<GitHubRequest>,
        rx: flume::Receiver<GitHubResponse>,
    ) -> Self {
        Self {
            tx,
            rx,
            cache: HashMap::new(),
            last_requested: HashMap::new(),
        }
    }
}

impl GitHubActor {
    pub fn get_cached(&self, worker_key: &str) -> Option<&GitHubResponse> {
        self.cache.get(worker_key)
    }

    pub fn previous_is_draft(&self, worker_key: &str) -> bool {
        self.cache
            .get(worker_key)
            .map(|r| r.is_draft)
            .unwrap_or(false)
    }
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
