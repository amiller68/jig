use crate::error::Result;
use crate::issues::issue::{Issue, IssueFilter, IssueStatus};

use super::super::{IssueBackend, ProviderKind};
use super::LinearProvider;

impl IssueBackend for LinearProvider {
    fn name(&self) -> &str {
        "linear"
    }

    fn kind(&self) -> ProviderKind {
        Self::PROVIDER_KIND
    }

    fn update_status(&self, id: &str, new_status: &IssueStatus) -> Result<()> {
        self.update_status(id, new_status)
    }

    fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>> {
        self.list_issues(filter)
    }

    fn get(&self, id: &str) -> Result<Option<Issue>> {
        self.get_issue(id)
    }
}
