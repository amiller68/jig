//! Merge command - merge reviewed worktree into current branch

use clap::Args;

use jig_core::git::Repo;
use jig_core::worker::Worker;
use jig_core::Error;

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
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
}

impl Op for Merge {
    type Error = MergeError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let cfg = ctx.config()?;
        let worktree_path = cfg.worktrees_path.join(&self.name);

        if !worktree_path.exists() {
            return Err(jig_core::GitError::WorktreeNotFound(self.name.clone()).into());
        }

        // Check for uncommitted changes
        if Repo::open(&worktree_path)?.has_uncommitted_changes()? {
            return Err(jig_core::GitError::UncommittedChanges.into());
        }

        // Get branch name
        let branch = Repo::open(&worktree_path)?.current_branch()?;

        // Merge the branch
        let git_repo = Repo::discover()?;
        git_repo.merge_branch(&branch)?;

        ui::success(&format!(
            "Merged branch '{}' into current branch",
            ui::highlight(&branch)
        ));

        // Clean up worker state
        let workers = Worker::discover(cfg);
        if let Some(worker) = workers.iter().find(|w| w.name() == self.name) {
            worker.unregister()?;
            let _ = worker.kill();
        }

        eprintln!();
        ui::detail(&format!(
            "Remove worktree with: {}",
            ui::highlight(&format!("jig remove {}", self.name))
        ));

        Ok(NoOutput)
    }
}
