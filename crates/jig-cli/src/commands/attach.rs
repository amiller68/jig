//! Attach command - attach to tmux session

use clap::Args;

use jig_core::{spawn, RepoContext, RepoRegistry};

use crate::op::{NoOutput, Op, RepoCtx};

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
        match ctx.repo() {
            Ok(repo) => {
                spawn::attach(repo, self.name.as_deref())?;
                Ok(NoOutput)
            }
            Err(_) => {
                // Auto-detect: outside a git repo, fall back to global discovery
                let name = self.name.as_deref().ok_or(jig_core::Error::NameRequired)?;
                let registry = RepoRegistry::load().unwrap_or_default();
                let repos: Vec<_> = registry
                    .repos()
                    .iter()
                    .filter(|e| e.path.exists())
                    .filter_map(|e| RepoContext::from_path(&e.path).ok())
                    .collect();
                let repo = repos
                    .iter()
                    .find(|r| r.worktrees_path.join(name).exists())
                    .ok_or(jig_core::Error::WorktreeNotFound(name.to_string()))?;
                spawn::attach(repo, Some(name))?;
                Ok(NoOutput)
            }
        }
    }
}
