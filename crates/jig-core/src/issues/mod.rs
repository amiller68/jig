//! Issue provider system.
//!
//! Abstracts issue backends behind a common `IssueProvider` handle.
//! Currently only Linear is supported.

pub mod issue;
pub mod providers;

pub use issue::{Issue, IssueFilter, IssuePriority, IssueStatus, ParentIssue};
pub use providers::linear::LinearProvider;
pub use providers::{IssueProvider, ProviderKind};

use crate::config::JigToml;
use crate::error::{Error, Result};
use crate::global::GlobalConfig;

/// Create an issue provider based on repo and global configuration.
pub fn make_provider(jig_toml: &JigToml, global_config: &GlobalConfig) -> Result<IssueProvider> {
    match jig_toml.issues.provider {
        ProviderKind::Linear => Ok(IssueProvider::new(Box::new(make_linear_provider(
            jig_toml,
            global_config,
        )?))),
    }
}

/// Create a Linear provider (for mutation operations).
pub fn make_linear_provider(
    jig_toml: &JigToml,
    global_config: &GlobalConfig,
) -> Result<LinearProvider> {
    let linear_config = jig_toml.issues.linear.as_ref().ok_or_else(|| {
        Error::Custom("[issues.linear] config required when provider = \"linear\"".into())
    })?;
    LinearProvider::from_config(global_config, linear_config)
}
