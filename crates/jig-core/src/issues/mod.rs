//! Issue provider system.
//!
//! Abstracts issue backends (file-based, Linear, GitHub, etc.) behind a
//! common trait. The default `FileProvider` reads `issues/` markdown files
//! with `**Key:** Value` frontmatter.

pub mod file_provider;
pub mod provider;
pub mod types;

pub use file_provider::FileProvider;
pub use provider::IssueProvider;
pub use types::{Issue, IssueFilter, IssuePriority, IssueStatus};
