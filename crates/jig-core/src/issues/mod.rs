//! Issue provider system.
//!
//! Abstracts issue backends behind a common `IssueProvider` handle.
//! Currently only Linear is supported.

pub mod issue;
pub mod providers;

pub use issue::{Issue, IssueFilter, IssuePriority, IssueRef, IssueStatus};
pub use providers::linear::LinearProvider;
pub use providers::{IssueProvider, ProviderKind};
