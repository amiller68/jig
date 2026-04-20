use serde::Deserialize;

use super::super::error::LinearError;
use super::super::error::Result;
use super::super::request::LinearRequest;

const QUERY: &str = r#"
mutation DeleteRelation($id: String!) {
  issueRelationDelete(id: $id) {
    success
  }
}
"#;

pub struct DeleteRelation {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRelationResponse {
    pub issue_relation_delete: DeleteRelationResult,
}

#[derive(Debug, Deserialize)]
pub struct DeleteRelationResult {
    pub success: bool,
}

impl LinearRequest for DeleteRelation {
    type Response = DeleteRelationResponse;
    type Output = ();

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({ "id": self.id })
    }

    fn extract(response: Self::Response) -> Result<Self::Output> {
        if !response.issue_relation_delete.success {
            return Err(LinearError::Other(
                "Linear refused to delete relation".into(),
            ));
        }
        Ok(())
    }
}
