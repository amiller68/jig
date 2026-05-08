pub mod tmux;

pub use tmux::TmuxMux;

use std::path::Path;

pub const KNOWN_SHELLS: &[&str] = &["bash", "zsh", "fish", "sh"];

#[derive(Debug, thiserror::Error)]
pub enum MuxError {
    #[error("mux command failed: {command}: {detail}")]
    CommandFailed { command: String, detail: String },
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("mux command timed out: {command}")]
    Timeout { command: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A multiplexer backend scoped to a named group (e.g. one tmux session).
///
/// Window names are branch names — the backend manages the mapping to its
/// native hierarchy (tmux session:window, cmux workspace, etc.).
pub trait Mux: Send + Sync {
    fn create_window(&self, name: &str, dir: &Path) -> Result<(), MuxError>;
    fn window_exists(&self, name: &str) -> bool;
    fn kill_window(&self, name: &str) -> Result<(), MuxError>;
    fn kill_all(&self) -> Result<(), MuxError>;
    fn send_keys(&self, name: &str, keys: &[&str]) -> Result<(), MuxError>;
    fn send_message(&self, name: &str, message: &str) -> Result<(), MuxError>;
    fn is_running(&self, name: &str) -> bool;
    fn attach_window(&self, name: &str) -> Result<(), MuxError>;
    fn attach(&self) -> Result<(), MuxError>;
}
