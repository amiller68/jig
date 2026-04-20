//! GitHub client wrapping `gh` CLI.

use std::path::Path;
use std::process::{Command, Stdio};

use super::error::{GitHubError, Result};

/// GitHub API client using `gh` CLI.
///
/// Auth is delegated entirely to `gh` — it uses `GITHUB_TOKEN`,
/// `gh auth login`, or whatever the user has configured.
pub struct GitHubClient {
    /// Repository in `owner/repo` format.
    pub(crate) repo: String,
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
            return Err(GitHubError::Cli(
                "Failed to detect GitHub repository. Is `gh` authenticated?".into(),
            ));
        }

        let repo = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if repo.is_empty() {
            return Err(GitHubError::Other(
                "Could not determine repository name".into(),
            ));
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
            return Err(GitHubError::Cli(format!(
                "Failed to detect GitHub repository at {}. Is `gh` authenticated?",
                repo_path.display()
            )));
        }

        let repo = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if repo.is_empty() {
            return Err(GitHubError::Other(format!(
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

    /// Check if `gh` CLI is available and authenticated.
    pub fn is_healthy() -> bool {
        Command::new("gh")
            .args(["auth", "status"])
            .stdin(Stdio::null())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Execute a `gh api` call and return the response body.
    pub(crate) fn gh_api(&self, endpoint: &str) -> Result<String> {
        let output = Command::new("gh")
            .args(["api", endpoint, "--cache", "60s"])
            .stdin(Stdio::null())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitHubError::Cli(format!("gh api failed: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Execute a `gh api graphql` call and return the parsed JSON response.
    pub(crate) fn gh_graphql(&self, query: &str) -> Result<serde_json::Value> {
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
            return Err(GitHubError::Cli(format!("gh graphql failed: {}", stderr)));
        }

        let body = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(&body)?;
        Ok(parsed)
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
    fn is_healthy_does_not_panic() {
        let _ = GitHubClient::is_healthy();
    }
}
