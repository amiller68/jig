//! Terminal detection and operations
//!
//! Handles terminal emulator detection and tab opening.

use std::path::Path;
use std::process::Command;

use crate::error::Result;

/// Detected terminal emulator
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminal {
    ITerm2,
    TerminalApp,
    Ghostty,
    Kitty,
    WezTerm,
    Alacritty,
    Unknown(String),
}

impl Terminal {
    pub fn name(&self) -> &str {
        match self {
            Terminal::ITerm2 => "iTerm2",
            Terminal::TerminalApp => "Terminal.app",
            Terminal::Ghostty => "Ghostty",
            Terminal::Kitty => "Kitty",
            Terminal::WezTerm => "WezTerm",
            Terminal::Alacritty => "Alacritty",
            Terminal::Unknown(name) => name,
        }
    }

    pub fn supports_tabs(&self) -> bool {
        matches!(
            self,
            Terminal::ITerm2
                | Terminal::TerminalApp
                | Terminal::Ghostty
                | Terminal::Kitty
                | Terminal::WezTerm
        )
    }
}

/// Detect the current terminal emulator
pub fn detect_terminal() -> Terminal {
    // Check TERM_PROGRAM first
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        match term.to_lowercase().as_str() {
            "iterm.app" => return Terminal::ITerm2,
            "apple_terminal" => return Terminal::TerminalApp,
            "ghostty" => return Terminal::Ghostty,
            "wezterm" => return Terminal::WezTerm,
            "alacritty" => return Terminal::Alacritty,
            _ => {}
        }
    }

    // Check Kitty
    if std::env::var("KITTY_WINDOW_ID").is_ok() {
        return Terminal::Kitty;
    }

    // Check WezTerm
    if std::env::var("WEZTERM_UNIX_SOCKET").is_ok() {
        return Terminal::WezTerm;
    }

    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_else(|_| "unknown".to_string());
    Terminal::Unknown(term_program)
}

/// Open a new terminal tab with the given directory
pub fn open_tab(dir: &Path) -> Result<bool> {
    let terminal = detect_terminal();
    let dir_str = dir.to_string_lossy();

    match terminal {
        Terminal::ITerm2 => {
            let script = format!(
                r#"tell application "iTerm2"
                    tell current window
                        create tab with default profile
                        tell current session
                            write text "cd '{}'"
                        end tell
                    end tell
                end tell"#,
                dir_str
            );
            Command::new("osascript").args(["-e", &script]).output()?;
            Ok(true)
        }
        Terminal::TerminalApp => {
            let script = format!(
                r#"tell application "Terminal"
                    activate
                    tell application "System Events" to keystroke "t" using command down
                    delay 0.3
                    do script "cd '{}'" in front window
                end tell"#,
                dir_str
            );
            Command::new("osascript").args(["-e", &script]).output()?;
            Ok(true)
        }
        Terminal::Ghostty => {
            Command::new("open")
                .args(["-a", "Ghostty", &*dir_str])
                .output()?;
            Ok(true)
        }
        Terminal::Kitty => {
            if command_exists("kitten") {
                Command::new("kitten")
                    .args(["@", "launch", "--type=tab", "--cwd", &*dir_str])
                    .output()?;
                Ok(true)
            } else {
                tracing::warn!("kitten not found. Install it for tab support in Kitty.");
                Ok(false)
            }
        }
        Terminal::WezTerm => {
            if command_exists("wezterm") {
                Command::new("wezterm")
                    .args(["cli", "spawn", "--cwd", &*dir_str])
                    .output()?;
                Ok(true)
            } else {
                tracing::warn!("wezterm CLI not found.");
                Ok(false)
            }
        }
        Terminal::Alacritty => {
            Command::new("alacritty")
                .args(["--working-directory", &*dir_str])
                .spawn()?;
            Ok(true)
        }
        Terminal::Unknown(_) => {
            tracing::warn!(
                "Terminal '{}' not supported for opening tabs.",
                terminal.name()
            );
            Ok(false)
        }
    }
}

/// Check if a command is available
pub fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

/// Dependency status for health check
#[derive(Debug)]
pub struct DependencyStatus {
    pub name: String,
    pub available: bool,
    pub required: bool,
}

/// Get all dependencies status
pub fn check_dependencies() -> Vec<DependencyStatus> {
    vec![
        DependencyStatus {
            name: "git".to_string(),
            available: command_exists("git"),
            required: true,
        },
        DependencyStatus {
            name: "tmux".to_string(),
            available: command_exists("tmux"),
            required: false,
        },
        DependencyStatus {
            name: "jq".to_string(),
            available: command_exists("jq"),
            required: false,
        },
        DependencyStatus {
            name: "claude".to_string(),
            available: command_exists("claude"),
            required: false,
        },
        DependencyStatus {
            name: "gh".to_string(),
            available: command_exists("gh"),
            required: false,
        },
    ]
}
