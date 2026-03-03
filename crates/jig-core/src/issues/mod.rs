//! Issue provider system.
//!
//! Abstracts issue backends (file-based, Linear, GitHub, etc.) behind a
//! common trait. The default `FileProvider` reads `issues/` markdown files
//! with `**Key:** Value` frontmatter.

pub mod file_provider;
pub mod linear_client;
pub mod linear_provider;
pub mod provider;
pub mod types;

pub use file_provider::FileProvider;
pub use linear_provider::LinearProvider;
pub use provider::IssueProvider;
pub use types::{Issue, IssueFilter, IssuePriority, IssueStatus};

use std::path::Path;

use crate::config::JigToml;
use crate::error::{Error, Result};
use crate::global::GlobalConfig;

/// Create an issue provider based on repo and global configuration.
///
/// When `provider = "linear"`, requires an `[issues.linear]` section in
/// `jig.toml` and a matching profile in the global config. Otherwise
/// falls back to the file-based provider.
pub fn make_provider(
    repo_root: &Path,
    jig_toml: &JigToml,
    global_config: &GlobalConfig,
) -> Result<Box<dyn IssueProvider>> {
    match jig_toml.issues.provider.as_str() {
        "linear" => {
            let linear_config = jig_toml.issues.linear.as_ref().ok_or_else(|| {
                Error::Custom("[issues.linear] config required when provider = \"linear\"".into())
            })?;
            Ok(Box::new(LinearProvider::from_config(
                global_config,
                linear_config,
            )?))
        }
        _ => {
            let issues_dir = repo_root.join(&jig_toml.issues.directory);
            Ok(Box::new(FileProvider::new(&issues_dir)))
        }
    }
}
