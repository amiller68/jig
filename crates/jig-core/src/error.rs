//! Error types for jig-core

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("not in a git repository")]
    NotInGitRepo,

    #[error("worktree '{0}' not found")]
    WorktreeNotFound(String),

    #[error("uncommitted changes")]
    UncommittedChanges,

    #[error("name is required")]
    NameRequired,

    #[error("missing dependency: {0}")]
    MissingDependency(String),

    #[error(transparent)]
    Mux(#[from] crate::mux::MuxError),

    #[error(transparent)]
    Git(#[from] crate::git::GitError),

    #[error(transparent)]
    Linear(#[from] crate::issues::providers::linear::client::LinearError),

    #[error(transparent)]
    GitHub(#[from] crate::github::GitHubError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("template error: {0}")]
    Template(#[from] handlebars::RenderError),

    #[error("{0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, Error>;
