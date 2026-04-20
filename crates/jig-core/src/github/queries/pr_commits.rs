use super::super::client::GitHubClient;
use super::super::error::Result;
use super::super::types::PrCommit;

impl GitHubClient {
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
}
