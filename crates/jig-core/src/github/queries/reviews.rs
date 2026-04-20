use super::super::client::GitHubClient;
use super::super::error::{GitHubError, Result};
use super::super::types::{ReviewComment, ReviewState};

impl GitHubClient {
    /// Get review comments on a PR.
    ///
    /// Excludes `PENDING` reviews — those are in-progress drafts that the
    /// reviewer hasn't submitted yet.
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
                    "PENDING" => return None,
                    _ => return None,
                };

                Some(ReviewComment {
                    body: r["body"].as_str().unwrap_or("").to_string(),
                    path: None,
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
    /// resolved conversations don't trigger review nudges. Falls back to
    /// the REST endpoint (all comments, replies excluded) if GraphQL fails.
    pub fn get_review_comments(&self, pr_number: u64) -> Result<Vec<ReviewComment>> {
        if let Ok(comments) = self.get_unresolved_review_comments_graphql(pr_number) {
            return Ok(comments);
        }

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

    /// Fetch review comments from unresolved threads only (via GraphQL).
    fn get_unresolved_review_comments_graphql(&self, pr_number: u64) -> Result<Vec<ReviewComment>> {
        let (owner, name) = self
            .repo
            .split_once('/')
            .ok_or_else(|| GitHubError::Other("invalid repo format".to_string()))?;

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
        );

        let data = self.gh_graphql(&query)?;

        let threads = data["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"]
            .as_array()
            .ok_or_else(|| GitHubError::Other("unexpected graphql response shape".to_string()))?;

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
