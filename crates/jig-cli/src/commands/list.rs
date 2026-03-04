//! List worktrees command

use clap::Args;
use colored::Colorize;

use jig_core::git;

use crate::op::{GlobalCtx, Op, RepoCtx};

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

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        if self.all {
            return self.list_all_git_worktrees();
        }

        let repo = ctx.repo()?;
        let worktrees = git::list_worktree_names(&repo.worktrees_dir)?;
        if worktrees.is_empty() {
            eprintln!("No worktrees found");
        }
        Ok(ListOutput(worktrees))
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        if self.all {
            return self.list_all_git_worktrees();
        }

        let mut all_worktrees = Vec::new();
        for repo in &ctx.repos {
            let worktrees = git::list_worktree_names(&repo.worktrees_dir)?;
            all_worktrees.extend(worktrees);
        }
        Ok(ListOutput(all_worktrees))
    }
}

impl List {
    fn list_all_git_worktrees(&self) -> Result<ListOutput, ListError> {
        let worktrees = git::list_all_worktrees()?;
        for (path, branch) in &worktrees {
            let branch_display = if branch.is_empty() {
                "(detached)".dimmed().to_string()
            } else {
                branch.cyan().to_string()
            };
            eprintln!("{} {}", path.display(), branch_display);
        }
        Ok(ListOutput(vec![]))
    }
}
