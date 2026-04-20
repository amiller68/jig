use serde::Deserialize;

use super::super::error::Result;
use super::super::request::LinearRequest;

const QUERY: &str = r#"
mutation CreateIssue($input: IssueCreateInput!) {
  issueCreate(input: $input) {
    success
    issue {
      identifier
    }
  }
}
"#;

pub struct CreateIssue {
    pub input: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssueResponse {
    pub issue_create: CreateIssueResult,
}

#[derive(Debug, Deserialize)]
pub struct CreateIssueResult {
    #[allow(dead_code)]
    pub success: bool,
    pub issue: RawCreatedIssue,
}

#[derive(Debug, Deserialize)]
pub struct RawCreatedIssue {
    pub identifier: String,
}

impl LinearRequest for CreateIssue {
    type Response = CreateIssueResponse;
    type Output = String;

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({ "input": self.input })
    }

    fn extract(response: Self::Response) -> Result<Self::Output> {
        Ok(response.issue_create.issue.identifier)
    }
}
