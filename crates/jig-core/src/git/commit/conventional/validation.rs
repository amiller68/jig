use super::{CommitMessage, CommitType, ParseError};

/// Configuration for conventional commit validation.
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub allowed_types: Vec<CommitType>,
    pub require_scope: bool,
    pub allowed_scopes: Vec<String>,
    pub allow_breaking: bool,
    pub max_subject_length: usize,
    pub require_lowercase: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            allowed_types: CommitType::ALL.to_vec(),
            require_scope: false,
            allowed_scopes: vec![],
            allow_breaking: true,
            max_subject_length: 72,
            require_lowercase: true,
        }
    }
}

impl ValidationConfig {
    pub fn parse_and_validate(
        &self,
        input: &str,
    ) -> Result<(CommitMessage, Vec<ValidationError>), ValidationError> {
        let msg = CommitMessage::try_from(input).map_err(ValidationError::Parse)?;
        let errors = msg.validate(self);
        Ok((msg, errors))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("invalid commit format: {0}\n  expected: <type>[(<scope>)]: <description>")]
    Parse(ParseError),

    #[error("type '{found}' not allowed\n  allowed: {}", allowed.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", "))]
    DisallowedType {
        found: CommitType,
        allowed: Vec<CommitType>,
    },

    #[error("scope is required\n  format: <type>(<scope>): <description>")]
    MissingScope,

    #[error("invalid scope '{found}'\n  allowed: {}", allowed.join(", "))]
    InvalidScope { found: String, allowed: Vec<String> },

    #[error("subject too long ({length} chars, max {max})")]
    SubjectTooLong { length: usize, max: usize },

    #[error("subject should start with lowercase")]
    SubjectNotLowercase,

    #[error("breaking changes not allowed")]
    BreakingNotAllowed,
}

impl CommitMessage {
    pub fn validate(&self, config: &ValidationConfig) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if !config.allowed_types.contains(&self.commit_type) {
            errors.push(ValidationError::DisallowedType {
                found: self.commit_type,
                allowed: config.allowed_types.clone(),
            });
        }

        if config.require_scope && self.scope.is_none() {
            errors.push(ValidationError::MissingScope);
        }

        if !config.allowed_scopes.is_empty() {
            if let Some(scope) = &self.scope {
                if !config.allowed_scopes.contains(scope) {
                    errors.push(ValidationError::InvalidScope {
                        found: scope.clone(),
                        allowed: config.allowed_scopes.clone(),
                    });
                }
            }
        }

        if self.description.len() > config.max_subject_length {
            errors.push(ValidationError::SubjectTooLong {
                length: self.description.len(),
                max: config.max_subject_length,
            });
        }

        if config.require_lowercase {
            if let Some(c) = self.description.chars().next() {
                if c.is_uppercase() {
                    errors.push(ValidationError::SubjectNotLowercase);
                }
            }
        }

        if self.breaking && !config.allow_breaking {
            errors.push(ValidationError::BreakingNotAllowed);
        }

        errors
    }
}
