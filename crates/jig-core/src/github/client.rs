//! GitHub client wrapping `gh` CLI.

use std::process::Command;

use crate::error::{Error, Result};

use super::types::{CheckRun, CheckStatus, PrInfo, PrState, ReviewComment, ReviewState};

/// GitHub API client using `gh` CLI.
///
/// Auth is delegated entirely to `gh` — it uses `GITHUB_TOKEN`,
/// `gh auth login`, or whatever the user has configured.
pub struct GitHubClient {
    /// Repository in `owner/repo` format.
    repo: String,
}

impl GitHubClient {
    /// Create a client for the given repository.
    pub fn new(repo: impl Into<String>) -> Self {
        Self { repo: repo.into() }
    }

    /// Detect the repository from the current git remote.
    pub fn from_remote() -> Result<Self> {
        let output = Command::new("gh")
            .args([
                "repo",
                "view",
                "--json",
                "nameWithOwner",
                "-q",
                ".nameWithOwner",
            ])
            .output()?;

        if !output.status.success() {
            return Err(Error::Custom(
                "Failed to detect GitHub repository. Is `gh` authenticated?".into(),
            ));
        }

        let repo = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if repo.is_empty() {
            return Err(Error::Custom("Could not determine repository name".into()));
        }

        Ok(Self { repo })
    }

    /// Get PR info for a branch.
    pub fn get_pr_for_branch(&self, branch: &str) -> Result<Option<PrInfo>> {
        let encoded_branch = urlencoding::encode(branch);
        let output = self.gh_api(&format!(
            "repos/{}/pulls?head={}:{}&state=open",
            self.repo,
            self.repo.split('/').next().unwrap_or(""),
            encoded_branch
        ))?;

        let prs: Vec<serde_json::Value> = serde_json::from_str(&output)?;
        let Some(pr) = prs.first() else {
            return Ok(None);
        };

        Ok(Some(PrInfo {
            number: pr["number"].as_u64().unwrap_or(0),
            title: pr["title"].as_str().unwrap_or("").to_string(),
            state: PrState::Open,
            mergeable: pr["mergeable_state"].as_str().map(|s| s.to_uppercase()),
            head_branch: branch.to_string(),
            url: pr["html_url"].as_str().unwrap_or("").to_string(),
        }))
    }

    /// Get check runs for a git ref (branch name or SHA).
    pub fn get_check_runs(&self, git_ref: &str) -> Result<Vec<CheckRun>> {
        let output = self.gh_api(&format!(
            "repos/{}/commits/{}/check-runs",
            self.repo, git_ref
        ))?;

        let parsed: serde_json::Value = serde_json::from_str(&output)?;
        let runs = parsed["check_runs"].as_array();

        let Some(runs) = runs else {
            return Ok(vec![]);
        };

        Ok(runs
            .iter()
            .map(|r| CheckRun {
                name: r["name"].as_str().unwrap_or("").to_string(),
                status: match r["status"].as_str() {
                    Some("completed") => CheckStatus::Completed,
                    Some("in_progress") => CheckStatus::InProgress,
                    _ => CheckStatus::Queued,
                },
                conclusion: r["conclusion"].as_str().map(|s| s.to_string()),
                details_url: r["details_url"].as_str().map(|s| s.to_string()),
            })
            .collect())
    }

    /// Get failed check runs for a ref.
    pub fn get_failed_checks(&self, git_ref: &str) -> Result<Vec<CheckRun>> {
        let all = self.get_check_runs(git_ref)?;
        Ok(all.into_iter().filter(|r| r.is_failure()).collect())
    }

    /// Get review comments on a PR.
    pub fn get_reviews(&self, pr_number: u64) -> Result<Vec<ReviewComment>> {
        let output = self.gh_api(&format!("repos/{}/pulls/{}/reviews", self.repo, pr_number))?;

        let reviews: Vec<serde_json::Value> = serde_json::from_str(&output)?;

        Ok(reviews
            .iter()
            .filter_map(|r| {
                let state = match r["state"].as_str()? {
                    "APPROVED" => ReviewState::Approved,
                    "CHANGES_REQUESTED" => ReviewState::ChangesRequested,
                    "COMMENTED" => ReviewState::Commented,
                    "DISMISSED" => ReviewState::Dismissed,
                    "PENDING" => ReviewState::Pending,
                    _ => return None,
                };

                Some(ReviewComment {
                    body: r["body"].as_str().unwrap_or("").to_string(),
                    path: None, // Top-level reviews don't have path
                    line: None,
                    state,
                    author: r["user"]["login"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    /// Get inline review comments on a PR.
    pub fn get_review_comments(&self, pr_number: u64) -> Result<Vec<ReviewComment>> {
        let output = self.gh_api(&format!("repos/{}/pulls/{}/comments", self.repo, pr_number))?;

        let comments: Vec<serde_json::Value> = serde_json::from_str(&output)?;

        Ok(comments
            .iter()
            .map(|c| ReviewComment {
                body: c["body"].as_str().unwrap_or("").to_string(),
                path: c["path"].as_str().map(|s| s.to_string()),
                line: c["line"].as_u64().or_else(|| c["original_line"].as_u64()),
                state: ReviewState::Commented,
                author: c["user"]["login"].as_str().unwrap_or("").to_string(),
            })
            .collect())
    }

    /// Check if a PR has merge conflicts.
    pub fn has_conflicts(&self, pr_number: u64) -> Result<bool> {
        let output = self.gh_api(&format!("repos/{}/pulls/{}", self.repo, pr_number))?;

        let pr: serde_json::Value = serde_json::from_str(&output)?;
        let mergeable_state = pr["mergeable_state"].as_str().unwrap_or("");
        Ok(mergeable_state == "dirty" || pr["mergeable"].as_bool() == Some(false))
    }

    /// Check if `gh` CLI is available and authenticated.
    pub fn is_available() -> bool {
        Command::new("gh")
            .args(["auth", "status"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Execute a `gh api` call and return the response body.
    fn gh_api(&self, endpoint: &str) -> Result<String> {
        let output = Command::new("gh")
            .args(["api", endpoint, "--cache", "60s"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Custom(format!("gh api failed: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_repo() {
        let client = GitHubClient::new("owner/repo");
        assert_eq!(client.repo, "owner/repo");
    }

    #[test]
    fn is_available_does_not_panic() {
        // Just verify it doesn't panic — may return true or false depending on env
        let _ = GitHubClient::is_available();
    }
}
