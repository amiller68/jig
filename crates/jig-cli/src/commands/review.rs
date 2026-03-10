//! Review command - show diff for parent review

use clap::Args;

use jig_core::git::Repo;
use jig_core::Error;

use crate::op::{Op, RepoCtx};
use crate::ui;

/// Show diff for parent review
#[derive(Args, Debug, Clone)]
pub struct Review {
    /// Worktree name
    pub name: String,

    /// Show full diff instead of summary
    #[arg(long)]
    pub full: bool,
}

/// Output containing diff content (goes to stdout)
#[derive(Debug)]
pub struct ReviewOutput(String);

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
}

impl Op for Review {
    type Error = ReviewError;
    type Output = ReviewOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;
        let worktree_path = repo.worktrees_dir.join(&self.name);

        if !worktree_path.exists() {
            return Err(Error::WorktreeNotFound(self.name.clone()).into());
        }

        let branch = Repo::worktree_branch(&worktree_path)?;
        let commits = Repo::commits_ahead(&worktree_path, &repo.base_branch)?;
        let is_dirty = Repo::has_uncommitted_changes(&worktree_path)?;

        // Header
        ui::header(&format!("Review: {}", self.name));
        eprintln!();
        eprintln!("  {} {}", ui::dim("Branch:"), ui::highlight(&branch));
        eprintln!(
            "  {} {} ahead of {}",
            ui::dim("Commits:"),
            ui::highlight(&commits.len().to_string()),
            ui::dim(&repo.base_branch)
        );

        if is_dirty {
            eprintln!();
            ui::warning("Worktree has uncommitted changes");
        }

        // Commit history
        if !commits.is_empty() {
            eprintln!();
            ui::header("Commits:");
            for commit in &commits {
                eprintln!("  {}", commit);
            }
        }

        // Diff
        eprintln!();
        if self.full {
            ui::header("Full diff:");
            let diff = Repo::diff(&worktree_path, &repo.base_branch)?;
            if diff.is_empty() {
                eprintln!("  No changes");
                Ok(ReviewOutput(String::new()))
            } else {
                Ok(ReviewOutput(diff))
            }
        } else {
            ui::header("Changed files:");
            let stat = Repo::diff_stat(&worktree_path, &repo.base_branch)?;
            if stat.is_empty() {
                eprintln!("  No changes");
                Ok(ReviewOutput(String::new()))
            } else {
                Ok(ReviewOutput(stat))
            }
        }
    }
}
