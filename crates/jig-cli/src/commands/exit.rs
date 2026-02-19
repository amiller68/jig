//! Exit worktree command

use clap::Args;
use colored::Colorize;
use std::path::PathBuf;

use jig_core::{git, Error};

use crate::op::{Op, OpContext};

/// Exit current worktree and remove it
#[derive(Args, Debug, Clone)]
pub struct Exit {
    /// Force removal even with uncommitted changes
    #[arg(long, short)]
    pub force: bool,
}

/// Output containing cd command to base repo
#[derive(Debug)]
pub struct ExitOutput(PathBuf);

impl std::fmt::Display for ExitOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cd '{}'", self.0.display())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExitError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Op for Exit {
    type Error = ExitError;
    type Output = ExitOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        // Check if we're in a worktree
        let name = git::get_current_worktree_name_auto()?.ok_or(Error::NotInWorktree)?;

        let worktrees_dir = git::get_worktrees_dir()?;
        let worktree_path = worktrees_dir.join(&name);
        let base_repo = git::get_base_repo()?;

        // Check for uncommitted changes unless force
        if !self.force && git::has_uncommitted_changes(&worktree_path)? {
            return Err(Error::UncommittedChanges.into());
        }

        // Remove the worktree
        git::remove_worktree(&worktree_path, self.force)?;

        // Clean up empty parent directories (for nested paths)
        let mut parent = worktree_path.parent();
        while let Some(p) = parent {
            if p == worktrees_dir {
                break;
            }
            if p.read_dir()?.next().is_none() {
                std::fs::remove_dir(p)?;
            } else {
                break;
            }
            parent = p.parent();
        }

        eprintln!("{} Exited worktree '{}'", "✓".green(), name.cyan());

        // Output cd command to base repo
        let canonical = base_repo.canonicalize()?;
        Ok(ExitOutput(canonical))
    }
}
