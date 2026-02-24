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
pub mod context;
pub mod dispatch;
pub mod error;
pub mod events;
pub mod git;
pub mod global;
pub mod hooks;
pub mod notify;
pub mod registry;
pub mod session;
pub mod spawn;
pub mod state;
pub mod terminal;
pub mod worker;
pub mod worktree;

pub use adapter::{get_adapter, AgentAdapter, AgentType, CLAUDE_CODE};
pub use config::{Config, JigToml, RepoConfig};
pub use context::RepoContext;
pub use error::{Error, Result};
pub use events::{derive_status, Event, EventLog, EventType, WorkerState};
pub use global::{
    ensure_global_dirs, global_config_dir, global_state_dir, GlobalConfig, WorkerEntry,
    WorkersState,
};
pub use registry::RepoRegistry;
pub use state::OrchestratorState;
pub use worker::{DiffStats, FileDiff, TaskContext, Worker, WorkerId, WorkerStatus};
pub use worktree::Worktree;
