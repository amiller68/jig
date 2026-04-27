mod message;
mod validation;

pub use message::{CommitMessage, CommitType, Footer};
pub use validation::{ValidationConfig, ValidationError};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("empty commit message")]
    Empty,

    #[error("missing ': ' separator in '{0}'")]
    MissingSeparator(String),

    #[error("empty description")]
    EmptyDescription,

    #[error("unknown commit type '{0}'")]
    UnknownType(String),

    #[error("unclosed scope parenthesis")]
    UnclosedScope,

    #[error("unexpected characters after scope in '{0}'")]
    TrailingAfterScope(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let msg = CommitMessage::try_from("feat: add feature").unwrap();
        assert_eq!(msg.commit_type, CommitType::Feat);
        assert_eq!(msg.scope, None);
        assert!(!msg.breaking);
        assert_eq!(msg.description, "add feature");
        assert!(msg.body.is_none());
        assert!(msg.footers.is_empty());
    }

    #[test]
    fn parse_with_scope() {
        let msg = CommitMessage::try_from("feat(auth): add OAuth2").unwrap();
        assert_eq!(msg.commit_type, CommitType::Feat);
        assert_eq!(msg.scope, Some("auth".into()));
        assert_eq!(msg.description, "add OAuth2");
    }

    #[test]
    fn parse_breaking_exclamation() {
        let msg = CommitMessage::try_from("feat!: breaking change").unwrap();
        assert!(msg.breaking);
        assert_eq!(msg.commit_type, CommitType::Feat);
    }

    #[test]
    fn parse_breaking_scope_exclamation() {
        let msg = CommitMessage::try_from("feat(api)!: change response format").unwrap();
        assert!(msg.breaking);
        assert_eq!(msg.scope, Some("api".into()));
    }

    #[test]
    fn parse_breaking_footer() {
        let msg =
            CommitMessage::try_from("feat: change api\n\nBREAKING CHANGE: old API removed")
                .unwrap();
        assert!(msg.breaking);
        assert_eq!(msg.footers.len(), 1);
        assert_eq!(msg.footers[0].token, "BREAKING CHANGE");
        assert_eq!(msg.footers[0].value, "old API removed");
    }

    #[test]
    fn parse_body_and_footer() {
        let input = "fix: resolve crash\n\nThe crash was caused by a null pointer.\n\nCloses #42";
        let msg = CommitMessage::try_from(input).unwrap();
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
        let msg = CommitMessage::try_from(input).unwrap();
        assert_eq!(msg.body, Some("Added installation instructions.".into()));
        assert!(msg.footers.is_empty());
    }

    #[test]
    fn parse_error_missing_separator() {
        assert!(CommitMessage::try_from("feat add feature").is_err());
    }

    #[test]
    fn parse_error_empty() {
        assert!(CommitMessage::try_from("").is_err());
    }

    #[test]
    fn validate_valid() {
        let msg = CommitMessage::try_from("feat: add feature").unwrap();
        let errors = msg.validate(&ValidationConfig::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn parse_unknown_type() {
        let err = CommitMessage::try_from("invalid: something").unwrap_err();
        assert!(matches!(err, ParseError::UnknownType(_)));
    }

    #[test]
    fn validate_disallowed_type() {
        let msg = CommitMessage::try_from("ci: update pipeline").unwrap();
        let config = ValidationConfig {
            allowed_types: vec![CommitType::Feat, CommitType::Fix],
            ..Default::default()
        };
        let errors = msg.validate(&config);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::DisallowedType { .. }));
    }

    #[test]
    fn validate_missing_scope() {
        let msg = CommitMessage::try_from("feat: add feature").unwrap();
        let config = ValidationConfig {
            require_scope: true,
            ..Default::default()
        };
        let errors = msg.validate(&config);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::MissingScope)));
    }

    #[test]
    fn validate_invalid_scope() {
        let msg = CommitMessage::try_from("feat(unknown): add feature").unwrap();
        let config = ValidationConfig {
            allowed_scopes: vec!["api".into(), "cli".into()],
            ..Default::default()
        };
        let errors = msg.validate(&config);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidScope { .. })));
    }

    #[test]
    fn validate_subject_too_long() {
        let long_desc = "a".repeat(80);
        let msg = CommitMessage::try_from(format!("feat: {}", long_desc).as_str()).unwrap();
        let errors = msg.validate(&ValidationConfig::default());
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::SubjectTooLong { .. })));
    }

    #[test]
    fn validate_uppercase_subject() {
        let msg = CommitMessage::try_from("feat: Add feature").unwrap();
        let errors = msg.validate(&ValidationConfig::default());
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::SubjectNotLowercase)));
    }

    #[test]
    fn validate_breaking_not_allowed() {
        let msg = CommitMessage::try_from("feat!: breaking").unwrap();
        let config = ValidationConfig {
            allow_breaking: false,
            ..Default::default()
        };
        let errors = msg.validate(&config);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::BreakingNotAllowed)));
    }
}
