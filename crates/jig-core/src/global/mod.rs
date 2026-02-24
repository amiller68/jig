//! Global state infrastructure
//!
//! Cross-repo state aggregation at `~/.config/jig/`.

pub mod config;
pub mod paths;
pub mod state;

pub use config::{GitHubConfig, GlobalConfig, HealthConfig, NotifyConfig};
pub use paths::{
    ensure_global_dirs, global_config_dir, global_hooks_dir, global_state_dir, worker_events_dir,
};
pub use state::{WorkerEntry, WorkersState};
