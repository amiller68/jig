//! Worker name derivation from issues.

/// Derive a worker name from an issue ID and optional branch name.
///
/// When a branch name is available (e.g. Linear's `branchName` field like
/// `feature/aut-4969-spawn-agent-thread-is-broken`), it is used as-is since
/// it is already a valid git branch name.
///
/// For file-based issues (no branch name), the ID is lowercased and used
/// directly — it already contains a descriptive slug.
pub fn derive_worker_name(issue_id: &str, branch_name: Option<&str>) -> String {
    match branch_name {
        Some(name) if !name.is_empty() => sanitize_worker_name(name),
        _ => issue_id.to_lowercase(),
    }
}

/// Sanitize a branch name for use as a git worktree/branch name.
///
/// Applies git ref naming rules: no leading dots, no `.lock` suffix,
/// no `..`, no ASCII control chars, no `\`, no spaces.
pub fn sanitize_worker_name(name: &str) -> String {
    let mut result: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_control() || c == '\\' || c == ' ' || c == '~' || c == '^' || c == ':' {
                '-'
            } else {
                c
            }
        })
        .collect();

    // Collapse consecutive dots (no "..")
    while result.contains("..") {
        result = result.replace("..", ".");
    }

    // Strip leading dots
    result = result.trim_start_matches('.').to_string();

    // Strip trailing ".lock"
    if result.ends_with(".lock") {
        result.truncate(result.len() - 5);
    }

    // Strip trailing dots and slashes
    result = result.trim_end_matches(&['.', '/'][..]).to_string();

    if result.is_empty() {
        "worker".to_string()
    } else {
        result
    }
}

/// Try to extract a Linear-style identifier (e.g. `AUT-5044`) from a string.
///
/// Handles:
/// - Direct identifiers: `AUT-5044` → `AUT-5044`
/// - Branch-format strings: `feature/aut-5044-refactor-foo` → `AUT-5044`
///
/// Returns `None` if no identifier pattern is found.
pub fn extract_linear_identifier(input: &str) -> Option<String> {
    // First, check if the input is already a direct identifier (TEAM-123)
    if is_linear_identifier(input) {
        return Some(input.to_uppercase());
    }

    // Try to extract from a branch-format string.
    // Split on '/' and look at each segment for a leading TEAM-NUMBER pattern.
    for segment in input.rsplit('/') {
        // Try to find a TEAM-NUMBER pattern at the start of the segment.
        // Pattern: one or more ASCII letters, then '-', then one or more digits.
        if let Some(id) = extract_leading_identifier(segment) {
            return Some(id.to_uppercase());
        }
    }

    None
}

/// Check if a string is a direct Linear identifier like `AUT-5044`.
fn is_linear_identifier(s: &str) -> bool {
    let Some((team, num)) = s.rsplit_once('-') else {
        return false;
    };
    // Team part must be all ASCII alphabetic
    if team.is_empty() || !team.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    // Number part must be all digits
    !num.is_empty() && num.chars().all(|c| c.is_ascii_digit())
}

/// Extract a leading `TEAM-NUMBER` pattern from a segment like `aut-5044-refactor-foo`.
fn extract_leading_identifier(segment: &str) -> Option<String> {
    let mut chars = segment.chars().peekable();

    // Collect the team part (letters)
    let mut team = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphabetic() {
            team.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if team.is_empty() {
        return None;
    }

    // Expect a dash
    if chars.next() != Some('-') {
        return None;
    }

    // Collect the number part (digits)
    let mut number = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            number.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if number.is_empty() {
        return None;
    }

    // The next char (if any) should be '-' or end-of-string to confirm this is a valid boundary
    if let Some(&c) = chars.peek() {
        if c != '-' {
            return None;
        }
    }

    Some(format!("{}-{}", team, number))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- derive_worker_name tests (moved from issue_actor) --

    #[test]
    fn derive_worker_name_linear_no_branch() {
        assert_eq!(derive_worker_name("ENG-123", None), "eng-123");
    }

    #[test]
    fn derive_worker_name_linear_with_branch() {
        assert_eq!(
            derive_worker_name(
                "AUT-4969",
                Some("feature/aut-4969-spawn-agent-thread-is-broken")
            ),
            "feature/aut-4969-spawn-agent-thread-is-broken"
        );
    }

    #[test]
    fn derive_worker_name_linear_empty_branch() {
        assert_eq!(derive_worker_name("ENG-123", Some("")), "eng-123");
    }

    #[test]
    fn derive_worker_name_preserves_category_prefix() {
        assert_eq!(
            derive_worker_name("features/my-feature", None),
            "features/my-feature"
        );
    }

    #[test]
    fn derive_worker_name_preserves_nested_prefix() {
        assert_eq!(
            derive_worker_name("features/global-attach", None),
            "features/global-attach"
        );
    }

    #[test]
    fn derive_worker_name_preserves_bugs_prefix() {
        assert_eq!(derive_worker_name("bugs/fix-foo", None), "bugs/fix-foo");
    }

    #[test]
    fn derive_worker_name_simple() {
        assert_eq!(derive_worker_name("fix-bug", None), "fix-bug");
    }

    // -- sanitize_worker_name tests (moved from issue_actor) --

    #[test]
    fn sanitize_worker_name_strips_leading_dot() {
        assert_eq!(sanitize_worker_name(".hidden"), "hidden");
    }

    #[test]
    fn sanitize_worker_name_strips_dot_lock() {
        assert_eq!(sanitize_worker_name("branch.lock"), "branch");
    }

    #[test]
    fn sanitize_worker_name_collapses_double_dots() {
        assert_eq!(sanitize_worker_name("a..b"), "a.b");
    }

    #[test]
    fn sanitize_worker_name_replaces_control_chars() {
        assert_eq!(sanitize_worker_name("a\tb"), "a-b");
    }

    #[test]
    fn sanitize_worker_name_replaces_spaces() {
        assert_eq!(sanitize_worker_name("a b"), "a-b");
    }

    #[test]
    fn sanitize_worker_name_empty_fallback() {
        assert_eq!(sanitize_worker_name("..."), "worker");
    }

    // -- extract_linear_identifier tests --

    #[test]
    fn extract_direct_identifier() {
        assert_eq!(
            extract_linear_identifier("AUT-5044"),
            Some("AUT-5044".into())
        );
    }

    #[test]
    fn extract_lowercase_identifier() {
        assert_eq!(
            extract_linear_identifier("aut-5044"),
            Some("AUT-5044".into())
        );
    }

    #[test]
    fn extract_from_branch_name() {
        assert_eq!(
            extract_linear_identifier("feature/aut-5044-refactor-something"),
            Some("AUT-5044".into())
        );
    }

    #[test]
    fn extract_from_simple_branch() {
        assert_eq!(
            extract_linear_identifier("aut-5044-my-feature"),
            Some("AUT-5044".into())
        );
    }

    #[test]
    fn extract_no_match() {
        assert_eq!(extract_linear_identifier("just-a-branch-name"), None);
    }

    #[test]
    fn extract_no_match_no_number() {
        assert_eq!(extract_linear_identifier("feature/my-branch"), None);
    }
}
