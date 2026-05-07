//! Merge command - merge reviewed worktree into current branch

use clap::Args;

use crate::worker::Worker;
use jig_core::git::Repo;
use jig_core::mux::TmuxMux;
use jig_core::Error;

use crate::cli::op::{NoOutput, Op};
use crate::context::RepoConfig;
use crate::cli::ui;

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

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let cfg = RepoConfig::from_cwd()?;
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
        let workers = Worker::discover(&jig_core::git::Repo::open(&cfg.repo_root).unwrap());
        if let Some(worker) = workers.iter().find(|w| w.branch() == self.name.as_str()) {
            worker.unregister()?;
            let repo_name = cfg.repo_root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let mux = TmuxMux::for_repo(&repo_name);
            let _ = worker.kill(&mux);
        }

        eprintln!();
        ui::detail(&format!(
            "Remove worktree with: {}",
            ui::highlight(&format!("jig remove {}", self.name))
        ));

        Ok(NoOutput)
    }
}
