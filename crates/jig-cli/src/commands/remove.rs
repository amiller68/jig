//! Remove worktree command

use clap::Args;
use glob::Pattern;

use jig_core::worktree::Worktree;
use jig_core::{git, Error, RepoContext};

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
        let worktrees = git::list_worktree_names(&repo.worktrees_dir)?;

        // Find matching worktrees
        let pattern = Pattern::new(&self.pattern)?;

        let matching: Vec<_> = worktrees
            .iter()
            .filter(|name| pattern.matches(name) || *name == pattern.as_str())
            .cloned()
            .collect();

        if matching.is_empty() {
            // If not a pattern match, try exact match
            let exact_path = repo.worktrees_dir.join(pattern.as_str());
            if exact_path.exists() {
                let wt = Worktree::open(&repo.repo_root, &repo.worktrees_dir, pattern.as_str())?;
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
            let wt = Worktree::open(&repo.repo_root, &repo.worktrees_dir, &name)?;
            wt.remove(self.force)?;
            ui::success(&format!("Removed worktree '{}'", ui::highlight(&name)));
        }

        Ok(NoOutput)
    }
}
