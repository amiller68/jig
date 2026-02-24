//! GitHub integration via `gh` CLI.
//!
//! Wraps `gh api` calls for PR status, CI checks, and review comments.
//! Auth is handled by `gh` (uses `GITHUB_TOKEN` or `gh auth login`).

mod client;
mod detect;
mod types;

pub use client::GitHubClient;
pub use detect::{check_ci, check_commits, check_conflicts, check_reviews, PrCheck};
pub use types::{CheckRun, CheckStatus, PrCommit, PrInfo, PrState, ReviewComment, ReviewState};
