//! Review command — show diff, submit reviews, and respond to reviews

use clap::{Args, Subcommand};

use jig_core::git::{Branch, Repo};
use jig_core::Error;

use crate::op::{Op, RepoCtx};
use crate::ui;

pub mod respond;
pub mod submit;

/// Show diff for parent review, submit reviews, or respond to reviews
#[derive(Args, Debug, Clone)]
pub struct Review {
    #[command(subcommand)]
    pub command: Option<ReviewCommand>,

    /// Worktree name (shorthand for `jig review show <name>`)
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// Show full diff instead of summary
    #[arg(long)]
    pub full: bool,
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
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
}

impl Op for Review {
    type Error = ReviewError;
    type Output = ReviewOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        match &self.command {
            Some(ReviewCommand::Show(show)) => run_show(ctx, show),
            Some(ReviewCommand::Submit(sub)) => sub.run_inner(ctx),
            Some(ReviewCommand::Respond(resp)) => resp.run_inner(ctx),
            None => {
                // Backward compat: `jig review <name>` == `jig review show <name>`
                if let Some(name) = &self.name {
                    let show = ReviewShow {
                        name: name.clone(),
                        full: self.full,
                    };
                    run_show(ctx, &show)
                } else {
                    Err(ReviewError::Core(Error::Custom(
                        "Usage: jig review <name> or jig review submit/respond".into(),
                    )))
                }
            }
        }
    }
}

fn run_show(ctx: &RepoCtx, show: &ReviewShow) -> Result<ReviewOutput, ReviewError> {
    let repo = ctx.repo()?;
    let worktree_path = repo.worktrees_path.join(&show.name);

    if !worktree_path.exists() {
        return Err(Error::WorktreeNotFound(show.name.clone()).into());
    }

    let branch = Repo::open(&worktree_path)?.current_branch()?;
    let wt_repo = Repo::open(&worktree_path)?;
    let commits = wt_repo.commits_ahead(&Branch::new(&repo.base_branch))?;
    let is_dirty = wt_repo.has_uncommitted_changes()?;

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
    let diff = wt_repo.diff(&Branch::new(&repo.base_branch))?;
    if show.full {
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
