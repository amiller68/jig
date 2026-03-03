//! Hook management for jig.

pub mod claude;
pub mod git;
pub mod handlers;
pub mod install;
pub mod registry;
pub mod uninstall;

pub use claude::{
    install_claude_hooks, install_claude_hooks_to, InstallResult, CLAUDE_HOOK_TEMPLATES,
};
pub use git::{generate_hook, is_jig_managed, JIG_MANAGED_MARKER, MANAGED_HOOKS};
pub use handlers::{handle_post_commit, handle_post_merge, handle_pre_commit};
pub use install::{init_hooks, InitResult};
pub use registry::{HookEntry, HookRegistry};
pub use uninstall::uninstall_hooks;
