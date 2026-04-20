use serde::de::DeserializeOwned;

use super::error::Result;

/// A self-contained Linear GraphQL operation.
///
/// Each implementor defines the query string, how to build variables from
/// its fields, the raw GraphQL response shape, and how to extract the
/// meaningful output from that response.
pub trait LinearRequest {
    /// The raw GraphQL `data` shape (e.g. `{ issues: { nodes: [...] } }`).
    type Response: DeserializeOwned;

    /// The meaningful output extracted from `Response`.
    type Output;

    const QUERY: &'static str;

    fn variables(&self) -> serde_json::Value;

    fn extract(response: Self::Response) -> Result<Self::Output>;
}
