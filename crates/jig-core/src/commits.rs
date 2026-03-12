//! Conventional commit message parsing and validation.

use std::fmt;

use crate::error::{Error, Result};

/// A parsed conventional commit message.
#[derive(Debug, Clone, PartialEq)]
pub struct CommitMessage {
    pub commit_type: String,
    pub scope: Option<String>,
    pub breaking: bool,
    pub description: String,
    pub body: Option<String>,
    pub footers: Vec<Footer>,
}

/// A footer token-value pair from a conventional commit.
#[derive(Debug, Clone, PartialEq)]
pub struct Footer {
    pub token: String,
    pub value: String,
}

/// Parse a conventional commit message string into a `CommitMessage`.
///
/// Format: `<type>[(<scope>)][!]: <description>[\n\n<body>][\n\n<footer>*]`
pub fn parse(input: &str) -> Result<CommitMessage> {
    let input = input.trim();
    if input.is_empty() {
        return Err(Error::Custom("empty commit message".into()));
    }

    // Split header from body/footers at the first blank line
    let (header, rest) = match input.find("\n\n") {
        Some(pos) => (&input[..pos], Some(input[pos + 2..].trim())),
        None => (input, None),
    };

    // Header must be on a single line
    let header = header.lines().next().unwrap_or(header);

    // Find the ": " separator
    let colon_pos = header
        .find(": ")
        .ok_or_else(|| Error::Custom(format!("missing ': ' separator in '{}'", header)))?;

    let prefix = &header[..colon_pos];
    let description = header[colon_pos + 2..].to_string();

    if description.is_empty() {
        return Err(Error::Custom("empty description".into()));
    }

    // Parse prefix: type[(scope)][!]
    let (commit_type, scope, breaking_mark) = parse_prefix(prefix)?;

    // Parse body and footers
    let (body, footers) = match rest {
        Some(text) if !text.is_empty() => parse_body_and_footers(text),
        _ => (None, vec![]),
    };

    let breaking = breaking_mark
        || footers
            .iter()
            .any(|f| f.token == "BREAKING CHANGE" || f.token == "BREAKING-CHANGE");

    Ok(CommitMessage {
        commit_type,
        scope,
        breaking,
        description,
        body,
        footers,
    })
}

/// Parse the prefix portion: `type[(scope)][!]`
fn parse_prefix(prefix: &str) -> Result<(String, Option<String>, bool)> {
    let prefix = prefix.trim();

    // Check for breaking `!` at the end
    let (prefix, breaking) = if let Some(stripped) = prefix.strip_suffix('!') {
        (stripped, true)
    } else {
        (prefix, false)
    };

    // Check for scope in parentheses
    if let Some(paren_start) = prefix.find('(') {
        let commit_type = &prefix[..paren_start];
        let rest = &prefix[paren_start + 1..];

        let paren_end = rest
            .find(')')
            .ok_or_else(|| Error::Custom("unclosed scope parenthesis".into()))?;

        if paren_end + 1 != rest.len() {
            return Err(Error::Custom(format!(
                "unexpected characters after scope in '{}'",
                prefix
            )));
        }

        let scope = &rest[..paren_end];
        validate_type(commit_type)?;

        Ok((commit_type.to_string(), Some(scope.to_string()), breaking))
    } else {
        validate_type(prefix)?;
        Ok((prefix.to_string(), None, breaking))
    }
}

/// Validate that a type string is non-empty and alphanumeric.
fn validate_type(t: &str) -> Result<()> {
    if t.is_empty() {
        return Err(Error::Custom("empty commit type".into()));
    }
    if !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(Error::Custom(format!(
            "commit type '{}' contains invalid characters",
            t
        )));
    }
    Ok(())
}

/// Split the text after the header into body and footers.
///
/// Footers are lines matching `token: value` or `token #value` at the end,
/// separated from the body by a blank line.
fn parse_body_and_footers(text: &str) -> (Option<String>, Vec<Footer>) {
    // Split at the last blank line to find potential footer block
    let paragraphs: Vec<&str> = text.split("\n\n").collect();

    if !paragraphs.is_empty() {
        // Try to parse the last paragraph as footers
        let last = paragraphs.last().unwrap();
        let mut footers = Vec::new();
        let mut all_footers = true;

        for line in last.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(footer) = parse_footer_line(line) {
                footers.push(footer);
            } else {
                all_footers = false;
                break;
            }
        }

        if all_footers && !footers.is_empty() {
            // Everything before the last paragraph is body
            let body_parts: Vec<&str> = paragraphs[..paragraphs.len() - 1].to_vec();
            let body = if body_parts.is_empty() {
                None
            } else {
                let b = body_parts.join("\n\n");
                if b.trim().is_empty() {
                    None
                } else {
                    Some(b)
                }
            };
            return (body, footers);
        }
    }

    // No footers found — everything is body
    (Some(text.to_string()), vec![])
}

/// Try to parse a single line as a footer: `Token: value` or `Token #value`.
fn parse_footer_line(line: &str) -> Option<Footer> {
    // BREAKING CHANGE: value  (special case: space in token)
    if let Some(value) = line.strip_prefix("BREAKING CHANGE: ") {
        return Some(Footer {
            token: "BREAKING CHANGE".to_string(),
            value: value.to_string(),
        });
    }
    if let Some(value) = line.strip_prefix("BREAKING-CHANGE: ") {
        return Some(Footer {
            token: "BREAKING-CHANGE".to_string(),
            value: value.to_string(),
        });
    }

    // Token: value
    if let Some(colon_pos) = line.find(": ") {
        let token = &line[..colon_pos];
        if is_footer_token(token) {
            return Some(Footer {
                token: token.to_string(),
                value: line[colon_pos + 2..].to_string(),
            });
        }
    }

    // Token #value
    if let Some(hash_pos) = line.find(" #") {
        let token = &line[..hash_pos];
        if is_footer_token(token) {
            return Some(Footer {
                token: token.to_string(),
                value: line[hash_pos + 2..].to_string(),
            });
        }
    }

    None
}

/// A valid footer token is one or more word characters (letters, digits, hyphens).
fn is_footer_token(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Configuration for conventional commit validation.
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub allowed_types: Vec<String>,
    pub require_scope: bool,
    pub allowed_scopes: Vec<String>,
    pub allow_breaking: bool,
    pub max_subject_length: usize,
    pub require_lowercase: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            allowed_types: [
                "feat", "fix", "docs", "style", "refactor", "perf", "test", "chore", "ci",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            require_scope: false,
            allowed_scopes: vec![],
            allow_breaking: true,
            max_subject_length: 72,
            require_lowercase: true,
        }
    }
}

/// Validation error with helpful context.
#[derive(Debug)]
pub enum ValidationError {
    ParseError(String),
    InvalidType { found: String, allowed: Vec<String> },
    MissingScope,
    InvalidScope { found: String, allowed: Vec<String> },
    SubjectTooLong { length: usize, max: usize },
    SubjectNotLowercase,
    BreakingNotAllowed,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseError(msg) => {
                write!(
                    f,
                    "invalid commit format: {}\n  expected: <type>[(<scope>)]: <description>",
                    msg
                )
            }
            Self::InvalidType { found, allowed } => {
                write!(
                    f,
                    "invalid type '{}'\n  allowed: {}",
                    found,
                    allowed.join(", ")
                )
            }
            Self::MissingScope => {
                write!(
                    f,
                    "scope is required\n  format: <type>(<scope>): <description>"
                )
            }
            Self::InvalidScope { found, allowed } => {
                write!(
                    f,
                    "invalid scope '{}'\n  allowed: {}",
                    found,
                    allowed.join(", ")
                )
            }
            Self::SubjectTooLong { length, max } => {
                write!(f, "subject too long ({} chars, max {})", length, max)
            }
            Self::SubjectNotLowercase => {
                write!(f, "subject should start with lowercase")
            }
            Self::BreakingNotAllowed => {
                write!(f, "breaking changes not allowed")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate a parsed commit message against the given config.
///
/// Returns all validation errors found (not just the first).
pub fn validate(msg: &CommitMessage, config: &ValidationConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if !config.allowed_types.contains(&msg.commit_type) {
        errors.push(ValidationError::InvalidType {
            found: msg.commit_type.clone(),
            allowed: config.allowed_types.clone(),
        });
    }

    if config.require_scope && msg.scope.is_none() {
        errors.push(ValidationError::MissingScope);
    }

    if !config.allowed_scopes.is_empty() {
        if let Some(scope) = &msg.scope {
            if !config.allowed_scopes.contains(scope) {
                errors.push(ValidationError::InvalidScope {
                    found: scope.clone(),
                    allowed: config.allowed_scopes.clone(),
                });
            }
        }
    }

    if msg.description.len() > config.max_subject_length {
        errors.push(ValidationError::SubjectTooLong {
            length: msg.description.len(),
            max: config.max_subject_length,
        });
    }

    if config.require_lowercase {
        if let Some(c) = msg.description.chars().next() {
            if c.is_uppercase() {
                errors.push(ValidationError::SubjectNotLowercase);
            }
        }
    }

    if msg.breaking && !config.allow_breaking {
        errors.push(ValidationError::BreakingNotAllowed);
    }

    errors
}

/// Parse and validate a raw commit message string.
///
/// Returns the parsed message and any validation errors.
pub fn parse_and_validate(
    input: &str,
    config: &ValidationConfig,
) -> std::result::Result<(CommitMessage, Vec<ValidationError>), ValidationError> {
    let msg = parse(input).map_err(|e| ValidationError::ParseError(e.to_string()))?;
    let errors = validate(&msg, config);
    Ok((msg, errors))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let msg = parse("feat: add feature").unwrap();
        assert_eq!(msg.commit_type, "feat");
        assert_eq!(msg.scope, None);
        assert!(!msg.breaking);
        assert_eq!(msg.description, "add feature");
        assert!(msg.body.is_none());
        assert!(msg.footers.is_empty());
    }

    #[test]
    fn parse_with_scope() {
        let msg = parse("feat(auth): add OAuth2").unwrap();
        assert_eq!(msg.commit_type, "feat");
        assert_eq!(msg.scope, Some("auth".into()));
        assert_eq!(msg.description, "add OAuth2");
    }

    #[test]
    fn parse_breaking_exclamation() {
        let msg = parse("feat!: breaking change").unwrap();
        assert!(msg.breaking);
        assert_eq!(msg.commit_type, "feat");
    }

    #[test]
    fn parse_breaking_scope_exclamation() {
        let msg = parse("feat(api)!: change response format").unwrap();
        assert!(msg.breaking);
        assert_eq!(msg.scope, Some("api".into()));
    }

    #[test]
    fn parse_breaking_footer() {
        let msg = parse("feat: change api\n\nBREAKING CHANGE: old API removed").unwrap();
        assert!(msg.breaking);
        assert_eq!(msg.footers.len(), 1);
        assert_eq!(msg.footers[0].token, "BREAKING CHANGE");
        assert_eq!(msg.footers[0].value, "old API removed");
    }

    #[test]
    fn parse_body_and_footer() {
        let input = "fix: resolve crash\n\nThe crash was caused by a null pointer.\n\nCloses #42";
        let msg = parse(input).unwrap();
        assert_eq!(msg.description, "resolve crash");
        assert_eq!(
            msg.body,
            Some("The crash was caused by a null pointer.".into())
        );
        assert_eq!(msg.footers.len(), 1);
        assert_eq!(msg.footers[0].token, "Closes");
        assert_eq!(msg.footers[0].value, "42");
    }

    #[test]
    fn parse_body_no_footer() {
        let input = "docs: update readme\n\nAdded installation instructions.";
        let msg = parse(input).unwrap();
        assert_eq!(msg.body, Some("Added installation instructions.".into()));
        assert!(msg.footers.is_empty());
    }

    #[test]
    fn parse_error_missing_separator() {
        assert!(parse("feat add feature").is_err());
    }

    #[test]
    fn parse_error_empty() {
        assert!(parse("").is_err());
    }

    #[test]
    fn validate_valid() {
        let msg = parse("feat: add feature").unwrap();
        let errors = validate(&msg, &ValidationConfig::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_invalid_type() {
        let msg = parse("invalid: something").unwrap();
        let errors = validate(&msg, &ValidationConfig::default());
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::InvalidType { .. }));
    }

    #[test]
    fn validate_missing_scope() {
        let msg = parse("feat: add feature").unwrap();
        let config = ValidationConfig {
            require_scope: true,
            ..Default::default()
        };
        let errors = validate(&msg, &config);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::MissingScope)));
    }

    #[test]
    fn validate_invalid_scope() {
        let msg = parse("feat(unknown): add feature").unwrap();
        let config = ValidationConfig {
            allowed_scopes: vec!["api".into(), "cli".into()],
            ..Default::default()
        };
        let errors = validate(&msg, &config);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidScope { .. })));
    }

    #[test]
    fn validate_subject_too_long() {
        let long_desc = "a".repeat(80);
        let msg = parse(&format!("feat: {}", long_desc)).unwrap();
        let errors = validate(&msg, &ValidationConfig::default());
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::SubjectTooLong { .. })));
    }

    #[test]
    fn validate_uppercase_subject() {
        let msg = parse("feat: Add feature").unwrap();
        let errors = validate(&msg, &ValidationConfig::default());
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::SubjectNotLowercase)));
    }

    #[test]
    fn validate_breaking_not_allowed() {
        let msg = parse("feat!: breaking").unwrap();
        let config = ValidationConfig {
            allow_breaking: false,
            ..Default::default()
        };
        let errors = validate(&msg, &config);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::BreakingNotAllowed)));
    }
}
