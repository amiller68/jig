//! Actor trait and daemon background workers.

mod actor;
pub mod monitor;
pub mod prune;
pub mod spawn;
pub mod sync;
pub mod triage;

pub use actor::{Actor, ActorHandle};

use std::sync::Arc;

use crate::context::{Config, RepoEntry};

/// Shared context built once per daemon tick, passed to all actors.
#[derive(Clone)]
pub struct TickContext {
    pub config: Arc<Config>,
    pub repos: Arc<Vec<RepoEntry>>,
    pub session_prefix: String,
}
