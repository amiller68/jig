//! PR checks — query GitHub state and classify problems for nudging.

use url::Url;

use jig_core::git::conventional::CommitMessage;
use jig_core::github::error::Result;
use jig_core::github::{GitHub, PrState, ReviewState};

/// Aggregated PR report for a worker.
#[derive(Debug, Clone)]
pub struct PrReport {
    pub status: PrStatus,
    pub review_feedback_count: u32,
}

/// Overall PR status from the worker's perspective.
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

/// Per-worker PR health info collected during a tick.
#[derive(Debug, Clone, Default)]
pub struct PrHealth {
    pub pr_checks: PrChecks,
    pub pr_error: Option<String>,
    pub has_pr: bool,
}

/// Aggregate of per-category PR check results.
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
        if self.ci == Some(true) {
            out.push("ci");
        }
        if self.conflicts == Some(true) {
            out.push("conflicts");
        }
        if self.reviews == Some(true) {
            out.push("reviews");
        }
        if self.commits == Some(true) {
            out.push("commits");
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.ci.is_none()
            && self.conflicts.is_none()
            && self.reviews.is_none()
            && self.commits.is_none()
    }
}

/// Check CI status for a branch.
pub fn check_ci(client: &dyn GitHub, git_ref: &str) -> Result<bool> {
    let failures = client.get_failed_checks(git_ref)?;
    Ok(!failures.is_empty())
}

/// Check if a PR has merge conflicts.
pub fn check_conflicts(client: &dyn GitHub, pr_number: u64) -> Result<bool> {
    client.has_conflicts(pr_number)
}

/// Review check result — includes feedback counts alongside the problem flag.
pub struct ReviewResult {
    pub has_problem: bool,
    pub review_comment_count: u32,
    pub changes_requested_count: u32,
}

/// Check if a PR has unresolved review comments.
pub fn check_reviews(client: &dyn GitHub, pr_number: u64) -> Result<ReviewResult> {
    let reviews = client.get_reviews(pr_number)?;
    let inline = client.get_review_comments(pr_number)?;

    let changes_requested_count = reviews
        .iter()
        .filter(|r| r.state == ReviewState::ChangesRequested)
        .count() as u32;

    let review_comment_count = inline.len() as u32;

    if changes_requested_count == 0 && inline.is_empty() {
        return Ok(ReviewResult {
            has_problem: false,
            review_comment_count,
            changes_requested_count,
        });
    }

    // If the dev already pushed commits after the latest review feedback,
    // suppress the nudge — the ball is in the reviewer's court now.
    tracing::debug!(
        pr_number,
        changes_requested_count,
        review_comment_count,
        "check_reviews: has feedback, checking if dev pushed after"
    );
    if client.dev_pushed_after_reviews(pr_number) {
        return Ok(ReviewResult {
            has_problem: false,
            review_comment_count,
            changes_requested_count,
        });
    }

    Ok(ReviewResult {
        has_problem: true,
        review_comment_count,
        changes_requested_count,
    })
}

/// Check if PR commits follow conventional commit format.
pub fn check_commits(client: &dyn GitHub, pr_number: u64) -> Result<bool> {
    let commits = client.get_pr_commits(pr_number)?;
    let has_bad = commits
        .iter()
        .any(|c| CommitMessage::try_from(c.message.as_str()).is_err());
    Ok(has_bad)
}

/// Query GitHub for a worker's PR status, running all health checks if open.
pub fn check_pr(gh: &dyn GitHub, branch: &str, worker_key: &str) -> PrReport {
    let pr_url = match gh.get_pr_for_branch(branch) {
        Ok(Some(pr_info)) => match Url::parse(&pr_info.url) {
            Ok(url) => url,
            Err(_) => {
                return PrReport {
                    status: PrStatus::Error {
                        pr_url: None,
                        error: format!("invalid PR URL: {}", pr_info.url),
                    },
                    review_feedback_count: 0,
                };
            }
        },
        Ok(None) => {
            return PrReport {
                status: PrStatus::NoPr,
                review_feedback_count: 0,
            }
        }
        Err(e) => {
            tracing::debug!(worker = %worker_key, error = %e, "PR discovery failed");
            return PrReport {
                status: PrStatus::Error {
                    pr_url: None,
                    error: e.to_string(),
                },
                review_feedback_count: 0,
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
            return PrReport {
                status: PrStatus::Error {
                    pr_url: Some(pr_url),
                    error: "could not parse PR number from URL".to_string(),
                },
                review_feedback_count: 0,
            };
        }
    };

    let pr_state_info = match gh.get_pr_state(pr_number) {
        Ok(s) => s,
        Err(e) => {
            return PrReport {
                status: PrStatus::Error {
                    pr_url: Some(pr_url),
                    error: e.to_string(),
                },
                review_feedback_count: 0,
            };
        }
    };

    let status = match pr_state_info.state {
        PrState::Merged => PrStatus::Merged { pr_url },
        PrState::Closed => PrStatus::Closed { pr_url },
        PrState::Open => {
            let mut checks = PrChecks::default();
            let mut review_feedback_count: u32 = 0;

            match check_ci(gh, branch) {
                Ok(has_problem) => checks.ci = Some(has_problem),
                Err(e) => tracing::debug!(error = %e, "CI check failed"),
            }
            match check_conflicts(gh, pr_number) {
                Ok(has_problem) => checks.conflicts = Some(has_problem),
                Err(e) => tracing::debug!(error = %e, "conflicts check failed"),
            }
            match check_reviews(gh, pr_number) {
                Ok(r) => {
                    review_feedback_count =
                        r.review_comment_count + r.changes_requested_count;
                    checks.reviews = Some(r.has_problem);
                }
                Err(e) => tracing::debug!(error = %e, "reviews check failed"),
            }
            match check_commits(gh, pr_number) {
                Ok(has_problem) => checks.commits = Some(has_problem),
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

    let review_feedback_count = match &status {
        PrStatus::Open {
            review_feedback_count,
            ..
        } => *review_feedback_count,
        _ => 0,
    };

    PrReport {
        status,
        review_feedback_count,
    }
}

