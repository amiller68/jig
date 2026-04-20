use std::io;

pub type Result<T> = std::result::Result<T, GitHubError>;

#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    #[error("gh CLI failed: {0}")]
    Cli(String),
    #[error("failed to parse GitHub response: {msg}")]
    Parse { msg: String, body: String },
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
