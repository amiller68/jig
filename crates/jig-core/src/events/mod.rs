//! Event log system for worker lifecycle tracking.
//!
//! Append-only JSONL files per worker, stored in the global state dir.

mod derive;
mod log;
mod reducer;
mod schema;

pub use derive::derive_status;
pub use log::EventLog;
pub use reducer::WorkerState;
pub use schema::{Event, EventType};
