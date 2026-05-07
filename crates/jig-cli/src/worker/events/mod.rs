//! Event log system for worker lifecycle tracking.
//!
//! Append-only JSONL files per worker, stored in the global state dir.

mod reducer;
mod schema;

pub use reducer::{PrHealth, WorkerState};
pub use schema::{Event, EventKind, EventType, TerminalKind};

/// Worker event log — wraps the core `EventLog` with a `for_worker` constructor.
pub type EventLog = jig_core::EventLog<Event>;

/// Create an event log for a worker using the global config directory.
///
/// Path: `~/.config/jig/<repo>/<branch>/events.jsonl`
pub fn event_log_for_worker(repo: &str, branch: &str) -> jig_core::error::Result<EventLog> {
    let dir = crate::context::worker_events_dir(repo, branch)?;
    Ok(EventLog::new(dir.join("events.jsonl")))
}
