use serde::Deserialize;

use super::super::error::Result;
use super::super::request::LinearRequest;
use super::super::types::NodeList;

const QUERY: &str = r#"
query LabelsByTeam($filter: IssueLabelFilter) {
  issueLabels(filter: $filter, first: 250) {
    nodes {
      id
      name
      team { key }
    }
  }
}
"#;

pub struct LabelsByTeam {
    pub names: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelsResponse {
    pub issue_labels: NodeList<RawLabelWithId>,
}

#[derive(Debug, Deserialize)]
pub struct RawLabelWithId {
    pub id: String,
    pub name: String,
    pub team: Option<RawLabelTeam>,
}

#[derive(Debug, Deserialize)]
pub struct RawLabelTeam {
    pub key: String,
}

impl LinearRequest for LabelsByTeam {
    type Response = LabelsResponse;
    type Output = Vec<RawLabelWithId>;

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        let name_list: Vec<&str> = self.names.iter().map(|s| s.as_str()).collect();
        serde_json::json!({
            "filter": { "name": { "in": name_list } }
        })
    }

    fn extract(response: Self::Response) -> Result<Self::Output> {
        Ok(response.issue_labels.nodes)
    }
}
