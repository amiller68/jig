//! Open worktree command

use clap::Args;
use std::path::PathBuf;

use crate::context::RepoConfig;
use crate::context::RepoRegistry;
use crate::cli::op::Op;

/// Open/cd into a worktree
#[derive(Args, Debug, Clone)]
pub struct Open {
    /// Branch name
    pub branch: Option<String>,
}

/// Output containing cd command
#[derive(Debug)]
pub struct OpenOutput(PathBuf);

impl std::fmt::Display for OpenOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cd '{}'", self.0.display())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Context(#[from] crate::context::ContextError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
    #[error("{0}")]
    Usage(String),
}

impl Op for Open {
    type Error = OpenError;
    type Output = OpenOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let name = self.branch.as_deref().ok_or(OpenError::Usage("branch is required".into()))?;

        let cfg = match RepoConfig::from_cwd() {
            Ok(cfg) => cfg,
            Err(_) => {
                let registry = RepoRegistry::load().unwrap_or_default();
                let configs: Vec<_> = registry
                    .repos()
                    .iter()
                    .filter(|e| e.path.exists())
                    .filter_map(|e| RepoConfig::from_path(&e.path).ok())
                    .collect();
                configs
                    .into_iter()
                    .find(|c| c.worktrees_path.join(name).exists())
                    .ok_or(OpenError::Usage(format!("worktree '{}' not found", name)))?
            }
        };

        let worktree_path = cfg.worktrees_path.join(name);
        if !worktree_path.exists() {
            return Err(OpenError::Usage(format!("worktree '{}' not found", name)));
        }

        let canonical = worktree_path.canonicalize()?;
        Ok(OpenOutput(canonical))
    }
}
