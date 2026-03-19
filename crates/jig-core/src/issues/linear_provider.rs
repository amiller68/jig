//! Linear issue provider.
//!
//! Fetches issues from the Linear GraphQL API, mapping them to jig's
//! `Issue` type. Auth comes from a named profile in the global config.

use crate::config::LinearIssuesConfig;
use crate::error::{Error, Result};
use crate::global::GlobalConfig;

use super::linear_client::LinearClient;
use super::provider::IssueProvider;
use super::types::{Issue, IssueFilter, IssueStatus};

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
    pub const PROVIDER_KIND: super::provider::ProviderKind = super::provider::ProviderKind::Linear;

    /// Build a provider from repo and global config.
    ///
    /// Looks up the named profile in `global_config` to resolve the API key.
    /// Per-repo fields override profile-level defaults.
    pub fn from_config(
        global_config: &GlobalConfig,
        linear_config: &LinearIssuesConfig,
    ) -> Result<Self> {
        let profile = global_config
            .linear
            .profiles
            .get(&linear_config.profile)
            .ok_or_else(|| {
                Error::Linear(format!(
                    "Linear profile '{}' not found in global config (~/.config/jig/config.toml)",
                    linear_config.profile,
                ))
            })?;

        // Resolution: per-repo jig.toml > profile default > omitted.
        let team = linear_config
            .team
            .clone()
            .or_else(|| profile.team.clone())
            .ok_or_else(|| {
                Error::Linear(
                    "Linear team key is required — set 'team' in [issues.linear] in jig.toml or in the profile in ~/.config/jig/config.toml"
                        .to_string(),
                )
            })?;

        let projects = if linear_config.projects.is_empty() {
            profile.projects.clone()
        } else {
            linear_config.projects.clone()
        };

        let labels = if linear_config.labels.is_empty() {
            profile.labels.clone()
        } else {
            linear_config.labels.clone()
        };

        let assignee = linear_config
            .assignee
            .clone()
            .or_else(|| profile.assignee.clone());

        let client = LinearClient::new(&profile.api_key);

        // Resolve "me" to the authenticated user's ID.
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
        self.client
            .update_issue_status(identifier, &self.team, new_status)
    }

    /// Create a new issue in Linear.
    ///
    /// Uses team, project, assignee, and labels from the provider's config,
    /// with explicit arguments taking precedence.
    pub fn create_issue(
        &self,
        title: &str,
        body: Option<&str>,
        priority: Option<&super::types::IssuePriority>,
        labels: &[String],
        category: Option<&str>,
    ) -> Result<String> {
        // Merge labels: explicit labels take precedence, fall back to config
        let effective_labels = if labels.is_empty() {
            &self.labels
        } else {
            labels
        };

        // Category maps to Linear project; explicit overrides config
        let project = category.or_else(|| self.projects.first().map(|s| s.as_str()));

        self.client.create_issue(
            &self.team,
            title,
            body,
            priority,
            effective_labels,
            project,
            self.assignee.as_deref(),
        )
    }
}

impl IssueProvider for LinearProvider {
    fn name(&self) -> &str {
        "linear"
    }

    fn kind(&self) -> super::provider::ProviderKind {
        Self::PROVIDER_KIND
    }

    fn list(&self, filter: &IssueFilter) -> Result<Vec<Issue>> {
        // Use project filter from IssueFilter if provided, else fall back to config.
        let projects = if let Some(ref cat) = filter.category {
            vec![cat.clone()]
        } else {
            self.projects.clone()
        };

        let mut issues = self.client.list_issues(
            &self.team,
            &projects,
            filter.status.as_ref(),
            filter.priority.as_ref(),
            self.assignee.as_deref(),
        )?;

        // Client-side label filtering: merge config labels with filter labels.
        // All specified labels must match.
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

        // Sort by priority then id, consistent with FileProvider.
        issues.sort_by(|a, b| {
            let pa = a.priority.as_ref().map(|p| p.clone() as u8).unwrap_or(99);
            let pb = b.priority.as_ref().map(|p| p.clone() as u8).unwrap_or(99);
            pa.cmp(&pb).then_with(|| a.id.cmp(&b.id))
        });

        Ok(issues)
    }

    fn get(&self, id: &str) -> Result<Option<Issue>> {
        self.client.get_issue(id)
    }
}
