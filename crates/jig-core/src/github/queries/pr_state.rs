use super::super::client::GitHubClient;
use super::super::error::Result;
use super::super::types::{PrState, PrStateInfo};

impl GitHubClient {
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
}
