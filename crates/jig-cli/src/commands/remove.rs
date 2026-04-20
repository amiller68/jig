//! Remove worktree command

use clap::Args;
use glob::Pattern;

use jig_core::git::Repo;
use jig_core::{Error, RepoContext, Worker};

use crate::op::{GlobalCtx, NoOutput, Op, RepoCtx};
use crate::ui;

/// Remove worktree(s)
#[derive(Args, Debug, Clone)]
pub struct Remove {
    /// Worktree name or glob pattern
    pub pattern: String,

    /// Force removal even with uncommitted changes
    #[arg(long, short)]
    pub force: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RemoveError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error("Invalid pattern: {0}")]
    InvalidPattern(#[from] glob::PatternError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
}

impl Op for Remove {
    type Error = RemoveError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;
        self.remove_from_repo(repo)
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo_for_worktree(&self.pattern)?;
        self.remove_from_repo(repo)
    }
}

impl Remove {
    fn remove_from_repo(&self, repo: &RepoContext) -> Result<NoOutput, RemoveError> {
        let git_repo = Repo::open(&repo.repo_root)?;
        let worktrees = git_repo.list_worktrees()?;
        let names: Vec<String> = worktrees.iter().map(|wt| wt.name()).collect();

        // Find matching worktrees
        let pattern = Pattern::new(&self.pattern)?;

        let matching: Vec<_> = names
            .iter()
            .filter(|name| pattern.matches(name.as_str()) || name.as_str() == pattern.as_str())
            .cloned()
            .collect();

        if matching.is_empty() {
            // If not a pattern match, try exact match
            let exact_path = repo.worktrees_path.join(pattern.as_str());
            if exact_path.exists() {
                let wt = Worker::open(&repo.repo_root, &repo.worktrees_path, pattern.as_str())?;
                wt.remove(self.force)?;
                ui::success(&format!(
                    "Removed worktree '{}'",
                    ui::highlight(pattern.as_str())
                ));
                return Ok(NoOutput);
            }
            return Err(Error::WorktreeNotFound(pattern.as_str().to_string()).into());
        }

        // Remove each matching worktree
        for name in matching {
            let wt = Worker::open(&repo.repo_root, &repo.worktrees_path, &name)?;
            wt.remove(self.force)?;
            ui::success(&format!("Removed worktree '{}'", ui::highlight(&name)));
        }

        Ok(NoOutput)
    }
}
