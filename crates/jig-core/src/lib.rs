//! jig-core — standalone libraries for git, GitHub, issues, multiplexers,
//! and AI agent adapters.

pub mod agents;
pub mod error;
pub mod events;
pub mod git;
pub mod github;
pub mod issues;
pub mod mux;
pub mod prompt;

pub use agents::Agent;
pub use error::{Error, Result};
pub use events::{EventLog, Reducible};
pub use git::{Branch, DiffStats, FileDiff, GitError, Repo, Worktree, WorktreeRef, WORKTREES_DIR};
pub use github::GitHubClient;
pub use issues::issue::IssueRef;
pub use issues::{Issue, IssueFilter, IssuePriority, IssueProvider, IssueStatus, LinearProvider};
pub use mux::{Mux, MuxError, TmuxMux};
pub use prompt::Prompt;
