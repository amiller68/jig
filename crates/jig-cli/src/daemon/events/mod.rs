//! Daemon event log — append-only JSONL lifecycle events.

mod reducer;
mod schema;

pub use reducer::DaemonState;
pub use schema::{Event, EventKind};

use crate::context::daemon_log_path;
use jig_core::error::Result;
use jig_core::EventLog;

pub type DaemonLog = EventLog<Event>;

pub fn global() -> Result<DaemonLog> {
    Ok(EventLog::new(daemon_log_path()?))
}
