//! Actor trait and daemon background workers.

mod actor;
pub mod dispatch;
pub mod prune;
pub mod spawn;
pub mod sync;
pub mod triage;

pub use actor::{Actor, ActorHandle};
