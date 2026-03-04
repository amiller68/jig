//! Merge command - merge reviewed worktree into current branch

use clap::Args;
use colored::Colorize;

use jig_core::{git, spawn, Error, RepoContext};

use crate::op::{GlobalCtx, NoOutput, Op, RepoCtx};

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

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;
        self.merge_in_repo(repo)
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo_for_worktree(&self.name)?;
        self.merge_in_repo(repo)
    }
}

impl Merge {
    fn merge_in_repo(&self, repo: &RepoContext) -> Result<NoOutput, MergeError> {
        let worktree_path = repo.worktrees_dir.join(&self.name);

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
        spawn::unregister(repo, &self.name)?;

        // Kill tmux window if running
        spawn::kill_window(repo, &self.name)?;

        eprintln!();
        eprintln!(
            "  {} Remove worktree with: {}",
            "→".dimmed(),
            format!("jig remove {}", self.name).cyan()
        );

        Ok(NoOutput)
    }
}
