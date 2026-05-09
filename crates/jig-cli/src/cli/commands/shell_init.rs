//! Shell initialization command - prints shell integration code

use clap::Args;

use crate::cli::op::Op;
use crate::terminal::shell::{Shell, ShellError};

/// Print shell integration code
#[derive(Args, Debug, Clone)]
pub struct ShellInit {
    /// Shell type (bash, zsh, fish)
    pub shell: String,
}

#[derive(Debug)]
pub struct ShellInitOutput(String);

impl std::fmt::Display for ShellInitOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellInitError {
    #[error(transparent)]
    Shell(#[from] ShellError),
}

impl Op for ShellInit {
    type Error = ShellInitError;
    type Output = ShellInitOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let shell = match self.shell.to_lowercase().as_str() {
            "bash" => Shell::Bash,
            "zsh" => Shell::Zsh,
            "fish" => Shell::Fish,
            other => return Err(ShellError::UnsupportedShell(other.to_string()).into()),
        };
        Ok(ShellInitOutput(shell.init_script().to_string()))
    }
}
