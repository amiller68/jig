use super::super::client::GitHubClient;
use super::super::error::Result;

impl GitHubClient {
    /// Check if a PR has merge conflicts.
    pub fn has_conflicts(&self, pr_number: u64) -> Result<bool> {
        let output = self.gh_api(&format!("repos/{}/pulls/{}", self.repo, pr_number))?;

        let pr: serde_json::Value = serde_json::from_str(&output)?;
        let mergeable_state = pr["mergeable_state"].as_str().unwrap_or("");
        Ok(mergeable_state == "dirty" || pr["mergeable"].as_bool() == Some(false))
    }
}
