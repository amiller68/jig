//! Attach command - attach to tmux session

use clap::Args;

use jig_core::spawn;

use crate::op::{GlobalCtx, NoOutput, Op, RepoCtx};

/// Attach to tmux session
#[derive(Args, Debug, Clone)]
pub struct Attach {
    /// Window name to switch to
    pub name: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Attach {
    type Error = AttachError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;
        spawn::attach(repo, self.name.as_deref())?;
        Ok(NoOutput)
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        let repo = if let Some(name) = self.name.as_deref() {
            ctx.repo_for_worktree(name)?
        } else {
            ctx.repos.first().ok_or(jig_core::Error::NotInGitRepo)?
        };
        spawn::attach(repo, self.name.as_deref())?;
        Ok(NoOutput)
    }
}
