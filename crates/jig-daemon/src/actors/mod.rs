//! Actor trait and daemon background workers.

mod actor;
pub mod github;
pub mod issue;
pub mod nudge;
pub mod prune;
pub mod review;
pub mod spawn;
pub mod sync;
pub mod triage;

pub use actor::Actor;
