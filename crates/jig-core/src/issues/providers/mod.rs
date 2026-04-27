//! Issue provider trait and implementations.

pub mod linear;

use std::fmt;

use crate::error::Result;

use super::issue::{Issue, IssueFilter, IssueStatus};

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

    /// List issues eligible for auto-spawning (status=Planned).
    pub fn list_spawnable(&self, spawn_labels: &[String]) -> Result<Vec<Issue>> {
        let all = self.list(&IssueFilter {
            status: Some(IssueStatus::Planned),
            ..Default::default()
        })?;
        Ok(all
            .into_iter()
            .filter(|i| spawn_labels.is_empty() || i.auto(spawn_labels))
            .filter(|i| self.is_spawnable_with_deps(i))
            .collect())
    }

    /// List issues eligible for triage (status=Triage).
    pub fn list_triageable(&self) -> Result<Vec<Issue>> {
        self.list(&IssueFilter {
            status: Some(IssueStatus::Triage),
            ..Default::default()
        })
    }

    /// Check whether all dependencies of an issue are satisfied (Complete).
    pub fn is_spawnable_with_deps(&self, issue: &Issue) -> bool {
        issue.depends_on().iter().all(|dep_id| {
            matches!(self.get(dep_id), Ok(Some(dep)) if *dep.status() == IssueStatus::Complete)
        })
    }
}
