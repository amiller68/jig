//! Hook management for jig.

pub mod claude;

pub use claude::{
    install_claude_hooks, install_claude_hooks_to, InstallResult, CLAUDE_HOOK_TEMPLATES,
};
