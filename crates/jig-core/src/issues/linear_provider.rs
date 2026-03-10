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
}

impl LinearProvider {
    /// Build a provider from repo and global config.
    ///
    /// Looks up the named profile in `global_config` to resolve the API key.
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

        let team = linear_config.team.clone().ok_or_else(|| {
            Error::Linear(
                "Linear team key is required — set 'team' in [issues.linear] in jig.toml"
                    .to_string(),
            )
        })?;

        Ok(Self {
            client: LinearClient::new(&profile.api_key),
            team,
            projects: linear_config.projects.clone(),
        })
    }
}

impl IssueProvider for LinearProvider {
    fn name(&self) -> &str {
        "linear"
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
        )?;

        // Client-side label filtering (all specified labels must match).
        if !filter.labels.is_empty() {
            issues.retain(|i| i.matches(filter));
        }

        // Sort by priority then id, consistent with FileProvider.
        issues.sort_by(|a, b| {
            let pa = a.priority.as_ref().map(|p| p.clone() as u8).unwrap_or(99);
            let pb = b.priority.as_ref().map(|p| p.clone() as u8).unwrap_or(99);
            pa.cmp(&pb).then_with(|| a.id.cmp(&b.id))
        });

        Ok(issues)
    }

    fn list_spawnable(&self, spawn_labels: &[String]) -> Result<Vec<Issue>> {
        // List planned issues, then filter to those with jig-auto label
        let all = self.list(&IssueFilter {
            status: Some(IssueStatus::Planned),
            ..Default::default()
        })?;
        Ok(all
            .into_iter()
            .filter(|i| i.auto)
            .filter(|i| {
                spawn_labels
                    .iter()
                    .all(|required| i.labels.iter().any(|l| l.eq_ignore_ascii_case(required)))
            })
            .filter(|i| self.is_spawnable_with_deps(i))
            .collect())
    }

    fn get(&self, id: &str) -> Result<Option<Issue>> {
        self.client.get_issue(id)
    }
}
