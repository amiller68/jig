//! Merge command - merge reviewed worktree into current branch

use clap::Args;
use colored::Colorize;

use jig_core::{git, spawn, Error};

use crate::op::{NoOutput, Op, OpContext};

/// Merge reviewed worktree into current branch
#[derive(Args, Debug, Clone)]
pub struct Merge {
    /// Worktree name
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    #[error(transparent)]
    Core(#[from] Error),
}

impl Op for Merge {
    type Error = MergeError;
    type Output = NoOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let worktrees_dir = git::get_worktrees_dir()?;
        let worktree_path = worktrees_dir.join(&self.name);

        if !worktree_path.exists() {
            return Err(Error::WorktreeNotFound(self.name.clone()).into());
        }

        // Check for uncommitted changes
        if git::has_uncommitted_changes(&worktree_path)? {
            return Err(Error::UncommittedChanges.into());
        }

        // Get branch name
        let branch = git::get_worktree_branch(&worktree_path)?;

        // Merge the branch
        git::merge_branch(&branch)?;

        eprintln!(
            "{} Merged branch '{}' into current branch",
            "✓".green(),
            branch.cyan()
        );

        // Unregister from spawn state
        spawn::unregister(&self.name)?;

        // Kill tmux window if running
        spawn::kill_window(&self.name)?;

        eprintln!();
        eprintln!(
            "  {} Remove worktree with: {}",
            "→".dimmed(),
            format!("jig remove {}", self.name).cyan()
        );

        Ok(NoOutput)
    }
}
