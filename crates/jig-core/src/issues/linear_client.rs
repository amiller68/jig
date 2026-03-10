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
query GetIssue($identifier: String!) {
  issueSearch(filter: { identifier: { eq: $identifier } }, first: 1) {
    nodes {
      identifier
      title
      description
      url
      priority
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
"#;

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
#[serde(rename_all = "camelCase")]
struct SearchData {
    issue_search: NodeList<RawIssue>,
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
            .filter(|r| r.relation_type == "blocks")
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
            auto: false,
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

    /// Get a single issue by its identifier (e.g. "ENG-123").
    pub fn get_issue(&self, identifier: &str) -> Result<Option<Issue>> {
        let variables = serde_json::json!({
            "identifier": identifier,
        });

        let request = GqlRequest {
            query: GET_ISSUE_QUERY,
            variables,
        };

        let data: SearchData = self.execute(&request)?;
        Ok(data.issue_search.nodes.into_iter().next().map(Issue::from))
    }
}
