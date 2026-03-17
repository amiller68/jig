//! Issue provider system.
//!
//! Abstracts issue backends (file-based, Linear, GitHub, etc.) behind a
//! common trait. The default `FileProvider` reads `issues/` markdown files
//! with `**Key:** Value` frontmatter.

pub mod file_provider;
pub mod linear_client;
pub mod linear_provider;
pub mod naming;
pub mod provider;
pub mod types;

pub use file_provider::FileProvider;
pub use linear_provider::LinearProvider;
pub use provider::{IssueProvider, ProviderKind};
pub use types::{Issue, IssueFilter, IssuePriority, IssueStatus};

use std::path::Path;

use crate::config::JigToml;
use crate::error::{Error, Result};
use crate::global::GlobalConfig;

/// Create an issue provider based on repo and global configuration.
///
/// When `provider = "linear"`, requires an `[issues.linear]` section in
/// `jig.toml` and a matching profile in the global config. Otherwise
/// falls back to the file-based provider reading from the working tree.
pub fn make_provider(
    repo_root: &Path,
    jig_toml: &JigToml,
    global_config: &GlobalConfig,
) -> Result<Box<dyn IssueProvider>> {
    make_provider_inner(repo_root, jig_toml, global_config, None)
}

/// Like `make_provider`, but reads file-based issues from the given git ref
/// (e.g. `"origin/main"`) instead of the working tree. This keeps issue
/// discovery in sync with the remote after a `git fetch`.
pub fn make_provider_with_ref(
    repo_root: &Path,
    jig_toml: &JigToml,
    global_config: &GlobalConfig,
    git_ref: &str,
) -> Result<Box<dyn IssueProvider>> {
    make_provider_inner(repo_root, jig_toml, global_config, Some(git_ref))
}

/// Create a file-based provider (for mutation operations).
pub fn make_file_provider(repo_root: &Path, jig_toml: &JigToml) -> FileProvider {
    let issues_dir = repo_root.join(&jig_toml.issues.directory);
    FileProvider::new(&issues_dir)
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

fn make_provider_inner(
    repo_root: &Path,
    jig_toml: &JigToml,
    global_config: &GlobalConfig,
    git_ref: Option<&str>,
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
            let provider = FileProvider::new(&issues_dir);
            let provider = if let Some(git_ref) = git_ref {
                provider.with_git_ref(repo_root, git_ref, &jig_toml.issues.directory)
            } else {
                provider
            };
            Ok(Box::new(provider))
        }
    }
}
