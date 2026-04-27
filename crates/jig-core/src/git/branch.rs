//! Branch name newtype.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Branch(String);

impl Branch {
    pub fn new(s: impl Into<String>) -> Self {
        let name = s.into();
        let refname = format!("refs/heads/{name}");
        assert!(
            git2::Reference::is_valid_name(&refname),
            "invalid git branch name: {name:?}"
        );
        Self(name)
    }
}

impl std::fmt::Display for Branch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::ops::Deref for Branch {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl From<String> for Branch {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for Branch {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<Branch> for String {
    fn from(b: Branch) -> Self {
        b.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_valid() {
        Branch::new("main");
        Branch::new("feature/foo");
        Branch::new("origin/main");
        Branch::new("feature/aut-4969-spawn-agent-thread-is-broken");
    }

    #[test]
    #[should_panic(expected = "invalid git branch name")]
    fn new_rejects_empty() {
        Branch::new("");
    }

    #[test]
    #[should_panic(expected = "invalid git branch name")]
    fn new_rejects_double_dots() {
        Branch::new("a..b");
    }

    #[test]
    #[should_panic(expected = "invalid git branch name")]
    fn new_rejects_space() {
        Branch::new("a b");
    }

    #[test]
    #[should_panic(expected = "invalid git branch name")]
    fn new_rejects_dot_lock() {
        Branch::new("branch.lock");
    }
}
