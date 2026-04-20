use serde::Deserialize;

use super::super::error::Result;
use super::super::request::LinearRequest;
use super::super::types::NodeList;

const QUERY: &str = r#"
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

pub struct ListWorkflowStates {
    pub filter: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStatesResponse {
    pub workflow_states: NodeList<RawWorkflowState>,
}

#[derive(Debug, Deserialize)]
pub struct RawWorkflowState {
    pub id: String,
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    pub state_type: String,
}

impl LinearRequest for ListWorkflowStates {
    type Response = WorkflowStatesResponse;
    type Output = Vec<RawWorkflowState>;

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({ "filter": self.filter })
    }

    fn extract(response: Self::Response) -> Result<Self::Output> {
        Ok(response.workflow_states.nodes)
    }
}
