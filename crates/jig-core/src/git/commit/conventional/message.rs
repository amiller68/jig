use std::fmt;
use std::str::FromStr;

use super::ParseError;

type Result<T> = std::result::Result<T, ParseError>;

/// Known conventional commit types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommitType {
    Feat,
    Fix,
    Docs,
    Style,
    Refactor,
    Perf,
    Test,
    Chore,
    Ci,
}

impl CommitType {
    pub const ALL: &[CommitType] = &[
        Self::Feat,
        Self::Fix,
        Self::Docs,
        Self::Style,
        Self::Refactor,
        Self::Perf,
        Self::Test,
        Self::Chore,
        Self::Ci,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Feat => "feat",
            Self::Fix => "fix",
            Self::Docs => "docs",
            Self::Style => "style",
            Self::Refactor => "refactor",
            Self::Perf => "perf",
            Self::Test => "test",
            Self::Chore => "chore",
            Self::Ci => "ci",
        }
    }
}

impl fmt::Display for CommitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CommitType {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "feat" => Ok(Self::Feat),
            "fix" => Ok(Self::Fix),
            "docs" => Ok(Self::Docs),
            "style" => Ok(Self::Style),
            "refactor" => Ok(Self::Refactor),
            "perf" => Ok(Self::Perf),
            "test" => Ok(Self::Test),
            "chore" => Ok(Self::Chore),
            "ci" => Ok(Self::Ci),
            other => Err(ParseError::UnknownType(other.to_string())),
        }
    }
}

/// A parsed conventional commit message.
#[derive(Debug, Clone, PartialEq)]
pub struct CommitMessage {
    pub commit_type: CommitType,
    pub scope: Option<String>,
    pub breaking: bool,
    pub description: String,
    pub body: Option<String>,
    pub footers: Vec<Footer>,
}

impl TryFrom<&str> for CommitMessage {
    type Error = ParseError;

    fn try_from(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ParseError::Empty);
        }

        let (header, rest) = match input.find("\n\n") {
            Some(pos) => (&input[..pos], Some(input[pos + 2..].trim())),
            None => (input, None),
        };

        let header = header.lines().next().unwrap_or(header);

        let colon_pos = header
            .find(": ")
            .ok_or_else(|| ParseError::MissingSeparator(header.to_string()))?;

        let prefix = &header[..colon_pos];
        let description = header[colon_pos + 2..].to_string();

        if description.is_empty() {
            return Err(ParseError::EmptyDescription);
        }

        // Parse prefix: type[(scope)][!]
        let prefix = prefix.trim();
        let (prefix, breaking_mark) = if let Some(stripped) = prefix.strip_suffix('!') {
            (stripped, true)
        } else {
            (prefix, false)
        };

        let (commit_type, scope) = if let Some(paren_start) = prefix.find('(') {
            let type_str = &prefix[..paren_start];
            let after_paren = &prefix[paren_start + 1..];
            let paren_end = after_paren.find(')').ok_or(ParseError::UnclosedScope)?;
            if paren_end + 1 != after_paren.len() {
                return Err(ParseError::TrailingAfterScope(prefix.to_string()));
            }
            (type_str.parse()?, Some(after_paren[..paren_end].to_string()))
        } else {
            (prefix.parse()?, None)
        };

        // Parse body and footers from text after the blank line
        let (body, footers) = match rest {
            Some(text) if !text.is_empty() => {
                let paragraphs: Vec<&str> = text.split("\n\n").collect();
                let last = paragraphs.last().unwrap();

                let mut parsed_footers = Vec::new();
                let mut all_footers = true;
                for line in last.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Some(footer) = Footer::parse_line(line) {
                        parsed_footers.push(footer);
                    } else {
                        all_footers = false;
                        break;
                    }
                }

                if all_footers && !parsed_footers.is_empty() {
                    let body = paragraphs[..paragraphs.len() - 1].join("\n\n");
                    let body = if body.trim().is_empty() {
                        None
                    } else {
                        Some(body)
                    };
                    (body, parsed_footers)
                } else {
                    (Some(text.to_string()), vec![])
                }
            }
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
}

/// A footer token-value pair from a conventional commit.
#[derive(Debug, Clone, PartialEq)]
pub struct Footer {
    pub token: String,
    pub value: String,
}

impl Footer {
    pub(super) fn parse_line(line: &str) -> Option<Self> {
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

        if let Some(colon_pos) = line.find(": ") {
            let token = &line[..colon_pos];
            if Self::is_valid_token(token) {
                return Some(Footer {
                    token: token.to_string(),
                    value: line[colon_pos + 2..].to_string(),
                });
            }
        }

        if let Some(hash_pos) = line.find(" #") {
            let token = &line[..hash_pos];
            if Self::is_valid_token(token) {
                return Some(Footer {
                    token: token.to_string(),
                    value: line[hash_pos + 2..].to_string(),
                });
            }
        }

        None
    }

    fn is_valid_token(token: &str) -> bool {
        !token.is_empty() && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }
}
