use super::super::client::GitHubClient;
use super::super::error::Result;
use super::super::types::{PrInfo, PrState};

impl GitHubClient {
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
}
