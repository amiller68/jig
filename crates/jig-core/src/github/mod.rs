//! GitHub integration via `gh` CLI.
//!
//! Wraps `gh api` calls for PR status, CI checks, and review comments.
//! Auth is handled by `gh` (uses `GITHUB_TOKEN` or `gh auth login`).

mod client;
pub mod error;
mod queries;
mod types;

pub use client::GitHubClient;
pub use error::GitHubError;
pub use types::{
    CheckRun, CheckStatus, PrCommit, PrInfo, PrState, PrStateInfo, ReviewComment, ReviewState,
};

/// Trait abstracting GitHub API access for dependency injection.
pub trait GitHub: Send + Sync {
    fn get_pr_for_branch(&self, branch: &str) -> error::Result<Option<PrInfo>>;
    fn get_pr_state(&self, pr_number: u64) -> error::Result<PrStateInfo>;
    fn get_failed_checks(&self, git_ref: &str) -> error::Result<Vec<CheckRun>>;
    fn has_conflicts(&self, pr_number: u64) -> error::Result<bool>;
    fn get_reviews(&self, pr_number: u64) -> error::Result<Vec<ReviewComment>>;
    fn get_review_comments(&self, pr_number: u64) -> error::Result<Vec<ReviewComment>>;
    fn dev_pushed_after_reviews(&self, pr_number: u64) -> bool;
    fn get_pr_commits(&self, pr_number: u64) -> error::Result<Vec<PrCommit>>;
}

impl GitHub for GitHubClient {
    fn get_pr_for_branch(&self, branch: &str) -> error::Result<Option<PrInfo>> {
        self.get_pr_for_branch(branch)
    }

    fn get_pr_state(&self, pr_number: u64) -> error::Result<PrStateInfo> {
        self.get_pr_state(pr_number)
    }

    fn get_failed_checks(&self, git_ref: &str) -> error::Result<Vec<CheckRun>> {
        self.get_failed_checks(git_ref)
    }

    fn has_conflicts(&self, pr_number: u64) -> error::Result<bool> {
        self.has_conflicts(pr_number)
    }

    fn get_reviews(&self, pr_number: u64) -> error::Result<Vec<ReviewComment>> {
        self.get_reviews(pr_number)
    }

    fn get_review_comments(&self, pr_number: u64) -> error::Result<Vec<ReviewComment>> {
        self.get_review_comments(pr_number)
    }

    fn dev_pushed_after_reviews(&self, pr_number: u64) -> bool {
        self.dev_pushed_after_reviews(pr_number)
    }

    fn get_pr_commits(&self, pr_number: u64) -> error::Result<Vec<PrCommit>> {
        self.get_pr_commits(pr_number)
    }
}
