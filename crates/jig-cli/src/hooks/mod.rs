//! Hook management for jig.
//!
//! - Git hooks: wrapper scripts in `.git/hooks/` that call `jig hooks <name>`
//! - Agent hooks: jig event scripts installed into an agent's hook system

#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    EventLog(#[from] jig_core::events::EventLogError),
    #[error("{0}")]
    Validation(String),
}

pub mod git;
pub mod handlers;
pub mod install;
pub mod registry;
pub mod uninstall;

pub use git::{generate_hook, is_jig_managed, JIG_MANAGED_MARKER, MANAGED_HOOKS};
pub use handlers::{handle_commit_msg, handle_post_commit, handle_post_merge, handle_pre_commit};
pub use install::{init_hooks, InitResult};
pub use registry::{HookEntry, HookRegistry};
pub use uninstall::uninstall_hooks;
