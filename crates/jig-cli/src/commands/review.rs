//! Review command — show diff, submit reviews, and respond to reviews

use clap::{Args, Subcommand};

use jig_core::git::Repo;
use jig_core::Error;

use crate::op::{Op, RepoCtx};
use crate::ui;

pub mod respond;
pub mod submit;

/// Show diff for parent review, submit reviews, or respond to reviews
#[derive(Args, Debug, Clone)]
pub struct Review {
    #[command(subcommand)]
    pub command: ReviewCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ReviewCommand {
    /// Show diff for parent review
    Show(ReviewShow),
    /// Submit a review (reads review markdown from stdin)
    Submit(submit::ReviewSubmit),
    /// Respond to a review (reads response markdown from stdin)
    Respond(respond::ReviewRespond),
}

/// Show diff for parent review
#[derive(Args, Debug, Clone)]
pub struct ReviewShow {
    /// Worktree name
    pub name: String,

    /// Show full diff instead of summary
    #[arg(long)]
    pub full: bool,
}

/// Output containing diff content (goes to stdout)
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
    ReviewParse(#[from] jig_core::review::ReviewError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Review {0:03} not found at .jig/reviews/{0:03}.md")]
    ReviewNotFound(u32),
}

impl Op for Review {
    type Error = ReviewError;
    type Output = ReviewOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        match &self.command {
            ReviewCommand::Show(show) => run_show(ctx, show),
            ReviewCommand::Submit(sub) => sub.run_inner(ctx),
            ReviewCommand::Respond(resp) => resp.run_inner(ctx),
        }
    }
}

fn run_show(ctx: &RepoCtx, show: &ReviewShow) -> Result<ReviewOutput, ReviewError> {
    let repo = ctx.repo()?;
    let worktree_path = repo.worktrees_dir.join(&show.name);

    if !worktree_path.exists() {
        return Err(Error::WorktreeNotFound(show.name.clone()).into());
    }

    let branch = Repo::worktree_branch(&worktree_path)?;
    let commits = Repo::commits_ahead(&worktree_path, &repo.base_branch)?;
    let is_dirty = Repo::has_uncommitted_changes(&worktree_path)?;

    // Header
    ui::header(&format!("Review: {}", show.name));
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
    if show.full {
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
