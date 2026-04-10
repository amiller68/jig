//! Issue provider trait.

use std::fmt;

use crate::error::Result;

use super::types::{Issue, IssueFilter, IssueStatus};

/// Identifies the type of issue provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// File-based provider (reads `issues/` markdown files).
    File,
    /// Linear integration provider.
    Linear,
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderKind::File => write!(f, "file"),
            ProviderKind::Linear => write!(f, "linear"),
        }
    }
}

/// Trait for issue backends (file-based, Linear, GitHub, etc.).
pub trait IssueProvider {
    /// Provider name (e.g. "file").
    fn name(&self) -> &str;

    /// The kind of provider.
    fn kind(&self) -> ProviderKind;

    /// List issues matching the given filter.
    fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>>;

    /// Get a single issue by ID.
    fn get(&self, id: &str) -> Result<Option<Issue>>;

    /// Update the status of an issue by ID.
    fn update_status(&self, id: &str, status: &IssueStatus) -> Result<()>;

    /// List issues eligible for auto-spawning (status=Planned).
    ///
    /// - Empty slice: return ALL planned issues whose dependencies are satisfied
    /// - Non-empty slice: filter to issues whose labels match all entries
    ///   (case-insensitive), with satisfied dependencies
    ///
    /// Providers only need to implement `list` and `get`; override this only
    /// if the backend can push filtering server-side.
    fn list_spawnable(&self, spawn_labels: &[String]) -> Result<Vec<Issue>> {
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
    ///
    /// Returns all issues with Triage status. Unlike spawnable issues, triage
    /// issues don't need label filtering or dependency checks.
    fn list_triageable(&self) -> Result<Vec<Issue>> {
        self.list(&IssueFilter {
            status: Some(IssueStatus::Triage),
            ..Default::default()
        })
    }

    /// Check whether all dependencies of an issue are satisfied (Complete).
    ///
    /// Returns `true` if the issue has no dependencies or all dependencies
    /// resolve to `IssueStatus::Complete`. Missing/unresolvable dependencies
    /// are treated as unsatisfied.
    fn is_spawnable_with_deps(&self, issue: &Issue) -> bool {
        issue.depends_on.iter().all(|dep_id| {
            matches!(self.get(dep_id), Ok(Some(dep)) if dep.status == IssueStatus::Complete)
        })
    }
}
