//! Review command - show diff for parent review

use clap::Args;
use colored::Colorize;

use jig_core::{config, git, Error};

use crate::op::{Op, OpContext};

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

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let worktrees_dir = git::get_worktrees_dir()?;
        let worktree_path = worktrees_dir.join(&self.name);

        if !worktree_path.exists() {
            return Err(Error::WorktreeNotFound(self.name.clone()).into());
        }

        let base_branch = config::get_base_branch()?;
        let branch = git::get_worktree_branch(&worktree_path)?;
        let commits = git::get_commits_ahead(&worktree_path, &base_branch)?;
        let is_dirty = git::has_uncommitted_changes(&worktree_path)?;

        // Header
        eprintln!("{}", format!("Review: {}", self.name).bold());
        eprintln!();
        eprintln!("  {} {}", "Branch:".dimmed(), branch.cyan());
        eprintln!(
            "  {} {} ahead of {}",
            "Commits:".dimmed(),
            commits.len().to_string().cyan(),
            base_branch.dimmed()
        );

        if is_dirty {
            eprintln!();
            eprintln!(
                "  {} {}",
                "Warning:".yellow().bold(),
                "Worktree has uncommitted changes".yellow()
            );
        }

        // Commit history
        if !commits.is_empty() {
            eprintln!();
            eprintln!("{}", "Commits:".bold());
            for commit in &commits {
                eprintln!("  {}", commit);
            }
        }

        // Diff
        eprintln!();
        if self.full {
            eprintln!("{}", "Full diff:".bold());
            let diff = git::get_diff(&worktree_path, &base_branch)?;
            if diff.is_empty() {
                eprintln!("  No changes");
                Ok(ReviewOutput(String::new()))
            } else {
                Ok(ReviewOutput(diff))
            }
        } else {
            eprintln!("{}", "Changed files:".bold());
            let stat = git::get_diff_stat(&worktree_path, &base_branch)?;
            if stat.is_empty() {
                eprintln!("  No changes");
                Ok(ReviewOutput(String::new()))
            } else {
                Ok(ReviewOutput(stat))
            }
        }
    }
}
