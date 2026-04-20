use serde::Deserialize;

use super::super::error::{LinearError, Result};
use super::super::request::LinearRequest;
use super::super::types::NodeList;

const QUERY: &str = r#"
query TeamByKey($filter: TeamFilter) {
  teams(filter: $filter, first: 1) {
    nodes {
      id
    }
  }
}
"#;

pub struct TeamByKey {
    pub team_key: String,
}

#[derive(Debug, Deserialize)]
pub struct TeamsResponse {
    pub teams: NodeList<RawTeamId>,
}

#[derive(Debug, Deserialize)]
pub struct RawTeamId {
    pub id: String,
}

impl LinearRequest for TeamByKey {
    type Response = TeamsResponse;
    type Output = String;

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({
            "filter": { "key": { "eq": self.team_key } }
        })
    }

    fn extract(response: Self::Response) -> Result<Self::Output> {
        response
            .teams
            .nodes
            .into_iter()
            .next()
            .map(|t| t.id)
            .ok_or_else(|| LinearError::Other("team not found".into()))
    }
}
