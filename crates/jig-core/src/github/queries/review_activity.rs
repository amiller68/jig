use super::super::client::GitHubClient;

impl GitHubClient {
    /// Check whether the latest commit on a PR is newer than the latest review activity.
    ///
    /// Returns `true` if the developer has pushed commits after the most recent
    /// review or inline comment, meaning the feedback has likely been addressed
    /// and nudging would be premature (the ball is in the reviewer's court).
    ///
    /// Returns `false` (= should nudge) on any API error or if there are no commits.
    pub fn dev_pushed_after_reviews(&self, pr_number: u64) -> bool {
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
}
