use serde::Deserialize;

use super::super::error::Result;
use super::super::request::LinearRequest;

const QUERY: &str = r#"
query Viewer {
  viewer {
    id
    email
  }
}
"#;

pub struct Viewer;

#[derive(Debug, Deserialize)]
pub struct ViewerResponse {
    pub viewer: RawViewer,
}

#[derive(Debug, Deserialize)]
pub struct RawViewer {
    pub id: String,
    #[allow(dead_code)]
    pub email: Option<String>,
}

impl LinearRequest for Viewer {
    type Response = ViewerResponse;
    type Output = String;

    const QUERY: &'static str = QUERY;

    fn variables(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn extract(response: Self::Response) -> Result<Self::Output> {
        Ok(response.viewer.id)
    }
}
