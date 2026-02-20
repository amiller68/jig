//! jig-core - Core library for jig
//!
//! This crate provides the core functionality for jig:
//! - Git worktree operations
//! - Configuration management
//! - Worker state management
//! - Tmux session handling
//! - Orchestrator state persistence
//! - Agent adapters for different AI assistants

pub mod adapter;
pub mod config;
pub mod error;
pub mod git;
pub mod health;
pub mod session;
pub mod spawn;
pub mod state;
pub mod terminal;
pub mod worker;
pub mod worktree;

pub use adapter::{get_adapter, AgentAdapter, AgentType, CLAUDE_CODE};
pub use config::{Config, JigToml, RepoConfig};
pub use error::{Error, Result};
pub use state::OrchestratorState;
pub use worker::{DiffStats, FileDiff, TaskContext, Worker, WorkerId, WorkerStatus};
pub use worktree::Worktree;
