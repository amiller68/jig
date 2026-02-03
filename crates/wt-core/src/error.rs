//! Error types for wt-core

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Not in a git repository")]
    NotInGitRepo,

    #[error("Not in a worktree")]
    NotInWorktree,

    #[error("Worktree '{0}' already exists")]
    WorktreeExists(String),

    #[error("Worktree '{0}' does not exist")]
    WorktreeNotFound(String),

    #[error("Worker '{0}' not found")]
    WorkerNotFound(String),

    #[error("Branch '{0}' does not exist")]
    BranchNotFound(String),

    #[error("Worktree has uncommitted changes. Use --force to override")]
    UncommittedChanges,

    #[error("No worktrees found")]
    NoWorktrees,

    #[error("Name is required")]
    NameRequired,

    #[error("Config key '{0}' not found")]
    ConfigNotFound(String),

    #[error("Already initialized. Use --force to reinitialize")]
    AlreadyInitialized,

    #[error("Missing dependency: {0}")]
    MissingDependency(String),

    #[error("Tmux session not found: {0}")]
    TmuxSessionNotFound(String),

    #[error("Git error: {0}")]
    Git(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("Invalid path: {0}")]
    InvalidPath(PathBuf),

    #[error("State error: {0}")]
    State(String),

    #[error("{0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, Error>;
