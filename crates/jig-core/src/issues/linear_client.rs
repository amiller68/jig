//! Thin GraphQL client for the Linear API over `ureq`.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::types::{Issue, IssuePriority, IssueStatus};

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

/// Minimal sync client for Linear's GraphQL API.
pub struct LinearClient {
    api_key: String,
}

// -- GraphQL query constants --------------------------------------------------

const LIST_ISSUES_QUERY: &str = r#"
query ListIssues($filter: IssueFilter, $first: Int) {
  issues(filter: $filter, first: $first) {
    nodes {
      identifier
      title
      description
      url
      priority
      branchName
      state { type }
      project { name }
      team { name }
      children { nodes { identifier } }
      labels { nodes { name } }
      relations {
        nodes {
          type
          relatedIssue { identifier }
        }
      }
    }
  }
}
"#;

const UPDATE_ISSUE_STATUS_MUTATION: &str = r#"
mutation UpdateIssueStatus($issueId: String!, $stateId: String!) {
  issueUpdate(id: $issueId, input: { stateId: $stateId }) {
    success
    issue {
      identifier
      state { type }
    }
  }
}
"#;

const LIST_WORKFLOW_STATES_QUERY: &str = r#"
query ListWorkflowStates($filter: WorkflowStateFilter) {
  workflowStates(filter: $filter) {
    nodes {
      id
      name
      type
    }
  }
}
"#;

const GET_ISSUE_QUERY: &str = r#"
query GetIssue($filter: IssueFilter, $first: Int) {
  issues(filter: $filter, first: $first) {
    nodes {
      identifier
      title
      description
      url
      priority
      branchName
      state { type }
      project { name }
      team { name }
      children { nodes { identifier } }
      labels { nodes { name } }
      relations {
        nodes {
          type
          relatedIssue { identifier }
        }
      }
    }
  }
}
"#;

const VIEWER_QUERY: &str = r#"
query Viewer {
  viewer {
    id
    email
  }
}
"#;

const UPDATE_ISSUE_MUTATION: &str = r#"
mutation UpdateIssue($issueId: String!, $input: IssueUpdateInput!) {
  issueUpdate(id: $issueId, input: $input) {
    success
    issue {
      identifier
    }
  }
}
"#;

const CREATE_ISSUE_MUTATION: &str = r#"
mutation CreateIssue($input: IssueCreateInput!) {
  issueCreate(input: $input) {
    success
    issue {
      identifier
    }
  }
}
"#;

const TEAM_ID_QUERY: &str = r#"
query TeamByKey($filter: TeamFilter) {
  teams(filter: $filter, first: 1) {
    nodes {
      id
    }
  }
}
"#;

const LABELS_QUERY: &str = r#"
query LabelsByTeam($filter: IssueLabelFilter) {
  issueLabels(filter: $filter, first: 100) {
    nodes {
      id
      name
    }
  }
}
"#;

const PROJECTS_QUERY: &str = r#"
query ProjectsByName($filter: ProjectFilter) {
  projects(filter: $filter, first: 1) {
    nodes {
      id
    }
  }
}
"#;

/// Parse an identifier like "AUT-62" into (team_key, number).
///
/// Also accepts branch-format strings like `feature/aut-5044-refactor-foo`
/// by extracting the embedded Linear identifier first.
fn parse_identifier(identifier: &str) -> Option<(String, i64)> {
    // Try to extract a Linear identifier from branch-format input
    let canonical = super::naming::extract_linear_identifier(identifier)?;
    let (team, num) = canonical.rsplit_once('-')?;
    let n = num.parse::<i64>().ok()?;
    Some((team.to_string(), n))
}

// -- Raw API response types ---------------------------------------------------

#[derive(Debug, Deserialize)]
struct GqlResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GqlError>>,
}

#[derive(Debug, Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct ListData {
    issues: NodeList<RawIssue>,
}

#[derive(Debug, Deserialize)]
struct ViewerData {
    viewer: RawViewer,
}

#[derive(Debug, Deserialize)]
struct RawViewer {
    id: String,
    #[allow(dead_code)]
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowStatesData {
    workflow_states: NodeList<RawWorkflowState>,
}

#[derive(Debug, Deserialize)]
struct RawWorkflowState {
    id: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    state_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateIssueData {
    #[allow(dead_code)]
    issue_update: UpdateIssueResult,
}

#[derive(Debug, Deserialize)]
struct UpdateIssueResult {
    #[allow(dead_code)]
    success: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateIssueData {
    issue_create: CreateIssueResult,
}

#[derive(Debug, Deserialize)]
struct CreateIssueResult {
    #[allow(dead_code)]
    success: bool,
    issue: RawCreatedIssue,
}

#[derive(Debug, Deserialize)]
struct RawCreatedIssue {
    identifier: String,
}

#[derive(Debug, Deserialize)]
struct TeamsData {
    teams: NodeList<RawTeamId>,
}

#[derive(Debug, Deserialize)]
struct RawTeamId {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabelsData {
    issue_labels: NodeList<RawLabelWithId>,
}

#[derive(Debug, Deserialize)]
struct RawLabelWithId {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct ProjectsData {
    projects: NodeList<RawProjectId>,
}

#[derive(Debug, Deserialize)]
struct RawProjectId {
    id: String,
}

#[derive(Debug, Deserialize)]
struct NodeList<T> {
    nodes: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIssue {
    identifier: String,
    title: String,
    description: Option<String>,
    url: String,
    priority: u8,
    branch_name: Option<String>,
    state: RawState,
    project: Option<RawProject>,
    team: RawTeam,
    children: NodeList<RawChildRef>,
    labels: NodeList<RawLabel>,
    relations: NodeList<RawRelation>,
}

#[derive(Debug, Deserialize)]
struct RawState {
    #[serde(rename = "type")]
    state_type: String,
}

#[derive(Debug, Deserialize)]
struct RawProject {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawTeam {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawChildRef {
    identifier: String,
}

#[derive(Debug, Deserialize)]
struct RawLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRelation {
    #[serde(rename = "type")]
    relation_type: String,
    related_issue: RawChildRef,
}

// -- Request body -------------------------------------------------------------

#[derive(Serialize)]
struct GqlRequest {
    query: &'static str,
    variables: serde_json::Value,
}

// -- Conversion ---------------------------------------------------------------

fn map_status(state_type: &str) -> IssueStatus {
    match state_type {
        "backlog" | "unstarted" => IssueStatus::Planned,
        "started" => IssueStatus::InProgress,
        "completed" | "canceled" => IssueStatus::Complete,
        _ => IssueStatus::Planned,
    }
}

fn map_priority(p: u8) -> Option<IssuePriority> {
    match p {
        1 => Some(IssuePriority::Urgent),
        2 => Some(IssuePriority::High),
        3 => Some(IssuePriority::Medium),
        4 => Some(IssuePriority::Low),
        _ => None,
    }
}

impl From<RawIssue> for Issue {
    fn from(raw: RawIssue) -> Self {
        let status = map_status(&raw.state.state_type);
        let priority = map_priority(raw.priority);
        let category = raw
            .project
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| raw.team.name.clone());

        let depends_on: Vec<String> = raw
            .relations
            .nodes
            .into_iter()
            .filter(|r| r.relation_type == "is_blocked_by")
            .map(|r| r.related_issue.identifier)
            .collect();

        let children: Vec<String> = raw
            .children
            .nodes
            .into_iter()
            .map(|c| c.identifier)
            .collect();

        let body = match &raw.description {
            Some(desc) if !desc.is_empty() => format!("# {}\n\n{}", raw.title, desc),
            _ => format!("# {}", raw.title),
        };

        let labels: Vec<String> = raw.labels.nodes.into_iter().map(|l| l.name).collect();

        Issue {
            id: raw.identifier,
            title: raw.title,
            status,
            priority,
            category: Some(category),
            depends_on,
            body,
            source: raw.url,
            children,
            labels,
            branch_name: raw.branch_name,
        }
    }
}

// -- Client implementation ----------------------------------------------------

impl LinearClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
        }
    }

    /// Fetch the authenticated user's ID via the `viewer` query.
    /// Used to resolve `assignee = "me"`.
    pub fn viewer_id(&self) -> Result<String> {
        let request = GqlRequest {
            query: VIEWER_QUERY,
            variables: serde_json::json!({}),
        };
        let data: ViewerData = self.execute(&request)?;
        Ok(data.viewer.id)
    }

    fn execute<T: serde::de::DeserializeOwned>(&self, request: &GqlRequest) -> Result<T> {
        let response = ureq::post(LINEAR_API_URL)
            .config()
            .http_status_as_error(false)
            .build()
            .header("Authorization", &self.api_key)
            .header("Content-Type", "application/json")
            .send_json(request)
            .map_err(|e| Error::Linear(format!("HTTP request failed: {}", e)))?;

        let status = response.status();

        let body = response
            .into_body()
            .read_to_string()
            .map_err(|e| Error::Linear(format!("failed to read response: {}", e)))?;

        if status.as_u16() >= 400 {
            return Err(Error::Linear(format!("HTTP {}: {}", status.as_u16(), body)));
        }

        let gql: GqlResponse<T> = serde_json::from_str(&body).map_err(|e| {
            Error::Linear(format!("failed to parse response: {} — body: {}", e, body))
        })?;

        if let Some(errors) = gql.errors {
            let msgs: Vec<String> = errors.into_iter().map(|e| e.message).collect();
            return Err(Error::Linear(msgs.join("; ")));
        }

        gql.data
            .ok_or_else(|| Error::Linear("no data in response".into()))
    }

    /// List issues for a team, optionally filtering by project, state, priority, and assignee.
    pub fn list_issues(
        &self,
        team_key: &str,
        projects: &[String],
        status: Option<&IssueStatus>,
        priority: Option<&IssuePriority>,
        assignee: Option<&str>,
    ) -> Result<Vec<Issue>> {
        let mut filter = serde_json::Map::new();

        // Team filter
        filter.insert(
            "team".into(),
            serde_json::json!({ "key": { "eq": team_key } }),
        );

        if let Some(s) = status {
            let state_types = match s {
                IssueStatus::Planned => vec!["backlog", "unstarted"],
                IssueStatus::InProgress => vec!["started"],
                IssueStatus::Complete => vec!["completed", "canceled"],
                IssueStatus::Blocked => vec!["started"], // no direct mapping; return started
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

        // Assignee filter — the value is a pre-resolved user ID (from "me")
        // or an email address.
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

        let variables = serde_json::json!({
            "filter": filter,
            "first": 100,
        });

        let request = GqlRequest {
            query: LIST_ISSUES_QUERY,
            variables,
        };

        let data: ListData = self.execute(&request)?;
        Ok(data.issues.nodes.into_iter().map(Issue::from).collect())
    }

    /// Update an issue's workflow state by transitioning to a state of the
    /// given type. `team_key` is needed to look up workflow states.
    pub fn update_issue_status(
        &self,
        identifier: &str,
        team_key: &str,
        new_status: &IssueStatus,
    ) -> Result<()> {
        // Map IssueStatus to Linear state type(s) we want to transition to
        let target_state_type = match new_status {
            IssueStatus::Planned => "unstarted",
            IssueStatus::InProgress => "started",
            IssueStatus::Complete => "completed",
            IssueStatus::Blocked => "started", // Linear has no "blocked" state type
        };

        // Find the workflow state ID for the target type
        let filter = serde_json::json!({
            "team": { "key": { "eq": team_key } },
            "type": { "eq": target_state_type }
        });
        let variables = serde_json::json!({ "filter": filter });
        let request = GqlRequest {
            query: LIST_WORKFLOW_STATES_QUERY,
            variables,
        };

        let data: WorkflowStatesData = self.execute(&request)?;
        let state = data.workflow_states.nodes.first().ok_or_else(|| {
            Error::Linear(format!(
                "no workflow state of type '{}' found for team {}",
                target_state_type, team_key,
            ))
        })?;

        // Get issue's internal ID (we need to fetch it first)
        let issue = self
            .get_issue(identifier)?
            .ok_or_else(|| Error::Linear(format!("issue not found: {}", identifier)))?;
        // Use identifier as the ID for the mutation
        let variables = serde_json::json!({
            "issueId": issue.id,
            "stateId": state.id,
        });
        let request = GqlRequest {
            query: UPDATE_ISSUE_STATUS_MUTATION,
            variables,
        };

        let _data: UpdateIssueData = self.execute(&request)?;
        Ok(())
    }

    /// Update an issue's fields (title, description, priority, labels, project).
    ///
    /// Only fields that are `Some` / non-empty are sent in the mutation.
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
    ) -> Result<()> {
        let issue = self
            .get_issue(identifier)?
            .ok_or_else(|| Error::Linear(format!("issue not found: {}", identifier)))?;

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

        if input.is_empty() {
            return Ok(());
        }

        let variables = serde_json::json!({
            "issueId": issue.id,
            "input": input,
        });
        let request = GqlRequest {
            query: UPDATE_ISSUE_MUTATION,
            variables,
        };

        let _data: UpdateIssueData = self.execute(&request)?;
        Ok(())
    }

    /// Resolve a team key (e.g. "AUT") to its internal UUID.
    pub fn team_id(&self, team_key: &str) -> Result<String> {
        let filter = serde_json::json!({ "key": { "eq": team_key } });
        let variables = serde_json::json!({ "filter": filter });
        let request = GqlRequest {
            query: TEAM_ID_QUERY,
            variables,
        };
        let data: TeamsData = self.execute(&request)?;
        data.teams
            .nodes
            .into_iter()
            .next()
            .map(|t| t.id)
            .ok_or_else(|| Error::Linear(format!("team not found: {}", team_key)))
    }

    /// Resolve label names to their IDs for a given team.
    pub fn label_ids(&self, team_key: &str, names: &[String]) -> Result<Vec<String>> {
        if names.is_empty() {
            return Ok(vec![]);
        }
        let filter = serde_json::json!({
            "team": { "key": { "eq": team_key } }
        });
        let variables = serde_json::json!({ "filter": filter });
        let request = GqlRequest {
            query: LABELS_QUERY,
            variables,
        };
        let data: LabelsData = self.execute(&request)?;
        let ids: Vec<String> = data
            .issue_labels
            .nodes
            .into_iter()
            .filter(|l| names.iter().any(|n| n.eq_ignore_ascii_case(&l.name)))
            .map(|l| l.id)
            .collect();
        Ok(ids)
    }

    /// Resolve a project name to its ID.
    pub fn project_id(&self, name: &str) -> Result<Option<String>> {
        let filter = serde_json::json!({
            "name": { "eq": name }
        });
        let variables = serde_json::json!({ "filter": filter });
        let request = GqlRequest {
            query: PROJECTS_QUERY,
            variables,
        };
        let data: ProjectsData = self.execute(&request)?;
        Ok(data.projects.nodes.into_iter().next().map(|p| p.id))
    }

    /// Create a new issue in Linear.
    ///
    /// Returns the created issue's identifier (e.g. "AUT-1234").
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
    ) -> Result<String> {
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

        let variables = serde_json::json!({ "input": input });
        let request = GqlRequest {
            query: CREATE_ISSUE_MUTATION,
            variables,
        };

        let data: CreateIssueData = self.execute(&request)?;
        Ok(data.issue_create.issue.identifier)
    }

    /// Get a single issue by its identifier (e.g. "AUT-123").
    pub fn get_issue(&self, identifier: &str) -> Result<Option<Issue>> {
        let (team_key, number) = parse_identifier(identifier)
            .ok_or_else(|| Error::Linear(format!("invalid issue identifier: {identifier}")))?;

        let filter = serde_json::json!({
            "team": { "key": { "eq": team_key } },
            "number": { "eq": number }
        });

        let variables = serde_json::json!({
            "filter": filter,
            "first": 1,
        });

        let request = GqlRequest {
            query: GET_ISSUE_QUERY,
            variables,
        };

        let data: ListData = self.execute(&request)?;
        Ok(data.issues.nodes.into_iter().next().map(Issue::from))
    }
}
