//! List worktrees command

use clap::Args;
use colored::Colorize;

use jig_core::git;

use crate::op::{Op, OpContext};

/// List worktrees
#[derive(Args, Debug, Clone)]
pub struct List {
    /// Show all git worktrees (including base repo)
    #[arg(long)]
    pub all: bool,
}

/// Output containing worktree names (one per line)
#[derive(Debug)]
pub struct ListOutput(Vec<String>);

impl std::fmt::Display for ListOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for name in &self.0 {
            writeln!(f, "{}", name)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ListError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for List {
    type Error = ListError;
    type Output = ListOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        if self.all {
            // Show all git worktrees including base repo
            let worktrees = git::list_all_worktrees()?;

            for (path, branch) in &worktrees {
                let branch_display = if branch.is_empty() {
                    "(detached)".dimmed().to_string()
                } else {
                    branch.cyan().to_string()
                };
                eprintln!("{} {}", path.display(), branch_display);
            }
            // Don't output to stdout for --all mode
            Ok(ListOutput(vec![]))
        } else {
            // Show only .worktrees/
            let worktrees = git::list_worktrees()?;

            if worktrees.is_empty() {
                eprintln!("No worktrees found");
            }

            Ok(ListOutput(worktrees))
        }
    }
}
