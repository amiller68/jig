//! Daemon event log — append-only JSONL lifecycle events.
//!
//! Mirrors the worker events pattern: schema + reducer + generic EventLog.

mod reducer;
mod schema;

pub use reducer::DaemonState;
pub use schema::{Event, EventKind};

use crate::config::paths::daemon_log_path;
use crate::worker::events::EventLog;
use jig_core::error::Result;

pub type DaemonLog = EventLog<Event>;

pub fn global() -> Result<DaemonLog> {
    Ok(EventLog::new(daemon_log_path()?))
}
