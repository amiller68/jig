//! jig-core - Core library for jig
//!
//! This crate provides the core functionality for jig:
//! - Git worktree operations
//! - Configuration management
//! - Worker state management
//! - Tmux session handling
//! - Orchestrator state persistence
//! - Agent adapters for different AI assistants

pub mod agents;
pub mod config;
pub mod error;
pub mod git;
pub mod github;
pub mod hooks;
pub mod host;
pub mod issues;
pub mod notify;
pub mod prompt;
pub mod review;
pub mod worker;

/// Deprecated: use `host` directly
pub mod terminal {
    pub use crate::host::terminal::TerminalError;
    pub use crate::host::{check_dependencies, command_exists, DependencyStatus, Terminal};

    pub fn open_tab(dir: &std::path::Path) -> crate::error::Result<bool> {
        let terminal = Terminal::detect();
        match terminal.open_tab(dir) {
            Ok(()) => Ok(true),
            Err(TerminalError::NotSupported { .. }) => Ok(false),
            Err(TerminalError::MissingDependency(_)) => Ok(false),
            Err(TerminalError::Io(e)) => Err(crate::error::Error::Io(e)),
        }
    }
}

pub use agents::Agent;
pub use config::{
    ensure_global_dirs, global_config_dir, global_state_dir, Config, GlobalConfig, JigToml,
    LinearIssuesConfig, RepoRegistry, ReviewConfig, WorkerEntry, WorkersState,
};
pub use error::{Error, Result};
pub use git::{Branch, DiffStats, FileDiff, GitError, Repo, Worktree, WorktreeRef};
pub use github::GitHubClient;
pub use host::tmux::{TmuxError, TmuxSession, TmuxWindow};
pub use issues::issue::IssueRef;
pub use issues::{
    make_linear_provider, make_provider, Issue, IssueFilter, IssuePriority, IssueProvider,
    IssueStatus, LinearProvider,
};
pub use prompt::Prompt;
pub use worker::events::{
    derive_status, Event, EventKind, EventLog, EventType, TerminalKind, WorkerState,
};
pub use worker::{Worker, WorkerStatus};
