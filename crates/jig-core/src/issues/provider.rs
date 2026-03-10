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

    /// List issues eligible for auto-spawning (status=Planned).
    ///
    /// Default impl: queries for Planned issues, filters by `spawn_labels`
    /// (all must match, case-insensitive), and excludes issues with
    /// unresolved dependencies. Providers only need to implement `list` and
    /// `get`; override this only if the backend can push filtering server-side.
    fn list_spawnable(&self, spawn_labels: &[String]) -> Result<Vec<Issue>> {
        let all = self.list(&IssueFilter {
            status: Some(IssueStatus::Planned),
            ..Default::default()
        })?;
        Ok(all
            .into_iter()
            .filter(|i| {
                spawn_labels
                    .iter()
                    .all(|required| i.labels.iter().any(|l| l.eq_ignore_ascii_case(required)))
            })
            .filter(|i| self.is_spawnable_with_deps(i))
            .collect())
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
