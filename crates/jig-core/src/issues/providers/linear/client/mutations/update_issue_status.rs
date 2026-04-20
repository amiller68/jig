use serde::Deserialize;

use super::super::error::Result;
use super::super::request::LinearRequest;

const QUERY: &str = r#"
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

pub struct UpdateIssueStatus {
    pub issue_id: String,
    pub state_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIssueStatusResponse {
    #[allow(dead_code)]
    pub issue_update: UpdateResult,
}

#[derive(Debug, Deserialize)]
pub struct UpdateResult {
    #[allow(dead_code)]
    pub success: bool,
}

impl LinearRequest for UpdateIssueStatus {
    type Response = UpdateIssueStatusResponse;
    type Output = ();

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({
            "issueId": self.issue_id,
            "stateId": self.state_id,
        })
    }

    fn extract(_response: Self::Response) -> Result<Self::Output> {
        Ok(())
    }
}
