//! Open worktree command

use clap::Args;
use colored::Colorize;
use std::path::PathBuf;

use jig_core::{git, terminal, Error};

use crate::op::{Op, OpContext};

/// Open/cd into a worktree
#[derive(Args, Debug, Clone)]
pub struct Open {
    /// Worktree name (or --all to open all in tabs)
    pub name: Option<String>,

    /// Open all worktrees in new tabs
    #[arg(long)]
    pub all: bool,
}

/// Output containing optional cd command
#[derive(Debug)]
pub enum OpenOutput {
    /// No output (when opening tabs)
    None,
    /// cd command to stdout
    Cd(PathBuf),
}

impl std::fmt::Display for OpenOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenOutput::None => Ok(()),
            OpenOutput::Cd(path) => write!(f, "cd '{}'", path.display()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Op for Open {
    type Error = OpenError;
    type Output = OpenOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let worktrees_dir = git::get_worktrees_dir()?;

        if self.all {
            // Open all worktrees in new tabs
            let worktrees = git::list_worktrees()?;

            if worktrees.is_empty() {
                eprintln!("No worktrees to open");
                return Ok(OpenOutput::None);
            }

            for wt_name in worktrees {
                let path = worktrees_dir.join(&wt_name);
                if path.exists() {
                    let opened = terminal::open_tab(&path)?;
                    if opened {
                        eprintln!("{} Opened '{}' in new tab", "✓".green(), wt_name.cyan());
                    }
                }
            }

            // Don't output cd command - tabs are opened directly
            Ok(OpenOutput::None)
        } else {
            // Open specific worktree
            let name = self.name.as_deref().ok_or(Error::NameRequired)?;
            let worktree_path = worktrees_dir.join(name);

            if !worktree_path.exists() {
                return Err(Error::WorktreeNotFound(name.to_string()).into());
            }

            // Output cd command for shell wrapper to eval
            let canonical = worktree_path.canonicalize()?;
            Ok(OpenOutput::Cd(canonical))
        }
    }
}
