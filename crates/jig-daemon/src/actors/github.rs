//! GitHub actor — runs PR discovery and lifecycle checks in a background thread.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use url::Url;

use jig_core::github::{self, GitHubClient, PrState};
use jig_core::worker::Worker;

use crate::actors::Actor;

pub struct GitHubRequest {
    pub worker: Worker,
}

#[derive(Debug, Clone)]
pub struct GitHubResponse {
    pub worker: Worker,
    pub status: PrStatus,
}

#[derive(Debug, Clone)]
pub enum PrStatus {
    NoPr,
    Error {
        pr_url: Option<Url>,
        error: String,
    },
    Merged {
        pr_url: Url,
    },
    Closed {
        pr_url: Url,
    },
    Open {
        pr_url: Url,
        is_draft: bool,
        checks: PrChecks,
        review_feedback_count: u32,
    },
}

#[derive(Debug, Clone, Default)]
pub struct PrChecks {
    pub ci: Option<bool>,
    pub conflicts: Option<bool>,
    pub reviews: Option<bool>,
    pub commits: Option<bool>,
}

impl PrChecks {
    pub fn problems(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.ci == Some(true) { out.push("ci"); }
        if self.conflicts == Some(true) { out.push("conflicts"); }
        if self.reviews == Some(true) { out.push("reviews"); }
        if self.commits == Some(true) { out.push("commits"); }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.ci.is_none() && self.conflicts.is_none()
            && self.reviews.is_none() && self.commits.is_none()
    }
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
        process_request(req)
    }

    fn send(&mut self, req: GitHubRequest) -> bool {
        let key = req.worker.worker_key();
        if let Some(last) = self.last_requested.get(&key) {
            if last.elapsed() < GITHUB_POLL_INTERVAL {
                return false;
            }
        }
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
            self.cache.insert(resp.worker.worker_key(), resp.clone());
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
}

fn process_request(req: GitHubRequest) -> GitHubResponse {
    let worker_key = req.worker.worker_key();
    let branch = req.worker.branch().to_string();

    let client = match GitHubClient::from_repo_path(req.worker.path()) {
        Ok(c) => c,
        Err(_) => {
            return GitHubResponse {
                worker: req.worker,
                status: PrStatus::Error {
                    pr_url: None,
                    error: "GitHub client unavailable".to_string(),
                },
            };
        }
    };

    let pr_url = match client.get_pr_for_branch(&branch) {
        Ok(Some(pr_info)) => {
            tracing::info!(
                worker = %worker_key,
                pr_url = %pr_info.url,
                "discovered PR for branch"
            );
            match Url::parse(&pr_info.url) {
                Ok(url) => url,
                Err(_) => {
                    return GitHubResponse {
                        worker: req.worker,
                        status: PrStatus::Error {
                            pr_url: None,
                            error: format!("invalid PR URL: {}", pr_info.url),
                        },
                    };
                }
            }
        }
        Ok(None) => {
            return GitHubResponse {
                worker: req.worker,
                status: PrStatus::NoPr,
            };
        }
        Err(e) => {
            tracing::debug!(worker = %worker_key, error = %e, "PR discovery failed");
            return GitHubResponse {
                worker: req.worker,
                status: PrStatus::Error {
                    pr_url: None,
                    error: e.to_string(),
                },
            };
        }
    };

    let pr_number = match pr_url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .and_then(|s| s.parse::<u64>().ok())
    {
        Some(n) => n,
        None => {
            return GitHubResponse {
                worker: req.worker,
                status: PrStatus::Error {
                    pr_url: Some(pr_url),
                    error: "could not parse PR number from URL".to_string(),
                },
            };
        }
    };

    let pr_state_info = match client.get_pr_state(pr_number) {
        Ok(s) => s,
        Err(e) => {
            return GitHubResponse {
                worker: req.worker,
                status: PrStatus::Error {
                    pr_url: Some(pr_url),
                    error: e.to_string(),
                },
            };
        }
    };

    let status = match pr_state_info.state {
        PrState::Merged => PrStatus::Merged { pr_url },
        PrState::Closed => PrStatus::Closed { pr_url },
        PrState::Open => {
            let mut checks = PrChecks::default();
            let mut review_feedback_count: u32 = 0;

            match github::check_ci(&client, &branch) {
                Ok(c) => checks.ci = Some(c.has_problem),
                Err(e) => tracing::debug!(error = %e, "CI check failed"),
            }
            match github::check_conflicts(&client, pr_number) {
                Ok(c) => checks.conflicts = Some(c.has_problem),
                Err(e) => tracing::debug!(error = %e, "conflicts check failed"),
            }
            match github::check_reviews(&client, pr_number) {
                Ok(c) => {
                    review_feedback_count = c.review_comment_count.unwrap_or(0)
                        + c.changes_requested_count.unwrap_or(0);
                    checks.reviews = Some(c.has_problem);
                }
                Err(e) => tracing::debug!(error = %e, "reviews check failed"),
            }
            match github::check_commits(&client, pr_number) {
                Ok(c) => checks.commits = Some(c.has_problem),
                Err(e) => tracing::debug!(error = %e, "commits check failed"),
            }

            PrStatus::Open {
                pr_url,
                is_draft: pr_state_info.is_draft,
                checks,
                review_feedback_count,
            }
        }
    };

    GitHubResponse {
        worker: req.worker,
        status,
    }
}
