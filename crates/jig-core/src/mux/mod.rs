pub mod terminal;
pub mod tmux;

pub use terminal::{Terminal, TerminalError};
pub use tmux::{TmuxError, TmuxSession, TmuxWindow};

use std::path::Path;

pub const KNOWN_SHELLS: &[&str] = &["bash", "zsh", "fish", "sh"];

/// Check if a command is available on the host
pub fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

/// Dependency status for health check
#[derive(Debug)]
pub struct DependencyStatus {
    pub name: String,
    pub available: bool,
}

/// Check availability of required dependencies
pub fn check_dependencies() -> Vec<DependencyStatus> {
    vec![
        DependencyStatus {
            name: "git".to_string(),
            available: command_exists("git"),
        },
        DependencyStatus {
            name: "tmux".to_string(),
            available: command_exists("tmux"),
        },
        DependencyStatus {
            name: "claude".to_string(),
            available: command_exists("claude"),
        },
    ]
}

/// A multiplexer session — a named group of windows.
pub trait MuxSession {
    type Error: std::error::Error + Send + Sync + 'static;
    type Window: MuxWindow<Error = Self::Error>;

    fn new(name: impl Into<String>) -> Self;
    fn name(&self) -> &str;
    fn exists(&self) -> bool;
    fn ensure(&self) -> Result<(), Self::Error>;
    fn window(&self, name: impl Into<String>) -> Self::Window;
    fn window_names(&self) -> Result<Vec<String>, Self::Error>;
    fn kill(&self) -> Result<(), Self::Error>;
    fn attach(&self) -> Result<(), Self::Error>;
}

/// A window within a multiplexer session.
pub trait MuxWindow: Sized {
    type Error: std::error::Error + Send + Sync + 'static;

    fn new(session: impl Into<String>, window: impl Into<String>) -> Self;
    fn session_name(&self) -> &str;
    fn window_name(&self) -> &str;
    fn exists(&self) -> bool;
    fn create(&self, dir: &Path) -> Result<(), Self::Error>;
    fn kill(&self) -> Result<(), Self::Error>;
    fn send_keys(&self, keys: &[&str]) -> Result<(), Self::Error>;
    fn send_message(&self, message: &str) -> Result<(), Self::Error>;
    fn is_running(&self) -> bool;
    fn pane_command(&self) -> Option<String>;
    fn attach(&self) -> Result<(), Self::Error>;
}
