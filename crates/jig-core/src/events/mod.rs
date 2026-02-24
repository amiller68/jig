//! Event log system for worker lifecycle tracking.
//!
//! Append-only JSONL files per worker, stored in the global state dir.

mod log;
mod schema;

pub use log::EventLog;
pub use schema::{Event, EventType};
