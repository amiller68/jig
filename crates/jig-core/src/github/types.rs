//! GitHub API types.

use serde::{Deserialize, Serialize};

/// Pull request info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrInfo {
    pub number: u64,
    pub title: String,
    pub state: PrState,
    pub mergeable: Option<String>,
    pub head_branch: String,
    pub url: String,
}

/// Pull request state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

/// Result of checking a PR's current state.
#[derive(Debug, Clone)]
pub struct PrStateInfo {
    pub state: PrState,
    pub is_draft: bool,
    /// HEAD commit SHA of the PR branch.
    pub head_sha: Option<String>,
}

/// CI check run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRun {
    pub name: String,
    pub status: CheckStatus,
    pub conclusion: Option<String>,
    pub details_url: Option<String>,
}

/// CI check status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Queued,
    InProgress,
    Completed,
}

impl CheckRun {
    /// Whether this check run failed.
    pub fn is_failure(&self) -> bool {
        self.status == CheckStatus::Completed
            && matches!(
                self.conclusion.as_deref(),
                Some("failure") | Some("timed_out") | Some("cancelled")
            )
    }
}

/// Review comment on a PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub state: ReviewState,
    pub author: String,
    /// Short commit SHA this review/comment was made against.
    #[serde(default)]
    pub commit_id: Option<String>,
    /// ISO 8601 timestamp when the review/comment was submitted.
    #[serde(default)]
    pub submitted_at: Option<String>,
}

/// Review state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

/// A commit on a PR branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrCommit {
    pub sha: String,
    pub message: String,
}

/// Aggregated PR feedback: reviews + inline comments with commit context.
#[derive(Debug, Clone)]
pub struct PrFeedback {
    pub pr_number: u64,
    pub pr_title: String,
    pub pr_state: PrState,
    pub is_draft: bool,
    pub head_sha: Option<String>,
    /// Top-level reviews (approvals, changes_requested, etc.)
    pub reviews: Vec<ReviewComment>,
    /// Unresolved inline comments.
    pub inline_comments: Vec<ReviewComment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_run_failure_detection() {
        let pass = CheckRun {
            name: "tests".into(),
            status: CheckStatus::Completed,
            conclusion: Some("success".into()),
            details_url: None,
        };
        assert!(!pass.is_failure());

        let fail = CheckRun {
            name: "lint".into(),
            status: CheckStatus::Completed,
            conclusion: Some("failure".into()),
            details_url: None,
        };
        assert!(fail.is_failure());

        let running = CheckRun {
            name: "build".into(),
            status: CheckStatus::InProgress,
            conclusion: None,
            details_url: None,
        };
        assert!(!running.is_failure());
    }

    #[test]
    fn pr_state_serde() {
        let json = r#""OPEN""#;
        let state: PrState = serde_json::from_str(json).unwrap();
        assert_eq!(state, PrState::Open);
    }

    #[test]
    fn review_state_serde() {
        let json = r#""CHANGES_REQUESTED""#;
        let state: ReviewState = serde_json::from_str(json).unwrap();
        assert_eq!(state, ReviewState::ChangesRequested);
    }
}
