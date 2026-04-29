//! Issue provider trait and implementations.

pub mod linear;

use std::fmt;

use crate::error::Result;

use super::issue::{Issue, IssueFilter, IssueRef, IssueStatus};

/// Identifies the type of issue provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    /// Linear integration provider.
    Linear,
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderKind::Linear => write!(f, "linear"),
        }
    }
}

/// Trait for issue backends (file-based, Linear, GitHub, etc.).
pub trait IssueBackend {
    fn name(&self) -> &str;
    fn kind(&self) -> ProviderKind;
    fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>>;
    fn get(&self, id: &str) -> Result<Option<Issue>>;
    fn update_status(&self, id: &str, status: &IssueStatus) -> Result<()>;
}

/// Concrete handle that adapts to a project's configured issue backend.
pub struct IssueProvider {
    inner: Box<dyn IssueBackend>,
}

impl IssueProvider {
    pub fn new(inner: Box<dyn IssueBackend>) -> Self {
        Self { inner }
    }

    pub fn name(&self) -> &str {
        self.inner.name()
    }

    pub fn kind(&self) -> ProviderKind {
        self.inner.kind()
    }

    pub fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>> {
        self.inner.list(filter)
    }

    pub fn get(&self, id: &str) -> Result<Option<Issue>> {
        self.inner.get(id)
    }

    pub fn update_status(&self, id: &str, status: &IssueStatus) -> Result<()> {
        self.inner.update_status(id, status)
    }

    /// Check whether all dependencies of an issue are satisfied (Complete).
    pub fn may_spawn(&self, id: &IssueRef) -> bool {
        let issue = match self.get(id) {
            Ok(Some(issue)) => issue,
            _ => return false,
        };
        issue.depends_on().iter().all(|dep_id| {
            matches!(self.get(dep_id), Ok(Some(dep)) if *dep.status() == IssueStatus::Complete)
        })
    }
}
