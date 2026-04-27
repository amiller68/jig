//! Health checks — check GitHub/PR state and classify nudge types.

use std::sync::LazyLock;

use regex::Regex;

use super::client::GitHubClient;
use super::error::Result;

/// Default conventional commit pattern (compiled once).
static CONVENTIONAL_COMMIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(feat|fix|docs|style|refactor|perf|test|chore|ci|build|revert)(\(.+\))?!?: .+")
        .expect("valid regex")
});

/// Result of checking a PR's GitHub state.
#[derive(Debug, Clone)]
pub struct PrCheck {
    pub has_problem: bool,
    pub details: Vec<String>,
    pub review_comment_count: Option<u32>,
    pub changes_requested_count: Option<u32>,
}

/// Check CI status for a branch and return a nudge if checks are failing.
pub fn check_ci(client: &GitHubClient, git_ref: &str) -> Result<PrCheck> {
    let failures = client.get_failed_checks(git_ref)?;

    if failures.is_empty() {
        return Ok(PrCheck {
            has_problem: false,
            details: vec![],
            review_comment_count: None,
            changes_requested_count: None,
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
        has_problem: true,
        details,
        review_comment_count: None,
        changes_requested_count: None,
    })
}

/// Check if a PR has merge conflicts.
pub fn check_conflicts(client: &GitHubClient, pr_number: u64) -> Result<PrCheck> {
    let has_conflicts = client.has_conflicts(pr_number)?;

    if !has_conflicts {
        return Ok(PrCheck {
            has_problem: false,
            details: vec![],
            review_comment_count: None,
            changes_requested_count: None,
        });
    }

    Ok(PrCheck {
        has_problem: true,
        details: vec!["PR has merge conflicts".to_string()],
        review_comment_count: None,
        changes_requested_count: None,
    })
}

/// Check if a PR has unresolved review comments.
pub fn check_reviews(client: &GitHubClient, pr_number: u64) -> Result<PrCheck> {
    let reviews = client.get_reviews(pr_number)?;
    let inline = client.get_review_comments(pr_number)?;

    let changes_requested_count = reviews
        .iter()
        .filter(|r| r.state == super::types::ReviewState::ChangesRequested)
        .count() as u32;

    let has_changes_requested = changes_requested_count > 0;
    let review_comment_count = inline.len() as u32;

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
            has_problem: false,
            details: vec![],
            review_comment_count: Some(review_comment_count),
            changes_requested_count: Some(changes_requested_count),
        });
    }

    // If the dev already pushed commits after the latest review feedback,
    // suppress the nudge — the ball is in the reviewer's court now.
    tracing::debug!(
        pr_number,
        has_changes_requested,
        review_comment_count,
        "check_reviews: has feedback, checking if dev pushed after"
    );
    if client.dev_pushed_after_reviews(pr_number) {
        return Ok(PrCheck {
            has_problem: false,
            details: vec!["Dev pushed after latest review feedback".to_string()],
            review_comment_count: Some(review_comment_count),
            changes_requested_count: Some(changes_requested_count),
        });
    }

    let mut details = vec![];
    if has_changes_requested {
        details.push("Changes requested by reviewer".to_string());
    }
    details.extend(unresolved_comments);

    Ok(PrCheck {
        has_problem: true,
        details,
        review_comment_count: Some(review_comment_count),
        changes_requested_count: Some(changes_requested_count),
    })
}

/// Check if PR commits follow conventional commit format.
pub fn check_commits(client: &GitHubClient, pr_number: u64) -> Result<PrCheck> {
    let commits = client.get_pr_commits(pr_number)?;
    let re = &*CONVENTIONAL_COMMIT_RE;

    let bad: Vec<String> = commits
        .iter()
        .filter(|c| !re.is_match(&c.message))
        .map(|c| format!("{}: {}", c.sha, c.message))
        .collect();

    if bad.is_empty() {
        return Ok(PrCheck {
            has_problem: false,
            details: vec![],
            review_comment_count: None,
            changes_requested_count: None,
        });
    }

    Ok(PrCheck {
        has_problem: true,
        details: bad,
        review_comment_count: None,
        changes_requested_count: None,
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let end = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conventional_commit_regex_valid() {
        let re = &*CONVENTIONAL_COMMIT_RE;
        assert!(re.is_match("feat: add login"));
        assert!(re.is_match("fix(auth): handle expired tokens"));
        assert!(re.is_match("feat!: breaking change"));
        assert!(re.is_match("chore(deps): bump serde"));
        assert!(re.is_match("ci: update workflow"));
        assert!(re.is_match("build: update Cargo.toml"));
        assert!(re.is_match("revert: undo last commit"));
    }

    #[test]
    fn conventional_commit_regex_invalid() {
        let re = &*CONVENTIONAL_COMMIT_RE;
        assert!(!re.is_match("Update README"));
        assert!(!re.is_match("WIP"));
        assert!(!re.is_match("fix bug"));
        assert!(!re.is_match("Merge branch 'main' into feature"));
        assert!(!re.is_match(""));
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let long = "a".repeat(200);
        let result = truncate(&long, 100);
        assert!(result.ends_with("..."));
        assert_eq!(result.chars().count(), 103); // 100 + "..."
    }

    #[test]
    fn truncate_multibyte_utf8() {
        // Each emoji is 4 bytes — slicing at byte boundaries would panic
        let emojis = "🎉🎊🎈🎁🎂🎃🎄🎅🎆🎇";
        let result = truncate(emojis, 5);
        assert_eq!(result, "🎉🎊🎈🎁🎂...");
    }
}
