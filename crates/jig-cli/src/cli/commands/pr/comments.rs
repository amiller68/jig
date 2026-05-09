//! Comments subcommand — fetch and display PR review feedback

use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use jig_core::git::Repo;
use jig_core::github::{GitHubClient, ReviewComment, ReviewState};

use super::{PrError, PrOutput};

/// Show review comments and feedback on the PR for the current branch
#[derive(Args, Debug, Clone)]
pub struct Comments {
    /// PR number (defaults to the PR for the current branch)
    #[arg(long)]
    pub pr: Option<u64>,

    /// Only show comments between two commits (e.g. HEAD~3..HEAD)
    #[arg(long)]
    pub between: Option<String>,
}

impl Op for Comments {
    type Error = PrError;
    type Output = PrOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let gh = GitHubClient::from_remote()?;

        let pr_number = match self.pr {
            Some(n) => n,
            None => {
                let git_repo = Repo::discover()?;
                let branch = git_repo.current_branch().map_err(|_| PrError::NoBranch)?;
                let pr_info = gh
                    .get_pr_for_branch(&branch)?
                    .ok_or(PrError::NoPr)?;
                pr_info.number
            }
        };

        let reviews = gh.get_reviews(pr_number)?;
        let inline = gh.get_review_comments(pr_number)?;

        let mut output = String::new();

        let actionable_reviews: Vec<&ReviewComment> = reviews
            .iter()
            .filter(|r| {
                !r.body.is_empty()
                    && matches!(
                        r.state,
                        ReviewState::ChangesRequested | ReviewState::Commented
                    )
            })
            .collect();

        if actionable_reviews.is_empty() && inline.is_empty() {
            return Ok(PrOutput("No review feedback on this PR.".into()));
        }

        if !actionable_reviews.is_empty() {
            output.push_str(&format!(
                "{}\n",
                ui::bold(&format!("Reviews ({})", actionable_reviews.len()))
            ));
            for review in &actionable_reviews {
                let state_label = match review.state {
                    ReviewState::ChangesRequested => "changes requested",
                    ReviewState::Commented => "comment",
                    _ => "review",
                };
                output.push_str(&format!(
                    "\n  {} ({}) — {}\n",
                    ui::highlight(&review.author),
                    state_label,
                    review.body.trim()
                ));
            }
        }

        let filtered_inline: Vec<&ReviewComment> = if let Some(ref range) = self.between {
            let changed_files = changed_files_in_range(range);
            inline
                .iter()
                .filter(|c| {
                    c.path
                        .as_deref()
                        .map(|p| changed_files.contains(&p.to_string()))
                        .unwrap_or(true)
                })
                .collect()
        } else {
            inline.iter().collect()
        };

        if !filtered_inline.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!(
                "{}\n",
                ui::bold(&format!("Inline comments ({})", filtered_inline.len()))
            ));
            for comment in &filtered_inline {
                let location = match (&comment.path, comment.line) {
                    (Some(path), Some(line)) => format!("{}:{}", path, line),
                    (Some(path), None) => path.clone(),
                    _ => "general".into(),
                };
                output.push_str(&format!(
                    "\n  {} — {} ({})\n    {}\n",
                    ui::highlight(&location),
                    comment.author,
                    format_state(&comment.state),
                    comment.body.trim().replace('\n', "\n    ")
                ));
            }
        }

        Ok(PrOutput(output.trim_end().to_string()))
    }
}

fn format_state(state: &ReviewState) -> &'static str {
    match state {
        ReviewState::Approved => "approved",
        ReviewState::ChangesRequested => "changes requested",
        ReviewState::Commented => "comment",
        ReviewState::Dismissed => "dismissed",
        ReviewState::Pending => "pending",
    }
}

fn changed_files_in_range(range: &str) -> Vec<String> {
    std::process::Command::new("git")
        .args(["diff", "--name-only", range])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default()
}
