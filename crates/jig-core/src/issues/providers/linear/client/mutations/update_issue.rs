use serde::Deserialize;

use super::super::error::Result;
use super::super::request::LinearRequest;

const QUERY: &str = r#"
mutation UpdateIssue($issueId: String!, $input: IssueUpdateInput!) {
  issueUpdate(id: $issueId, input: $input) {
    success
    issue {
      identifier
    }
  }
}
"#;

pub struct UpdateIssue {
    pub issue_id: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIssueResponse {
    #[allow(dead_code)]
    pub issue_update: UpdateResult,
}

#[derive(Debug, Deserialize)]
pub struct UpdateResult {
    #[allow(dead_code)]
    pub success: bool,
}

impl LinearRequest for UpdateIssue {
    type Response = UpdateIssueResponse;
    type Output = ();

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({
            "issueId": self.issue_id,
            "input": self.input,
        })
    }

    fn extract(_response: Self::Response) -> Result<Self::Output> {
        Ok(())
    }
}
