//! GitHub client wrapping `gh` CLI.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{Error, Result};

use super::types::{
    CheckRun, CheckStatus, PrCommit, PrInfo, PrState, PrStateInfo, ReviewComment, ReviewState,
};

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
            .stdin(Stdio::null())
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

    /// Detect the repository from a specific repo path (runs `gh` in that directory).
    pub fn from_repo_path(repo_path: &Path) -> Result<Self> {
        let output = Command::new("gh")
            .args([
                "repo",
                "view",
                "--json",
                "nameWithOwner",
                "-q",
                ".nameWithOwner",
            ])
            .current_dir(repo_path)
            .stdin(Stdio::null())
            .output()?;

        if !output.status.success() {
            return Err(Error::Custom(format!(
                "Failed to detect GitHub repository at {}. Is `gh` authenticated?",
                repo_path.display()
            )));
        }

        let repo = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if repo.is_empty() {
            return Err(Error::Custom(format!(
                "Could not determine repository name at {}",
                repo_path.display()
            )));
        }

        tracing::debug!(
            repo_path = %repo_path.display(),
            owner_repo = %repo,
            "created GitHub client from repo path"
        );

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
    ///
    /// Excludes `PENDING` reviews — those are in-progress drafts that the
    /// reviewer hasn't submitted yet. Including them would cause false
    /// nudges while a reviewer is still writing comments.
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
                    "PENDING" => return None, // Skip unsubmitted draft reviews
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

    /// Get inline review comments from **unresolved** threads on a PR.
    ///
    /// Uses the GraphQL API to fetch only unresolved review threads, so
    /// resolved conversations don't trigger review nudges.  Falls back to
    /// the REST endpoint (all comments, replies excluded) if GraphQL fails.
    pub fn get_review_comments(&self, pr_number: u64) -> Result<Vec<ReviewComment>> {
        // Try GraphQL first — it exposes thread resolution status.
        if let Ok(comments) = self.get_unresolved_review_comments_graphql(pr_number) {
            return Ok(comments);
        }

        // Fallback: REST API, filter out reply comments.
        let output = self.gh_api(&format!("repos/{}/pulls/{}/comments", self.repo, pr_number))?;

        let comments: Vec<serde_json::Value> = serde_json::from_str(&output)?;

        Ok(comments
            .iter()
            .filter(|c| c.get("in_reply_to_id").and_then(|v| v.as_u64()).is_none())
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

    /// Get commits on a PR.
    pub fn get_pr_commits(&self, pr_number: u64) -> Result<Vec<PrCommit>> {
        let output = self.gh_api(&format!("repos/{}/pulls/{}/commits", self.repo, pr_number))?;

        let commits: Vec<serde_json::Value> = serde_json::from_str(&output)?;

        Ok(commits
            .iter()
            .map(|c| PrCommit {
                sha: c["sha"].as_str().unwrap_or("").chars().take(7).collect(),
                message: c["commit"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string(),
            })
            .collect())
    }

    /// Get the current state of a PR (open, closed, or merged) and whether it's a draft.
    pub fn get_pr_state(&self, pr_number: u64) -> Result<PrStateInfo> {
        let output = self.gh_api(&format!("repos/{}/pulls/{}", self.repo, pr_number))?;

        let pr: serde_json::Value = serde_json::from_str(&output)?;
        let merged = pr["merged"].as_bool().unwrap_or(false);
        let state_str = pr["state"].as_str().unwrap_or("open");
        let is_draft = pr["draft"].as_bool().unwrap_or(false);

        let state = if merged {
            PrState::Merged
        } else if state_str == "closed" {
            PrState::Closed
        } else {
            PrState::Open
        };

        let head_sha = pr["head"]["sha"].as_str().map(|s| s.to_string());

        Ok(PrStateInfo {
            state,
            is_draft,
            head_sha,
        })
    }

    /// Check if `gh` CLI is available and authenticated.
    pub fn is_available() -> bool {
        Command::new("gh")
            .args(["auth", "status"])
            .stdin(Stdio::null())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Check whether the latest commit on a PR is newer than the latest review activity.
    ///
    /// Returns `true` if the developer has pushed commits after the most recent
    /// review or inline comment, meaning the feedback has likely been addressed
    /// and nudging would be premature (the ball is in the reviewer's court).
    ///
    /// Returns `false` (= should nudge) on any API error or if there are no commits.
    pub fn dev_pushed_after_reviews(&self, pr_number: u64) -> bool {
        // Get latest commit timestamp
        let commits_json = match self
            .gh_api(&format!("repos/{}/pulls/{}/commits", self.repo, pr_number))
        {
            Ok(j) => j,
            Err(e) => {
                tracing::debug!(pr_number, error = %e, "dev_pushed_after_reviews: commits API failed");
                return false;
            }
        };
        let commits: Vec<serde_json::Value> = match serde_json::from_str(&commits_json) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(pr_number, error = %e, "dev_pushed_after_reviews: commits parse failed");
                return false;
            }
        };
        let latest_commit_date = commits
            .last()
            .and_then(|c| c["commit"]["committer"]["date"].as_str())
            .unwrap_or("");

        if latest_commit_date.is_empty() {
            tracing::debug!(pr_number, "dev_pushed_after_reviews: no commit date found");
            return false;
        }

        // Get latest review timestamp (top-level reviews like CHANGES_REQUESTED)
        let reviews_json = match self
            .gh_api(&format!("repos/{}/pulls/{}/reviews", self.repo, pr_number))
        {
            Ok(j) => j,
            Err(e) => {
                tracing::debug!(pr_number, error = %e, "dev_pushed_after_reviews: reviews API failed");
                return false;
            }
        };
        let reviews: Vec<serde_json::Value> = match serde_json::from_str(&reviews_json) {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(pr_number, error = %e, "dev_pushed_after_reviews: reviews parse failed");
                return false;
            }
        };
        let latest_review_date = reviews
            .iter()
            .filter(|r| r["state"].as_str() != Some("PENDING"))
            .filter_map(|r| r["submitted_at"].as_str())
            .max()
            .unwrap_or("");

        // Get latest inline comment timestamp
        let comments_json = match self
            .gh_api(&format!("repos/{}/pulls/{}/comments", self.repo, pr_number))
        {
            Ok(j) => j,
            Err(e) => {
                tracing::debug!(pr_number, error = %e, "dev_pushed_after_reviews: comments API failed");
                return false;
            }
        };
        let comments: Vec<serde_json::Value> = match serde_json::from_str(&comments_json) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(pr_number, error = %e, "dev_pushed_after_reviews: comments parse failed");
                return false;
            }
        };
        let latest_comment_date = comments
            .iter()
            .filter_map(|c| c["created_at"].as_str())
            .max()
            .unwrap_or("");

        // Find the most recent review activity
        let latest_feedback = std::cmp::max(latest_review_date, latest_comment_date);

        if latest_feedback.is_empty() {
            tracing::debug!(
                pr_number,
                "dev_pushed_after_reviews: no review activity found"
            );
            return false;
        }

        let result = latest_commit_date > latest_feedback;
        tracing::info!(
            pr_number,
            latest_commit_date,
            latest_review_date,
            latest_comment_date,
            %latest_feedback,
            result,
            "dev_pushed_after_reviews"
        );
        result
    }

    /// Execute a `gh api` call and return the response body.
    pub(crate) fn gh_api(&self, endpoint: &str) -> Result<String> {
        let output = Command::new("gh")
            .args(["api", endpoint, "--cache", "60s"])
            .stdin(Stdio::null())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Custom(format!("gh api failed: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Execute a `gh api graphql` call and return the parsed JSON response.
    fn gh_graphql(&self, query: &str) -> Result<serde_json::Value> {
        let output = Command::new("gh")
            .args([
                "api",
                "graphql",
                "--cache",
                "60s",
                "-f",
                &format!("query={}", query),
            ])
            .stdin(Stdio::null())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Custom(format!("gh graphql failed: {}", stderr)));
        }

        let body = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(&body)?;
        Ok(parsed)
    }

    /// Fetch review comments from unresolved threads only (via GraphQL).
    fn get_unresolved_review_comments_graphql(&self, pr_number: u64) -> Result<Vec<ReviewComment>> {
        let (owner, name) = self
            .repo
            .split_once('/')
            .ok_or_else(|| Error::Custom("invalid repo format".to_string()))?;

        let query = format!(
            r#"{{
              repository(owner: "{owner}", name: "{name}") {{
                pullRequest(number: {pr_number}) {{
                  reviewThreads(first: 100) {{
                    nodes {{
                      isResolved
                      comments(first: 1) {{
                        nodes {{
                          body
                          path
                          line: originalLine
                          author {{ login }}
                        }}
                      }}
                    }}
                  }}
                }}
              }}
            }}"#,
            owner = owner,
            name = name,
            pr_number = pr_number,
        );

        let data = self.gh_graphql(&query)?;

        let threads = data["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"]
            .as_array()
            .ok_or_else(|| Error::Custom("unexpected graphql response shape".to_string()))?;

        let mut comments = Vec::new();
        for thread in threads {
            if thread["isResolved"].as_bool() == Some(true) {
                continue;
            }
            if let Some(first) = thread["comments"]["nodes"]
                .as_array()
                .and_then(|a| a.first())
            {
                comments.push(ReviewComment {
                    body: first["body"].as_str().unwrap_or("").to_string(),
                    path: first["path"].as_str().map(|s| s.to_string()),
                    line: first["line"].as_u64(),
                    state: ReviewState::Commented,
                    author: first["author"]["login"].as_str().unwrap_or("").to_string(),
                });
            }
        }

        Ok(comments)
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
