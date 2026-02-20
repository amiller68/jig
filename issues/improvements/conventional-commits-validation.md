# Conventional Commits Validation Library

**Status:** Planned  
**Priority:** Medium  
**Category:** Improvements

## Objective

Build a robust, configurable conventional commit message validator with helpful error messages and pre-commit hook support.

## Background

Conventional commits are critical for:
- Automated versioning (semver bumps)
- Changelog generation
- Release automation
- Consistent commit history

Current validation is done with regex in shell scripts. Need proper parser with:
- Clear error messages
- Scope validation
- Breaking change detection
- Custom type configuration

## Specification

**Conventional Commits v1.0.0:**
```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

**With breaking changes:**
```
<type>!: <description>
<type>(<scope>)!: <description>
BREAKING CHANGE: <description>
```

## Architecture

### Parser

**Using nom for parsing:**

```rust
use nom::{
    IResult,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{alphanumeric1, space0, space1},
    combinator::opt,
    sequence::{delimited, tuple},
};

#[derive(Debug, PartialEq)]
pub struct CommitMessage {
    pub commit_type: String,
    pub scope: Option<String>,
    pub breaking: bool,
    pub description: String,
    pub body: Option<String>,
    pub footers: Vec<Footer>,
}

#[derive(Debug, PartialEq)]
pub struct Footer {
    pub token: String,
    pub value: String,
}

pub fn parse_commit_message(input: &str) -> IResult<&str, CommitMessage> {
    let (input, (commit_type, scope, breaking, _, description)) = tuple((
        alphanumeric1,                          // type
        opt(delimited(tag("("), take_until(")"), tag(")"))),  // optional scope
        opt(tag("!")),                          // optional breaking !
        tag(": "),                              // colon + space
        take_until("\n"),                       // description
    ))(input)?;
    
    // Parse body and footers
    let (input, body_and_footers) = opt(tuple((
        tag("\n\n"),
        take_while1(|_| true),
    )))(input)?;
    
    let (body, footers) = if let Some((_, rest)) = body_and_footers {
        parse_body_and_footers(rest)
    } else {
        (None, vec![])
    };
    
    Ok((input, CommitMessage {
        commit_type: commit_type.to_string(),
        scope: scope.map(|s| s.to_string()),
        breaking: breaking.is_some() || footers.iter().any(|f| f.token == "BREAKING CHANGE"),
        description: description.to_string(),
        body,
        footers,
    }))
}
```

### Validator

```rust
#[derive(Debug)]
pub struct ValidationConfig {
    pub allowed_types: Vec<String>,
    pub require_scope: bool,
    pub allowed_scopes: Vec<String>,  // empty = any
    pub allow_breaking: bool,
    pub max_subject_length: usize,
    pub require_lowercase: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            allowed_types: vec![
                "feat", "fix", "docs", "style", "refactor",
                "perf", "test", "chore", "ci"
            ].into_iter().map(String::from).collect(),
            require_scope: false,
            allowed_scopes: vec![],
            allow_breaking: true,
            max_subject_length: 72,
            require_lowercase: true,
        }
    }
}

#[derive(Debug)]
pub enum ValidationError {
    InvalidFormat(String),
    InvalidType { found: String, allowed: Vec<String> },
    MissingScope,
    InvalidScope { found: String, allowed: Vec<String> },
    SubjectTooLong { length: usize, max: usize },
    SubjectNotLowercase,
    BreakingNotAllowed,
}

pub fn validate_commit_message(
    message: &CommitMessage,
    config: &ValidationConfig
) -> Result<(), ValidationError> {
    // Validate type
    if !config.allowed_types.contains(&message.commit_type) {
        return Err(ValidationError::InvalidType {
            found: message.commit_type.clone(),
            allowed: config.allowed_types.clone(),
        });
    }
    
    // Validate scope if required
    if config.require_scope && message.scope.is_none() {
        return Err(ValidationError::MissingScope);
    }
    
    // Validate scope against allowed list
    if !config.allowed_scopes.is_empty() {
        if let Some(scope) = &message.scope {
            if !config.allowed_scopes.contains(scope) {
                return Err(ValidationError::InvalidScope {
                    found: scope.clone(),
                    allowed: config.allowed_scopes.clone(),
                });
            }
        }
    }
    
    // Validate subject length
    if message.description.len() > config.max_subject_length {
        return Err(ValidationError::SubjectTooLong {
            length: message.description.len(),
            max: config.max_subject_length,
        });
    }
    
    // Validate lowercase
    if config.require_lowercase {
        let first_char = message.description.chars().next().unwrap();
        if first_char.is_uppercase() {
            return Err(ValidationError::SubjectNotLowercase);
        }
    }
    
    // Validate breaking changes
    if message.breaking && !config.allow_breaking {
        return Err(ValidationError::BreakingNotAllowed);
    }
    
    Ok(())
}
```

### Error Messages

**User-friendly formatting:**

```rust
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ValidationError::InvalidFormat(msg) => {
                write!(f, "Invalid commit message format: {}\n\n\
                    Expected: <type>(<scope>): <description>\n\
                    Example: feat(auth): add OAuth2 support", msg)
            },
            ValidationError::InvalidType { found, allowed } => {
                write!(f, "Invalid commit type '{}'\n\n\
                    Allowed types: {}\n\n\
                    Examples:\n\
                    • feat: new feature (minor version bump)\n\
                    • fix: bug fix (patch version bump)\n\
                    • docs: documentation only\n\
                    • refactor: code refactoring (no behavior change)",
                    found, allowed.join(", "))
            },
            ValidationError::MissingScope => {
                write!(f, "Commit scope is required\n\n\
                    Format: <type>(<scope>): <description>\n\
                    Example: feat(auth): add login endpoint")
            },
            ValidationError::InvalidScope { found, allowed } => {
                write!(f, "Invalid scope '{}'\n\n\
                    Allowed scopes: {}\n\
                    Example: feat({}): add new feature",
                    found, allowed.join(", "), allowed.first().unwrap_or(&"scope".to_string()))
            },
            ValidationError::SubjectTooLong { length, max } => {
                write!(f, "Subject line too long ({} characters, max {})\n\n\
                    Keep the subject line concise. Move details to the commit body.",
                    length, max)
            },
            ValidationError::SubjectNotLowercase => {
                write!(f, "Subject should start with lowercase\n\n\
                    Correct: feat: add new feature\n\
                    Wrong:   feat: Add new feature")
            },
            ValidationError::BreakingNotAllowed => {
                write!(f, "Breaking changes are not allowed in this repository")
            },
        }
    }
}
```

## Configuration

**In `jig.toml`:**

```toml
[conventionalCommits]
# Allowed types
types = ["feat", "fix", "docs", "style", "refactor", "perf", "test", "chore", "ci"]

# Require scope (e.g., feat(auth):)
requireScope = false

# Allowed scopes (empty = any)
scopes = ["api", "ui", "cli", "core", "docs"]

# Allow breaking changes (!)
allowBreaking = true

# Max subject length
maxSubjectLength = 72

# Require lowercase subject
requireLowercase = true

# Enforce in pre-commit hook (blocks invalid commits)
enforcePreCommit = false
```

## CLI Integration

### Pre-commit Hook

```bash
# In .git/hooks/pre-commit (generated by jig)
if [ -f .git/jig-hooks.json ]; then
    if jig hooks pre-commit "$1"; then
        exit 0
    else
        echo ""
        echo "💡 Tip: Use 'git commit --no-verify' to bypass this check (not recommended)"
        exit 1
    fi
fi
```

### Manual Validation

```bash
# Validate last commit
jig commit validate

# Validate specific commit
jig commit validate HEAD~3

# Validate commit message from stdin
echo "feat: add feature" | jig commit validate --stdin

# Validate commit message file
jig commit validate --file .git/COMMIT_EDITMSG

# Show examples
jig commit examples
```

## Examples Command

```bash
$ jig commit examples

Conventional Commit Examples:

Basic commits:
  feat: add user authentication
  fix: resolve login timeout
  docs: update README with examples
  refactor: simplify error handling

With scope:
  feat(auth): add OAuth2 support
  fix(ui): correct button alignment
  test(api): add integration tests

Breaking changes:
  feat!: remove legacy API
  feat(api)!: change response format
  
  Or with footer:
  feat(api): change response format
  
  BREAKING CHANGE: API responses now use camelCase instead of snake_case

Valid types:
  • feat:     New feature (minor version bump)
  • fix:      Bug fix (patch version bump)
  • docs:     Documentation only
  • style:    Formatting, missing semi colons, etc.
  • refactor: Code change that neither fixes a bug nor adds a feature
  • perf:     Performance improvement
  • test:     Adding tests
  • chore:    Maintenance tasks
  • ci:       CI configuration changes

Tips:
  • Keep subject line under 72 characters
  • Use present tense ("add" not "added")
  • Start with lowercase (after type)
  • No period at the end of subject
```

## Git Integration

### Commit Message Template

**Generate `.git/commit-template`:**

```
# <type>(<scope>): <subject>
#
# <body>
#
# <footer>
#
# Types: feat, fix, docs, style, refactor, perf, test, chore, ci
# Breaking changes: feat!: or BREAKING CHANGE: in footer
#
# Examples:
#   feat(auth): add OAuth2 support
#   fix(ui): resolve button alignment issue
#   docs: update installation instructions
```

**Install template:**
```bash
jig commit template install
git config commit.template .git/commit-template
```

### Commit Message Editor

**Open editor with examples:**
```bash
jig commit --interactive

# Opens $EDITOR with template pre-filled
# Validates on save
# Retries if invalid
```

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let msg = "feat: add feature";
        let result = parse_commit_message(msg).unwrap().1;
        assert_eq!(result.commit_type, "feat");
        assert_eq!(result.scope, None);
        assert_eq!(result.breaking, false);
        assert_eq!(result.description, "add feature");
    }

    #[test]
    fn test_parse_with_scope() {
        let msg = "feat(auth): add OAuth2";
        let result = parse_commit_message(msg).unwrap().1;
        assert_eq!(result.commit_type, "feat");
        assert_eq!(result.scope, Some("auth".to_string()));
    }

    #[test]
    fn test_parse_breaking_exclamation() {
        let msg = "feat!: breaking change";
        let result = parse_commit_message(msg).unwrap().1;
        assert!(result.breaking);
    }

    #[test]
    fn test_validate_invalid_type() {
        let msg = CommitMessage {
            commit_type: "invalid".to_string(),
            scope: None,
            breaking: false,
            description: "test".to_string(),
            body: None,
            footers: vec![],
        };
        
        let config = ValidationConfig::default();
        assert!(validate_commit_message(&msg, &config).is_err());
    }
}
```

## Implementation Phases

### Phase 1: Core Parser
1. Add `nom` dependency
2. Implement commit message parser
3. Unit tests for parser
4. Error types

### Phase 2: Validator
1. Validation logic
2. Configurable rules
3. Error messages
4. Unit tests for validation

### Phase 3: CLI Integration
1. `jig commit validate` command
2. Pre-commit hook integration
3. `jig commit examples` command
4. Error formatting

### Phase 4: Developer Experience
1. Commit template generation
2. Interactive commit editor
3. Git config integration
4. IDE/editor integration hints

## Acceptance Criteria

### Parser
- [ ] Parse basic format: `type: description`
- [ ] Parse with scope: `type(scope): description`
- [ ] Parse breaking: `type!:` and `type(scope)!:`
- [ ] Parse body and footers
- [ ] Parse `BREAKING CHANGE:` footer
- [ ] Handle multi-line descriptions

### Validator
- [ ] Validate allowed types
- [ ] Validate required scope
- [ ] Validate allowed scopes
- [ ] Validate subject length
- [ ] Validate lowercase
- [ ] Validate breaking changes
- [ ] Configurable rules

### Error Messages
- [ ] Clear, actionable error messages
- [ ] Show examples for common errors
- [ ] Suggest fixes when possible

### CLI
- [ ] `jig commit validate` validates commits
- [ ] `jig commit examples` shows examples
- [ ] `jig commit template install` creates template
- [ ] Pre-commit hook integration
- [ ] Exit codes for scripting

### Configuration
- [ ] Per-repo config in `jig.toml`
- [ ] Configurable types, scopes, rules
- [ ] Enable/disable enforcement
- [ ] Global defaults

## Open Questions

1. Should we support custom types? (Yes, fully configurable)
2. Should we validate body and footer format? (Future enhancement)
3. Should we support emoji in types? (No, conventional commits spec doesn't include them)
4. Should we integrate with commitizen? (No, standalone validator)

## Related Issues

- issues/features/git-hooks-management.md (pre-commit hook)
- issues/features/github-integration.md (PR commit validation)
