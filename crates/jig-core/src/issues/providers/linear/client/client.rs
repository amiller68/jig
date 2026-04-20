use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::error::{LinearError, Result};
use super::request::LinearRequest;

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

pub struct LinearClient {
    api_key: String,
}

#[derive(Serialize)]
struct GqlBody {
    query: &'static str,
    variables: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GqlResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GqlError>>,
}

#[derive(Debug, Deserialize)]
struct GqlError {
    message: String,
}

impl LinearClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
        }
    }

    pub fn execute<R: LinearRequest>(&self, request: R) -> Result<R::Output> {
        let variables = request.variables();
        let wrapper: R::Response = self.raw_execute(R::QUERY, variables)?;
        R::extract(wrapper)
    }

    fn raw_execute<T: DeserializeOwned>(
        &self,
        query: &'static str,
        variables: serde_json::Value,
    ) -> Result<T> {
        let body = GqlBody { query, variables };

        let response = ureq::post(LINEAR_API_URL)
            .config()
            .http_status_as_error(false)
            .build()
            .header("Authorization", &self.api_key)
            .header("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| LinearError::Http(e.to_string()))?;

        let status = response.status();

        let text = response
            .into_body()
            .read_to_string()
            .map_err(|e| LinearError::ReadBody(e.to_string()))?;

        if status.as_u16() >= 400 {
            return Err(LinearError::Status {
                status: status.as_u16(),
                body: text,
            });
        }

        let gql: GqlResponse<T> = serde_json::from_str(&text).map_err(|e| LinearError::Parse {
            msg: e.to_string(),
            body: text.clone(),
        })?;

        if let Some(errors) = gql.errors {
            let msgs: Vec<String> = errors.into_iter().map(|e| e.message).collect();
            return Err(LinearError::GraphQL(msgs.join("; ")));
        }

        gql.data.ok_or(LinearError::NoData)
    }
}
