pub mod terminal;
pub mod tmux;

pub use terminal::{Terminal, TerminalError};
pub use tmux::{TmuxError, TmuxSession, TmuxWindow};

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
