//! Hook management for jig.
//!
//! - Git hooks: wrapper scripts in `.git/hooks/` that call `jig hooks <name>`
//! - Agent hooks: jig event scripts installed into an agent's hook system

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

/// Agent hook scripts — generic jig event plumbing that any agent can install.
/// Each entry is (event_name, script_content).
pub const AGENT_HOOK_SCRIPTS: &[(&str, &str)] = &[
    ("PostToolUse", include_str!("agent_scripts/PostToolUse.sh")),
    (
        "Notification",
        include_str!("agent_scripts/Notification.sh"),
    ),
    ("Stop", include_str!("agent_scripts/Stop.sh")),
];
