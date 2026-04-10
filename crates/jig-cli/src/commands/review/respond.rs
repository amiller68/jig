//! `jig review respond` — read response markdown from stdin, validate, write to .jig/reviews/

use std::io::Read as _;

use clap::Args;

use jig_core::review::{self, ReviewResponse};

use crate::op::RepoCtx;
use crate::ui;

use super::{ReviewError, ReviewOutput};

/// Respond to a review (reads response markdown from stdin)
#[derive(Args, Debug, Clone)]
pub struct ReviewRespond {
    /// Review number to respond to
    #[arg(long = "review")]
    pub review_number: u32,
}

impl ReviewRespond {
    pub fn run_inner(&self, ctx: &RepoCtx) -> Result<ReviewOutput, ReviewError> {
        let repo = ctx.repo()?;
        let cwd = std::env::current_dir()?;

        // Validate that the review file exists
        let reviews_dir = review::reviews_dir(&cwd);
        let review_file = reviews_dir.join(format!("{:03}.md", self.review_number));
        if !review_file.exists() {
            return Err(ReviewError::ReviewNotFound(self.review_number));
        }

        // Read all of stdin
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;

        // Parse and validate
        let response = ReviewResponse::from_markdown(&input)?;

        // Write response file
        let response_path = review::review_response_path(&cwd, self.review_number);
        let markdown = response.to_markdown();
        std::fs::write(&response_path, &markdown)?;

        // Make path relative for display
        let display_path = response_path
            .strip_prefix(&repo.repo_root)
            .unwrap_or(&response_path);

        ui::success(&format!("Response written to {}", display_path.display()));

        Ok(ReviewOutput(format!(
            "Response written to {}",
            display_path.display()
        )))
    }
}
