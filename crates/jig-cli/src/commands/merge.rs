//! Merge command - merge reviewed worktree into current branch

use clap::Args;

use jig_core::git::Repo;
use jig_core::{spawn, Error};

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

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
        let worktree_path = repo.worktrees_dir.join(&self.name);

        if !worktree_path.exists() {
            return Err(Error::WorktreeNotFound(self.name.clone()).into());
        }

        // Check for uncommitted changes
        if Repo::has_uncommitted_changes(&worktree_path)? {
            return Err(Error::UncommittedChanges.into());
        }

        // Get branch name
        let branch = Repo::worktree_branch(&worktree_path)?;

        // Merge the branch
        let git_repo = Repo::discover()?;
        git_repo.merge_branch(&branch)?;

        ui::success(&format!(
            "Merged branch '{}' into current branch",
            ui::highlight(&branch)
        ));

        // Unregister from spawn state
        spawn::unregister(repo, &self.name)?;

        // Kill tmux window if running
        spawn::kill_window(repo, &self.name)?;

        eprintln!();
        ui::detail(&format!(
            "Remove worktree with: {}",
            ui::highlight(&format!("jig remove {}", self.name))
        ));

        Ok(NoOutput)
    }
}
