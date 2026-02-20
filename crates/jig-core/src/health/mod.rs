//! Health monitoring for workers
//!
//! Tracks worker health metrics and nudge counts for the heartbeat system.

mod state;

pub use state::{HealthState, WorkerHealth, HEALTH_DIR, HEALTH_FILE};
