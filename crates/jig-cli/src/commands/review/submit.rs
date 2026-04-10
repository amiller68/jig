//! `jig review submit` — read review markdown from stdin, validate, write to .jig/reviews/

use std::io::Read as _;

use clap::Args;

use jig_core::review::{self, Review};

use crate::op::RepoCtx;
use crate::ui;

use super::{ReviewError, ReviewOutput};

/// Submit a review (reads review markdown from stdin)
#[derive(Args, Debug, Clone)]
pub struct ReviewSubmit {}

impl ReviewSubmit {
    pub fn run_inner(&self, ctx: &RepoCtx) -> Result<ReviewOutput, ReviewError> {
        let repo = ctx.repo()?;

        // Read all of stdin
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;

        // Parse and validate
        let review_data = Review::from_markdown(&input)?;

        // Resolve worktree from cwd — use the worktrees_dir as the reviews root
        // Reviews are stored in the current working directory's .jig/reviews/
        let cwd = std::env::current_dir()?;

        // Create reviews dir if needed
        let reviews_dir = review::reviews_dir(&cwd);
        std::fs::create_dir_all(&reviews_dir)?;

        // Compute next filename
        let review_path = review::next_review_path(&cwd);
        let review_number = review::review_count(&cwd) + 1;

        // Re-serialize to normalize formatting
        let markdown = review_data.to_markdown(review_number);
        std::fs::write(&review_path, &markdown)?;

        // Make path relative for display
        let display_path = review_path
            .strip_prefix(&repo.repo_root)
            .unwrap_or(&review_path);

        ui::success(&format!("Review written to {}", display_path.display()));

        Ok(ReviewOutput(format!(
            "Review written to {}\nVerdict: {}",
            display_path.display(),
            review_data.verdict
        )))
    }
}
