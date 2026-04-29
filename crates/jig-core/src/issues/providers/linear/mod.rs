//! Linear issue provider.
//!
//! Fetches issues from the Linear GraphQL API, mapping them to jig's
//! `Issue` type.

pub mod client;
mod provider;

use crate::error::Result;

use crate::issues::issue::{Issue, IssueFilter, IssueStatus};
use client::LinearClient;

/// Issue provider backed by the Linear API.
pub struct LinearProvider {
    client: LinearClient,
    team: String,
    projects: Vec<String>,
    assignee: Option<String>,
    labels: Vec<String>,
}

impl LinearProvider {
    /// The provider kind for Linear issues.
    pub const PROVIDER_KIND: super::ProviderKind = super::ProviderKind::Linear;

    /// Create a provider with resolved config values.
    ///
    /// If `assignee` is `"me"`, resolves to the authenticated user's ID
    /// via the Linear API.
    pub fn new(
        api_key: &str,
        team: String,
        projects: Vec<String>,
        assignee: Option<String>,
        labels: Vec<String>,
    ) -> Result<Self> {
        let client = LinearClient::new(api_key);

        let assignee = match assignee.as_deref() {
            Some("me") => {
                let viewer_id = client.viewer_id()?;
                Some(viewer_id)
            }
            other => other.map(|s| s.to_string()),
        };

        Ok(Self {
            client,
            team,
            projects,
            assignee,
            labels,
        })
    }
}

impl LinearProvider {
    /// Update the workflow state of a Linear issue.
    pub fn update_status(&self, identifier: &str, new_status: &IssueStatus) -> Result<()> {
        Ok(self
            .client
            .update_issue_status(identifier, &self.team, new_status)?)
    }

    /// Update an existing issue's fields in Linear.
    ///
    /// Only fields that are `Some` / non-empty are updated.
    /// `assignee` accepts "me" (resolved to the authenticated user's ID) or a
    /// raw Linear user ID.
    #[allow(clippy::too_many_arguments)]
    pub fn update_issue(
        &self,
        identifier: &str,
        title: Option<&str>,
        body: Option<&str>,
        priority: Option<&crate::issues::issue::IssuePriority>,
        labels: &[String],
        category: Option<&str>,
        assignee: Option<&str>,
        parent: Option<&str>,
        remove_parent: bool,
    ) -> Result<()> {
        let resolved_assignee = match assignee {
            Some("me") => Some(self.client.viewer_id()?),
            other => other.map(|s| s.to_string()),
        };

        Ok(self.client.update_issue(
            identifier,
            &self.team,
            title,
            body,
            priority,
            labels,
            category,
            resolved_assignee.as_deref(),
            parent,
            remove_parent,
        )?)
    }

    /// Add a "blocked by" dependency relation.
    ///
    /// `identifier` is blocked by `blocker_identifier`.
    pub fn add_blocked_by(&self, identifier: &str, blocker_identifier: &str) -> Result<()> {
        Ok(self
            .client
            .create_blocked_by_relation(identifier, blocker_identifier)?)
    }

    /// Remove a "blocked by" dependency relation.
    pub fn remove_blocked_by(&self, identifier: &str, blocker_identifier: &str) -> Result<()> {
        Ok(self
            .client
            .remove_blocked_by_relation(identifier, blocker_identifier)?)
    }

    /// Create a new issue in Linear.
    ///
    /// Uses team, project, assignee, and labels from the provider's config,
    /// with explicit arguments taking precedence.
    #[allow(clippy::too_many_arguments)]
    pub fn create_issue(
        &self,
        title: &str,
        body: Option<&str>,
        priority: Option<&crate::issues::issue::IssuePriority>,
        labels: &[String],
        category: Option<&str>,
        parent: Option<&str>,
        initial_status: Option<&IssueStatus>,
    ) -> Result<String> {
        // Merge labels: explicit labels take precedence, fall back to config
        let effective_labels = if labels.is_empty() {
            &self.labels
        } else {
            labels
        };

        // Category maps to Linear project; explicit overrides config
        let project = category.or_else(|| self.projects.first().map(|s| s.as_str()));

        Ok(self.client.create_issue(
            &self.team,
            title,
            body,
            priority,
            effective_labels,
            project,
            self.assignee.as_deref(),
            parent,
            initial_status,
        )?)
    }
}

impl LinearProvider {
    pub(crate) fn list_issues(&self, filter: &IssueFilter) -> Result<Vec<Issue>> {
        let mut issues = self.client.list_issues(
            &self.team,
            &self.projects,
            filter.status.as_ref(),
            filter.priority.as_ref(),
            self.assignee.as_deref(),
        )?;

        let effective_labels: Vec<String> = if !filter.labels.is_empty() {
            filter.labels.clone()
        } else {
            self.labels.clone()
        };

        if !effective_labels.is_empty() {
            let label_filter = IssueFilter {
                labels: effective_labels,
                ..IssueFilter::default()
            };
            issues.retain(|i| i.matches(&label_filter));
        }

        issues.sort_by(|a, b| {
            a.priority()
                .cmp(b.priority())
                .then_with(|| a.id().cmp(b.id()))
        });

        Ok(issues)
    }

    pub(crate) fn get_issue(&self, id: &str) -> Result<Option<Issue>> {
        Ok(self.client.get_issue(id)?)
    }
}
