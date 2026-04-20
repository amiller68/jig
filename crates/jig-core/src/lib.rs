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
pub mod commits;
pub mod config;
pub mod context;
pub mod daemon;
pub mod dispatch;
pub mod error;
pub mod events;
pub mod git;
pub mod github;
pub mod global;
pub mod hooks;
pub mod host;
pub mod issues;
pub mod notify;
pub mod nudge;
pub mod registry;
pub mod review;
pub mod spawn;
pub mod state;
pub mod templates;
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
pub use config::{Config, JigToml, LinearIssuesConfig, RepoConfig, ReviewConfig};
pub use context::RepoContext;
pub use error::{Error, Result};
pub use events::{derive_status, Event, EventLog, EventType, WorkerState};
pub use git::{Branch, DiffStats, FileDiff, GitError, Repo, Worktree, WorktreeRef};
pub use github::GitHubClient;
pub use global::{
    ensure_global_dirs, global_config_dir, global_state_dir, GlobalConfig, WorkerEntry,
    WorkersState,
};
pub use host::tmux::{TmuxError, TmuxSession, TmuxWindow};
pub use issues::{
    make_linear_provider, make_provider, Issue, IssueFilter, IssuePriority, IssueProvider,
    IssueStatus, LinearProvider,
};
pub use nudge::{classify_nudge, execute_nudge, NudgeType};
pub use registry::RepoRegistry;
pub use state::OrchestratorState;
pub use templates::{TemplateContext, TemplateEngine};
pub use worker::{IssueRef, ParentInfo, Worker, WorkerStatus};
