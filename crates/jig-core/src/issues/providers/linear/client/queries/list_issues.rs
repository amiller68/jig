use serde::Deserialize;

use super::super::error::Result;
use super::super::request::LinearRequest;
use super::super::types::{NodeList, RawIssue};

const QUERY: &str = r#"
query ListIssues($filter: IssueFilter, $first: Int) {
  issues(filter: $filter, first: $first) {
    nodes {
      id
      identifier
      title
      description
      url
      priority
      branchName
      state { type }
      project { name }
      team { name }
      parent { identifier title description branchName state { type } }
      children { nodes { identifier branchName state { type } } }
      labels { nodes { name } }
      inverseRelations {
        nodes {
          id
          type
          issue { identifier }
          relatedIssue { identifier }
        }
      }
    }
  }
}
"#;

pub struct ListIssues {
    pub filter: serde_json::Value,
    pub first: i32,
}

#[derive(Debug, Deserialize)]
pub struct ListIssuesResponse {
    pub issues: NodeList<RawIssue>,
}

impl LinearRequest for ListIssues {
    type Response = ListIssuesResponse;
    type Output = Vec<RawIssue>;

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({
            "filter": self.filter,
            "first": self.first,
        })
    }

    fn extract(response: Self::Response) -> Result<Self::Output> {
        Ok(response.issues.nodes)
    }
}
