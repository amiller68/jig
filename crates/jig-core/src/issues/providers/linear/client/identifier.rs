/// Try to extract a Linear-style identifier (e.g. `AUT-5044`) from a string.
///
/// Handles:
/// - Direct identifiers: `AUT-5044` → `AUT-5044`
/// - Branch-format strings: `feature/aut-5044-refactor-foo` → `AUT-5044`
///
/// Returns `None` if no identifier pattern is found.
pub fn extract_linear_identifier(input: &str) -> Option<String> {
    if is_linear_identifier(input) {
        return Some(input.to_uppercase());
    }

    for segment in input.rsplit('/') {
        if let Some(id) = extract_leading_identifier(segment) {
            return Some(id.to_uppercase());
        }
    }

    None
}

/// Parse an identifier like "AUT-62" into (team_key, number).
///
/// Also accepts branch-format strings like `feature/aut-5044-refactor-foo`
/// by extracting the embedded Linear identifier first.
pub fn parse_identifier(identifier: &str) -> Option<(String, i64)> {
    let canonical = extract_linear_identifier(identifier)?;
    let (team, num) = canonical.rsplit_once('-')?;
    let n = num.parse::<i64>().ok()?;
    Some((team.to_string(), n))
}

fn is_linear_identifier(s: &str) -> bool {
    let Some((team, num)) = s.rsplit_once('-') else {
        return false;
    };
    if team.is_empty() || !team.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    !num.is_empty() && num.chars().all(|c| c.is_ascii_digit())
}

fn extract_leading_identifier(segment: &str) -> Option<String> {
    let mut chars = segment.chars().peekable();

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

    if chars.next() != Some('-') {
        return None;
    }

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

    #[test]
    fn parse_valid() {
        assert_eq!(parse_identifier("AUT-62"), Some(("AUT".into(), 62)));
    }

    #[test]
    fn parse_from_branch() {
        assert_eq!(
            parse_identifier("feature/aut-5044-refactor-foo"),
            Some(("AUT".into(), 5044))
        );
    }

    #[test]
    fn parse_invalid() {
        assert_eq!(parse_identifier("not-an-issue"), None);
    }
}
