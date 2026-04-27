pub mod client;
pub mod error;
pub mod identifier;
pub mod mutations;
pub mod queries;
pub mod request;
pub mod types;

use crate::git::Branch;
use crate::issues::issue::{Issue, IssuePriority, IssueRef, IssueStatus};

pub use client::LinearClient;
pub use error::LinearError;
pub use request::LinearRequest;

fn map_status(state_type: &str) -> IssueStatus {
    match state_type {
        "triage" => IssueStatus::Triage,
        "backlog" => IssueStatus::Backlog,
        "unstarted" => IssueStatus::Planned,
        "started" => IssueStatus::InProgress,
        "completed" | "canceled" => IssueStatus::Complete,
        _ => IssueStatus::Planned,
    }
}

fn map_priority(p: u8) -> IssuePriority {
    match p {
        1 => IssuePriority::Urgent,
        2 => IssuePriority::High,
        3 => IssuePriority::Medium,
        4 => IssuePriority::Low,
        _ => IssuePriority::Medium,
    }
}

impl From<types::RawIssue> for Issue {
    fn from(raw: types::RawIssue) -> Self {
        let status = map_status(&raw.state.state_type);
        let priority = map_priority(raw.priority);
        let depends_on = raw
            .inverse_relations
            .nodes
            .into_iter()
            .filter(|r| r.relation_type == "blocks")
            .map(|r| r.issue.identifier.into())
            .collect();

        let children: Vec<IssueRef> = raw
            .children
            .nodes
            .into_iter()
            .map(|c| c.identifier.into())
            .collect();

        let body = match &raw.description {
            Some(desc) if !desc.is_empty() => format!("# {}\n\n{}", raw.title, desc),
            _ => format!("# {}", raw.title),
        };

        let labels: Vec<String> = raw.labels.nodes.into_iter().map(|l| l.name).collect();

        let parent: Option<IssueRef> = raw.parent.map(|p| p.identifier.into());

        let branch = raw
            .branch_name
            .map(Into::into)
            .unwrap_or_else(|| Branch::new(raw.identifier.to_lowercase()));

        let mut issue = Issue::new(raw.identifier, raw.title, status, priority, branch, body)
            .with_depends_on(depends_on)
            .with_children(children)
            .with_labels(labels);
        if let Some(p) = parent {
            issue = issue.with_parent(p);
        }
        issue
    }
}

impl LinearClient {
    pub fn viewer_id(&self) -> error::Result<String> {
        self.execute(queries::viewer::Viewer)
    }

    pub fn list_issues(
        &self,
        team_key: &str,
        projects: &[String],
        status: Option<&IssueStatus>,
        priority: Option<&IssuePriority>,
        assignee: Option<&str>,
    ) -> error::Result<Vec<Issue>> {
        let mut filter = serde_json::Map::new();

        filter.insert(
            "team".into(),
            serde_json::json!({ "key": { "eq": team_key } }),
        );

        if let Some(s) = status {
            let state_types = match s {
                IssueStatus::Triage => vec!["triage"],
                IssueStatus::Backlog => vec!["backlog"],
                IssueStatus::Planned => vec!["unstarted"],
                IssueStatus::InProgress => vec!["started"],
                IssueStatus::Complete => vec!["completed", "canceled"],
                IssueStatus::Blocked => vec!["started"],
            };
            filter.insert(
                "state".into(),
                serde_json::json!({ "type": { "in": state_types } }),
            );
        }

        if let Some(p) = priority {
            let num = match p {
                IssuePriority::Urgent => 1,
                IssuePriority::High => 2,
                IssuePriority::Medium => 3,
                IssuePriority::Low => 4,
            };
            filter.insert(
                "priority".into(),
                serde_json::json!({ "number": { "eq": num } }),
            );
        }

        if !projects.is_empty() {
            filter.insert(
                "project".into(),
                serde_json::json!({ "name": { "in": projects } }),
            );
        }

        if let Some(assignee_val) = assignee {
            if assignee_val.contains('@') {
                filter.insert(
                    "assignee".into(),
                    serde_json::json!({ "email": { "eq": assignee_val } }),
                );
            } else {
                filter.insert(
                    "assignee".into(),
                    serde_json::json!({ "id": { "eq": assignee_val } }),
                );
            }
        }

        let raw = self.execute(queries::list_issues::ListIssues {
            filter: serde_json::Value::Object(filter),
            first: 100,
        })?;
        Ok(raw.into_iter().map(Issue::from).collect())
    }

    fn resolve_state_id(&self, team_key: &str, new_status: &IssueStatus) -> error::Result<String> {
        let (target_state_type, target_state_name) = match new_status {
            IssueStatus::Triage => ("triage", "Triage"),
            IssueStatus::Backlog => ("backlog", "Backlog"),
            IssueStatus::Planned => ("unstarted", "Todo"),
            IssueStatus::InProgress => ("started", "In Progress"),
            IssueStatus::Complete => ("completed", "Done"),
            IssueStatus::Blocked => ("started", "In Progress"),
        };

        let filter = serde_json::json!({
            "team": { "key": { "eq": team_key } },
            "type": { "eq": target_state_type },
            "name": { "eqIgnoreCase": target_state_name }
        });

        let states = self.execute(queries::workflow_states::ListWorkflowStates {
            filter: filter.clone(),
        })?;

        let state = if states.is_empty() {
            let filter = serde_json::json!({
                "team": { "key": { "eq": team_key } },
                "type": { "eq": target_state_type }
            });
            let states = self.execute(queries::workflow_states::ListWorkflowStates { filter })?;
            states.into_iter().next().ok_or_else(|| {
                error::LinearError::Other(format!(
                    "no workflow state of type '{}' found for team {}",
                    target_state_type, team_key,
                ))
            })?
        } else {
            states.into_iter().next().ok_or_else(|| {
                error::LinearError::Other(format!(
                    "no workflow state of type '{}' found for team {}",
                    target_state_type, team_key,
                ))
            })?
        };

        Ok(state.id)
    }

    pub fn update_issue_status(
        &self,
        identifier: &str,
        team_key: &str,
        new_status: &IssueStatus,
    ) -> error::Result<()> {
        let state_id = self.resolve_state_id(team_key, new_status)?;
        let issue = self
            .get_issue(identifier)?
            .ok_or_else(|| error::LinearError::Other(format!("issue not found: {}", identifier)))?;

        self.execute(mutations::update_issue_status::UpdateIssueStatus {
            issue_id: issue.id().to_string(),
            state_id,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_issue(
        &self,
        identifier: &str,
        team_key: &str,
        title: Option<&str>,
        body: Option<&str>,
        priority: Option<&IssuePriority>,
        labels: &[String],
        project: Option<&str>,
        assignee: Option<&str>,
        parent: Option<&str>,
        remove_parent: bool,
    ) -> error::Result<()> {
        let issue = self
            .get_issue(identifier)?
            .ok_or_else(|| error::LinearError::Other(format!("issue not found: {}", identifier)))?;

        let mut input = serde_json::Map::new();

        if let Some(t) = title {
            input.insert("title".into(), serde_json::json!(t));
        }

        if let Some(desc) = body {
            input.insert("description".into(), serde_json::json!(desc));
        }

        if let Some(p) = priority {
            let num = match p {
                IssuePriority::Urgent => 1,
                IssuePriority::High => 2,
                IssuePriority::Medium => 3,
                IssuePriority::Low => 4,
            };
            input.insert("priority".into(), serde_json::json!(num));
        }

        if !labels.is_empty() {
            let label_ids = self.label_ids(team_key, labels)?;
            if !label_ids.is_empty() {
                input.insert("labelIds".into(), serde_json::json!(label_ids));
            }
        }

        if let Some(proj_name) = project {
            if let Some(proj_id) = self.project_id(proj_name)? {
                input.insert("projectId".into(), serde_json::json!(proj_id));
            }
        }

        if let Some(assignee_val) = assignee {
            input.insert("assigneeId".into(), serde_json::json!(assignee_val));
        }

        if remove_parent {
            input.insert("parentId".into(), serde_json::Value::Null);
        } else if let Some(parent_id) = parent {
            let parent_issue = self.get_issue(parent_id)?.ok_or_else(|| {
                error::LinearError::Other(format!("parent issue not found: {}", parent_id))
            })?;
            input.insert(
                "parentId".into(),
                serde_json::json!(parent_issue.id().to_string()),
            );
        }

        if input.is_empty() {
            return Ok(());
        }

        self.execute(mutations::update_issue::UpdateIssue {
            issue_id: issue.id().to_string(),
            input: serde_json::Value::Object(input),
        })
    }

    pub fn team_id(&self, team_key: &str) -> error::Result<String> {
        self.execute(queries::team_by_key::TeamByKey {
            team_key: team_key.to_string(),
        })
    }

    pub fn label_ids(&self, team_key: &str, names: &[String]) -> error::Result<Vec<String>> {
        if names.is_empty() {
            return Ok(vec![]);
        }

        let raw_labels = self.execute(queries::labels_by_team::LabelsByTeam {
            names: names.to_vec(),
        })?;

        let mut ids: Vec<String> = Vec::with_capacity(names.len());
        for wanted in names {
            let candidates: Vec<&queries::labels_by_team::RawLabelWithId> = raw_labels
                .iter()
                .filter(|l| l.name.eq_ignore_ascii_case(wanted))
                .collect();

            let chosen = candidates
                .iter()
                .find(|l| l.team.as_ref().map(|t| t.key.as_str()) == Some(team_key))
                .or_else(|| candidates.iter().find(|l| l.team.is_none()));

            if let Some(label) = chosen {
                ids.push(label.id.clone());
            }
        }
        Ok(ids)
    }

    pub fn project_id(&self, name: &str) -> error::Result<Option<String>> {
        self.execute(queries::projects_by_name::ProjectsByName {
            name: name.to_string(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_issue(
        &self,
        team_key: &str,
        title: &str,
        body: Option<&str>,
        priority: Option<&IssuePriority>,
        labels: &[String],
        project: Option<&str>,
        assignee: Option<&str>,
        parent: Option<&str>,
        initial_status: Option<&IssueStatus>,
    ) -> error::Result<String> {
        let team_id = self.team_id(team_key)?;

        let mut input = serde_json::Map::new();
        input.insert("teamId".into(), serde_json::json!(team_id));
        input.insert("title".into(), serde_json::json!(title));

        if let Some(desc) = body {
            if !desc.is_empty() {
                input.insert("description".into(), serde_json::json!(desc));
            }
        }

        if let Some(p) = priority {
            let num = match p {
                IssuePriority::Urgent => 1,
                IssuePriority::High => 2,
                IssuePriority::Medium => 3,
                IssuePriority::Low => 4,
            };
            input.insert("priority".into(), serde_json::json!(num));
        }

        let label_ids = self.label_ids(team_key, labels)?;
        if !label_ids.is_empty() {
            input.insert("labelIds".into(), serde_json::json!(label_ids));
        }

        if let Some(proj_name) = project {
            if let Some(proj_id) = self.project_id(proj_name)? {
                input.insert("projectId".into(), serde_json::json!(proj_id));
            }
        }

        if let Some(assignee_val) = assignee {
            input.insert("assigneeId".into(), serde_json::json!(assignee_val));
        }

        if let Some(parent_id) = parent {
            let parent_issue = self.get_issue(parent_id)?.ok_or_else(|| {
                error::LinearError::Other(format!("parent issue not found: {}", parent_id))
            })?;
            input.insert(
                "parentId".into(),
                serde_json::json!(parent_issue.id().to_string()),
            );
        }

        if let Some(status) = initial_status {
            let state_id = self.resolve_state_id(team_key, status)?;
            input.insert("stateId".into(), serde_json::json!(state_id));
        }

        self.execute(mutations::create_issue::CreateIssue {
            input: serde_json::Value::Object(input),
        })
    }

    fn resolve_issue_uuid(&self, identifier: &str) -> error::Result<String> {
        let (team_key, number) = identifier::parse_identifier(identifier).ok_or_else(|| {
            error::LinearError::Other(format!("invalid issue identifier: {identifier}"))
        })?;

        let filter = serde_json::json!({
            "team": { "key": { "eq": team_key } },
            "number": { "eq": number }
        });

        let raw = self.execute(queries::get_issue::GetIssue { filter })?;
        raw.map(|r| r.id)
            .ok_or_else(|| error::LinearError::Other(format!("issue not found: {identifier}")))
    }

    pub fn create_blocked_by_relation(
        &self,
        issue_identifier: &str,
        blocker_identifier: &str,
    ) -> error::Result<()> {
        let issue_uuid = self.resolve_issue_uuid(issue_identifier)?;
        let blocker_uuid = self.resolve_issue_uuid(blocker_identifier)?;

        self.execute(mutations::create_relation::CreateRelation {
            issue_id: blocker_uuid,
            related_issue_id: issue_uuid,
            relation_type: "blocks".into(),
        })
    }

    pub fn remove_blocked_by_relation(
        &self,
        issue_identifier: &str,
        blocker_identifier: &str,
    ) -> error::Result<()> {
        let (team_key, number) =
            identifier::parse_identifier(issue_identifier).ok_or_else(|| {
                error::LinearError::Other(format!("invalid issue identifier: {issue_identifier}"))
            })?;

        let filter = serde_json::json!({
            "team": { "key": { "eq": team_key } },
            "number": { "eq": number }
        });

        let raw_issue = self
            .execute(queries::get_issue::GetIssue { filter })?
            .ok_or_else(|| {
                error::LinearError::Other(format!("issue not found: {issue_identifier}"))
            })?;

        let relation_id = raw_issue
            .inverse_relations
            .nodes
            .into_iter()
            .find(|r| {
                r.relation_type == "blocks"
                    && r.issue.identifier.eq_ignore_ascii_case(blocker_identifier)
            })
            .and_then(|r| r.id)
            .ok_or_else(|| {
                error::LinearError::Other(format!(
                    "no 'blocked by' relation found between {} and {}",
                    issue_identifier, blocker_identifier,
                ))
            })?;

        self.execute(mutations::delete_relation::DeleteRelation { id: relation_id })
    }

    pub fn get_issue(&self, identifier: &str) -> error::Result<Option<Issue>> {
        let (team_key, number) = identifier::parse_identifier(identifier).ok_or_else(|| {
            error::LinearError::Other(format!("invalid issue identifier: {identifier}"))
        })?;

        let filter = serde_json::json!({
            "team": { "key": { "eq": team_key } },
            "number": { "eq": number }
        });

        let raw = self.execute(queries::get_issue::GetIssue { filter })?;
        Ok(raw.map(Issue::from))
    }
}
