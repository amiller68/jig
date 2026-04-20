use serde::Deserialize;

use super::super::error::LinearError;
use super::super::error::Result;
use super::super::request::LinearRequest;

const QUERY: &str = r#"
mutation CreateRelation($input: IssueRelationCreateInput!) {
  issueRelationCreate(input: $input) {
    success
    issueRelation { id }
  }
}
"#;

pub struct CreateRelation {
    pub issue_id: String,
    pub related_issue_id: String,
    pub relation_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRelationResponse {
    pub issue_relation_create: CreateRelationResult,
}

#[derive(Debug, Deserialize)]
pub struct CreateRelationResult {
    pub success: bool,
}

impl LinearRequest for CreateRelation {
    type Response = CreateRelationResponse;
    type Output = ();

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({
            "input": {
                "issueId": self.issue_id,
                "relatedIssueId": self.related_issue_id,
                "type": self.relation_type,
            }
        })
    }

    fn extract(response: Self::Response) -> Result<Self::Output> {
        if !response.issue_relation_create.success {
            return Err(LinearError::Other(
                "Linear refused to create relation".into(),
            ));
        }
        Ok(())
    }
}
