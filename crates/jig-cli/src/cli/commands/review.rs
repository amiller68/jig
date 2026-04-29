//! Review command — show diff for parent review

use clap::Args;

use jig_core::git::{Branch, Repo};
use jig_core::Error;

use crate::cli::op::{Op, RepoCtx};
use crate::cli::ui;

/// Show diff for parent review
#[derive(Args, Debug, Clone)]
pub struct Review {
    /// Worktree name
    pub name: String,

    /// Show full diff instead of summary
    #[arg(long)]
    pub full: bool,
}

#[derive(Debug)]
pub struct ReviewOutput(pub String);

impl std::fmt::Display for ReviewOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.0.is_empty() {
            write!(f, "{}", self.0)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
}

impl Op for Review {
    type Error = ReviewError;
    type Output = ReviewOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let cfg = ctx.config()?;
        let worktree_path = cfg.worktrees_path.join(&self.name);

        if !worktree_path.exists() {
            return Err(Error::WorktreeNotFound(self.name.clone()).into());
        }

        let base_branch = cfg.base_branch();
        let branch = Repo::open(&worktree_path)?.current_branch()?;
        let wt_repo = Repo::open(&worktree_path)?;
        let commits = wt_repo.commits_ahead(&Branch::new(&base_branch))?;
        let is_dirty = wt_repo.has_uncommitted_changes()?;

        ui::header(&format!("Review: {}", self.name));
        eprintln!();
        eprintln!("  {} {}", ui::dim("Branch:"), ui::highlight(&branch));
        eprintln!(
            "  {} {} ahead of {}",
            ui::dim("Commits:"),
            ui::highlight(&commits.len().to_string()),
            ui::dim(&base_branch)
        );

        if is_dirty {
            eprintln!();
            ui::warning("Worktree has uncommitted changes");
        }

        if !commits.is_empty() {
            eprintln!();
            ui::header("Commits:");
            for commit in &commits {
                eprintln!("  {}", commit);
            }
        }

        eprintln!();
        let diff = wt_repo.diff(&Branch::new(&base_branch))?;
        if self.full {
            ui::header("Full diff:");
            let patch = diff.patch()?;
            if patch.is_empty() {
                eprintln!("  No changes");
                Ok(ReviewOutput(String::new()))
            } else {
                Ok(ReviewOutput(patch))
            }
        } else {
            ui::header("Changed files:");
            let stat = diff.stat_string()?;
            if stat.is_empty() {
                eprintln!("  No changes");
                Ok(ReviewOutput(String::new()))
            } else {
                Ok(ReviewOutput(stat))
            }
        }
    }
}
