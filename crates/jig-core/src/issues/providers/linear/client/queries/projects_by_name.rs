use serde::Deserialize;

use super::super::error::Result;
use super::super::request::LinearRequest;
use super::super::types::NodeList;

const QUERY: &str = r#"
query ProjectsByName($filter: ProjectFilter) {
  projects(filter: $filter, first: 1) {
    nodes {
      id
    }
  }
}
"#;

pub struct ProjectsByName {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectsResponse {
    pub projects: NodeList<RawProjectId>,
}

#[derive(Debug, Deserialize)]
pub struct RawProjectId {
    pub id: String,
}

impl LinearRequest for ProjectsByName {
    type Response = ProjectsResponse;
    type Output = Option<String>;

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({
            "filter": { "name": { "eq": self.name } }
        })
    }

    fn extract(response: Self::Response) -> Result<Self::Output> {
        Ok(response.projects.nodes.into_iter().next().map(|p| p.id))
    }
}
