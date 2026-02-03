//! TUI Application state and logic
//!
//! This will be the main application struct that manages:
//! - Worker list view
//! - Detail view for selected worker
//! - Key bindings and navigation

use wt_core::OrchestratorState;

/// TUI application state
#[allow(dead_code)]
pub struct App {
    /// Current state from disk
    state: Option<OrchestratorState>,
    /// Currently selected worker index
    selected: usize,
    /// Whether the app should quit
    should_quit: bool,
}

impl App {
    /// Create a new app instance
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            state: None,
            selected: 0,
            should_quit: false,
        }
    }
}
