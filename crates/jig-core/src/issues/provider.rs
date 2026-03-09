//! Issue provider trait.

use crate::error::Result;

use super::types::{Issue, IssueFilter, IssueStatus};

/// Trait for issue backends (file-based, Linear, GitHub, etc.).
pub trait IssueProvider {
    /// Provider name (e.g. "file").
    fn name(&self) -> &str;

    /// List issues matching the given filter.
    fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>>;

    /// Get a single issue by ID.
    fn get(&self, id: &str) -> Result<Option<Issue>>;

    /// List issues eligible for auto-spawning (status=Planned + auto=true).
    fn list_spawnable(&self) -> Result<Vec<Issue>>;

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
