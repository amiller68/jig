//! Issue provider trait.

use crate::error::Result;

use super::types::{Issue, IssueFilter};

/// Trait for issue backends (file-based, Linear, GitHub, etc.).
pub trait IssueProvider {
    /// Provider name (e.g. "file").
    fn name(&self) -> &str;

    /// List issues matching the given filter.
    fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>>;

    /// Get a single issue by ID.
    fn get(&self, id: &str) -> Result<Option<Issue>>;
}
