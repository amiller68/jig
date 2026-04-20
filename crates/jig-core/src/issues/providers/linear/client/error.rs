#[derive(Debug, thiserror::Error)]
pub enum LinearError {
    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("HTTP {status}: {body}")]
    Status { status: u16, body: String },

    #[error("failed to read response: {0}")]
    ReadBody(String),

    #[error("failed to parse response: {msg} — body: {body}")]
    Parse { msg: String, body: String },

    #[error("GraphQL error: {0}")]
    GraphQL(String),

    #[error("no data in response")]
    NoData,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, LinearError>;
