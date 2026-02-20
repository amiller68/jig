//! Git hook management
//!
//! Provides hook registry for tracking installed hooks,
//! enabling idempotent init and safe uninstall.

pub mod registry;

pub use registry::{HookEntry, HookRegistry};
