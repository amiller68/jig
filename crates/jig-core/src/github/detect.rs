//! Detection functions — check GitHub state and classify nudge types.

use crate::error::Result;
use crate::nudge::NudgeType;

use super::client::GitHubClient;

/// Result of checking a PR's GitHub state.
#[derive(Debug, Clone)]
pub struct PrCheck {
    /// Nudge type if action is needed.
    pub nudge: Option<NudgeType>,
    /// Human-readable details.
    pub details: Vec<String>,
}

/// Check CI status for a branch and return a nudge if checks are failing.
pub fn check_ci(client: &GitHubClient, git_ref: &str) -> Result<PrCheck> {
    let failures = client.get_failed_checks(git_ref)?;

    if failures.is_empty() {
        return Ok(PrCheck {
            nudge: None,
            details: vec![],
        });
    }

    let details: Vec<String> = failures
        .iter()
        .map(|f| {
            let url = f
                .details_url
                .as_deref()
                .map(|u| format!(" ({})", u))
                .unwrap_or_default();
            format!(
                "{}: {}{}",
                f.name,
                f.conclusion.as_deref().unwrap_or("failed"),
                url
            )
        })
        .collect();

    Ok(PrCheck {
        nudge: Some(NudgeType::Ci),
        details,
    })
}

/// Check if a PR has merge conflicts.
pub fn check_conflicts(client: &GitHubClient, pr_number: u64) -> Result<PrCheck> {
    let has_conflicts = client.has_conflicts(pr_number)?;

    if !has_conflicts {
        return Ok(PrCheck {
            nudge: None,
            details: vec![],
        });
    }

    Ok(PrCheck {
        nudge: Some(NudgeType::Conflict),
        details: vec!["PR has merge conflicts".to_string()],
    })
}

/// Check if a PR has unresolved review comments.
pub fn check_reviews(client: &GitHubClient, pr_number: u64) -> Result<PrCheck> {
    let reviews = client.get_reviews(pr_number)?;
    let inline = client.get_review_comments(pr_number)?;

    let has_changes_requested = reviews
        .iter()
        .any(|r| r.state == super::types::ReviewState::ChangesRequested);

    let unresolved_comments: Vec<String> = inline
        .iter()
        .map(|c| {
            let location = match (&c.path, c.line) {
                (Some(path), Some(line)) => format!("{}:{}", path, line),
                (Some(path), None) => path.clone(),
                _ => "general".to_string(),
            };
            format!("{} ({}): {}", c.author, location, truncate(&c.body, 100))
        })
        .collect();

    if !has_changes_requested && unresolved_comments.is_empty() {
        return Ok(PrCheck {
            nudge: None,
            details: vec![],
        });
    }

    let mut details = vec![];
    if has_changes_requested {
        details.push("Changes requested by reviewer".to_string());
    }
    details.extend(unresolved_comments);

    Ok(PrCheck {
        nudge: Some(NudgeType::Review),
        details,
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let long = "a".repeat(200);
        let result = truncate(&long, 100);
        assert_eq!(result.len(), 103); // 100 + "..."
        assert!(result.ends_with("..."));
    }
}
