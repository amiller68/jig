use super::super::client::GitHubClient;
use super::super::error::Result;
use super::super::types::{CheckRun, CheckStatus};

impl GitHubClient {
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
}
