//! Thin GraphQL client for the Linear API over `ureq`.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::types::{CreateIssueInput, CreatedIssue, Issue, IssuePriority, IssueStatus};

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

const GET_TEAM_QUERY: &str = r#"
query GetTeam($filter: TeamFilter) {
  teams(filter: $filter) {
    nodes { id key }
  }
}
"#;

const LIST_LABELS_QUERY: &str = r#"
query ListLabels($filter: IssueLabelFilter, $first: Int) {
  issueLabels(filter: $filter, first: $first) {
    nodes { id name }
  }
}
"#;

const CREATE_ISSUE_MUTATION: &str = r#"
mutation CreateIssue($input: IssueCreateInput!) {
  issueCreate(input: $input) {
    success
    issue {
      identifier
      url
    }
  }
}
"#;

/// Parse an identifier like "AUT-62" into (team_key, number).
fn parse_identifier(identifier: &str) -> Option<(&str, i64)> {
    let (team, num) = identifier.rsplit_once('-')?;
    let n = num.parse::<i64>().ok()?;
    Some((team, n))
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

// -- Additional response types for create/team/label queries ------------------

#[derive(Debug, Deserialize)]
struct TeamData {
    teams: NodeList<RawTeamNode>,
}

#[derive(Debug, Deserialize)]
struct RawTeamNode {
    id: String,
    key: String,
}

#[derive(Debug, Deserialize)]
struct LabelData {
    #[serde(rename = "issueLabels")]
    issue_labels: NodeList<RawLabelNode>,
}

#[derive(Debug, Deserialize)]
struct RawLabelNode {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CreateIssueData {
    #[serde(rename = "issueCreate")]
    issue_create: RawIssueCreateResult,
}

#[derive(Debug, Deserialize)]
struct RawIssueCreateResult {
    success: bool,
    issue: Option<RawCreatedIssue>,
}

#[derive(Debug, Deserialize)]
struct RawCreatedIssue {
    identifier: String,
    url: String,
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

    /// List issues for a team, optionally filtering by project, state, and priority.
    pub fn list_issues(
        &self,
        team_key: &str,
        projects: &[String],
        status: Option<&IssueStatus>,
        priority: Option<&IssuePriority>,
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

    /// Get the team UUID for a team key (e.g. "ENG").
    pub fn get_team_id(&self, team_key: &str) -> Result<String> {
        let request = GqlRequest {
            query: GET_TEAM_QUERY,
            variables: serde_json::json!({
                "filter": { "key": { "eq": team_key } }
            }),
        };

        let data: TeamData = self.execute(&request)?;
        data.teams
            .nodes
            .into_iter()
            .find(|t| t.key == team_key)
            .map(|t| t.id)
            .ok_or_else(|| Error::Linear(format!("team '{}' not found", team_key)))
    }

    /// List labels for a team, returning (id, name) pairs.
    pub fn list_team_labels(&self, team_id: &str) -> Result<Vec<(String, String)>> {
        let request = GqlRequest {
            query: LIST_LABELS_QUERY,
            variables: serde_json::json!({
                "filter": { "team": { "id": { "eq": team_id } } },
                "first": 200,
            }),
        };

        let data: LabelData = self.execute(&request)?;
        Ok(data
            .issue_labels
            .nodes
            .into_iter()
            .map(|l| (l.id, l.name))
            .collect())
    }

    /// Create a new issue in the given team.
    ///
    /// Resolves label names to IDs automatically.
    pub fn create_issue(&self, team_key: &str, input: &CreateIssueInput) -> Result<CreatedIssue> {
        let team_id = self.get_team_id(team_key)?;

        // Resolve label names to IDs
        let label_ids = if input.labels.is_empty() {
            vec![]
        } else {
            let all_labels = self.list_team_labels(&team_id)?;
            let mut ids = Vec::new();
            for name in &input.labels {
                let id = all_labels
                    .iter()
                    .find(|(_, n)| n.eq_ignore_ascii_case(name))
                    .map(|(id, _)| id.clone())
                    .ok_or_else(|| {
                        Error::Linear(format!("label '{}' not found in team '{}'", name, team_key))
                    })?;
                ids.push(id);
            }
            ids
        };

        let mut issue_input = serde_json::json!({
            "teamId": team_id,
            "title": input.title,
            "description": input.body,
        });

        if !label_ids.is_empty() {
            issue_input["labelIds"] = serde_json::json!(label_ids);
        }

        let request = GqlRequest {
            query: CREATE_ISSUE_MUTATION,
            variables: serde_json::json!({ "input": issue_input }),
        };

        let data: CreateIssueData = self.execute(&request)?;
        if !data.issue_create.success {
            return Err(Error::Linear("issue creation failed".into()));
        }

        let raw = data
            .issue_create
            .issue
            .ok_or_else(|| Error::Linear("no issue returned after creation".into()))?;

        Ok(CreatedIssue {
            id: raw.identifier,
            url: raw.url,
        })
    }
}
