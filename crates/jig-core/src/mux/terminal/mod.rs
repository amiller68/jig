mod emulator;

use std::path::Path;

pub use emulator::*;

/// Terminal-specific errors
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("{terminal} does not support {operation}")]
    NotSupported {
        terminal: TerminalEmulatorKind,
        operation: String,
    },
    #[error("missing dependency: {0}")]
    MissingDependency(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Wraps a detected terminal emulator, providing the entry point for
/// terminal operations.
pub struct Terminal {
    emulator: Box<dyn TerminalEmulator>,
}

impl Terminal {
    pub fn detect() -> Self {
        let emulator: Box<dyn TerminalEmulator> = if let Ok(term) = std::env::var("TERM_PROGRAM") {
            match term.to_lowercase().as_str() {
                "iterm.app" => Box::new(ITerm2),
                "apple_terminal" => Box::new(TerminalApp),
                "ghostty" => Box::new(Ghostty),
                "wezterm" => Box::new(WezTerm),
                "alacritty" => Box::new(Alacritty),
                _ => Box::new(Unknown(term)),
            }
        } else if std::env::var("KITTY_WINDOW_ID").is_ok() {
            Box::new(Kitty)
        } else if std::env::var("WEZTERM_UNIX_SOCKET").is_ok() {
            Box::new(WezTerm)
        } else {
            Box::new(Unknown("unknown".to_string()))
        };
        Self { emulator }
    }

    pub fn kind(&self) -> TerminalEmulatorKind {
        self.emulator.kind()
    }

    pub fn open_tab(&self, dir: &Path) -> Result<(), TerminalError> {
        self.emulator.open_tab(dir)
    }
}
