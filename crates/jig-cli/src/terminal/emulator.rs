use std::fmt;
use std::path::Path;
use std::process::Command;

use super::{Terminal, TerminalError};

/// Terminal identity
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEmulatorKind {
    ITerm2,
    TerminalApp,
    Ghostty,
    Kitty,
    WezTerm,
    Alacritty,
    Unknown(String),
}

impl fmt::Display for TerminalEmulatorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ITerm2 => write!(f, "iTerm2"),
            Self::TerminalApp => write!(f, "Terminal.app"),
            Self::Ghostty => write!(f, "Ghostty"),
            Self::Kitty => write!(f, "Kitty"),
            Self::WezTerm => write!(f, "WezTerm"),
            Self::Alacritty => write!(f, "Alacritty"),
            Self::Unknown(name) => write!(f, "{}", name),
        }
    }
}

pub trait TerminalEmulator {
    fn kind(&self) -> TerminalEmulatorKind;

    fn open_tab(&self, dir: &Path) -> Result<(), TerminalError>;
}

pub struct ITerm2;

impl TerminalEmulator for ITerm2 {
    fn kind(&self) -> TerminalEmulatorKind {
        TerminalEmulatorKind::ITerm2
    }

    fn open_tab(&self, dir: &Path) -> Result<(), TerminalError> {
        let dir_str = dir.to_string_lossy();
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
        Ok(())
    }
}

pub struct TerminalApp;

impl TerminalEmulator for TerminalApp {
    fn kind(&self) -> TerminalEmulatorKind {
        TerminalEmulatorKind::TerminalApp
    }

    fn open_tab(&self, dir: &Path) -> Result<(), TerminalError> {
        let dir_str = dir.to_string_lossy();
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
        Ok(())
    }
}

pub struct Ghostty;

impl TerminalEmulator for Ghostty {
    fn kind(&self) -> TerminalEmulatorKind {
        TerminalEmulatorKind::Ghostty
    }

    fn open_tab(&self, _dir: &Path) -> Result<(), TerminalError> {
        Err(TerminalError::NotSupported {
            terminal: self.kind(),
            operation: "open_tab".to_string(),
        })
    }
}

pub struct Kitty;

impl TerminalEmulator for Kitty {
    fn kind(&self) -> TerminalEmulatorKind {
        TerminalEmulatorKind::Kitty
    }

    fn open_tab(&self, dir: &Path) -> Result<(), TerminalError> {
        if Terminal::which("kitten").is_none() {
            return Err(TerminalError::MissingDependency("kitten".to_string()));
        }
        let dir_str = dir.to_string_lossy();
        Command::new("kitten")
            .args(["@", "launch", "--type=tab", "--cwd", &*dir_str])
            .output()?;
        Ok(())
    }
}

pub struct WezTerm;

impl TerminalEmulator for WezTerm {
    fn kind(&self) -> TerminalEmulatorKind {
        TerminalEmulatorKind::WezTerm
    }

    fn open_tab(&self, dir: &Path) -> Result<(), TerminalError> {
        if Terminal::which("wezterm").is_none() {
            return Err(TerminalError::MissingDependency("wezterm".to_string()));
        }
        let dir_str = dir.to_string_lossy();
        Command::new("wezterm")
            .args(["cli", "spawn", "--cwd", &*dir_str])
            .output()?;
        Ok(())
    }
}

pub struct Alacritty;

impl TerminalEmulator for Alacritty {
    fn kind(&self) -> TerminalEmulatorKind {
        TerminalEmulatorKind::Alacritty
    }

    fn open_tab(&self, _dir: &Path) -> Result<(), TerminalError> {
        Err(TerminalError::NotSupported {
            terminal: self.kind(),
            operation: "open_tab".to_string(),
        })
    }
}

pub struct Unknown(pub String);

impl TerminalEmulator for Unknown {
    fn kind(&self) -> TerminalEmulatorKind {
        TerminalEmulatorKind::Unknown(self.0.clone())
    }

    fn open_tab(&self, _dir: &Path) -> Result<(), TerminalError> {
        Err(TerminalError::NotSupported {
            terminal: self.kind(),
            operation: "open_tab".to_string(),
        })
    }
}
