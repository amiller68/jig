//! Worker health detection
//!
//! Provides tmux output scraping and pattern matching to detect
//! worker states: working, idle at shell prompt, or stuck at
//! an interactive prompt.

pub mod detector;
pub mod tmux;

pub use detector::{WorkerDetector, WorkerState};
pub use tmux::{capture_pane, check_worker};
