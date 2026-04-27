//! Exit worktree command

use clap::Args;
use std::path::PathBuf;

use jig_core::Worktree;

use crate::op::{Op, RepoCtx};
use crate::ui;

/// Exit current worktree and remove it
#[derive(Args, Debug, Clone)]
pub struct Exit {
    /// Force removal even with uncommitted changes
    #[arg(long, short)]
    pub force: bool,
}

/// Output containing cd command to base repo
#[derive(Debug)]
pub struct ExitOutput(PathBuf);

impl std::fmt::Display for ExitOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cd '{}'", self.0.display())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExitError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
}

impl Op for Exit {
    type Error = ExitError;
    type Output = ExitOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let cfg = ctx.config()?;

        let wt = Worktree::current()?;
        let name = wt.name();

        wt.remove(self.force)?;

        ui::success(&format!("Exited worktree '{}'", ui::highlight(&name)));

        let canonical = cfg.repo_root.canonicalize()?;
        Ok(ExitOutput(canonical))
    }
}
