//! Open worktree command

use clap::Args;
use std::path::PathBuf;

use jig_core::git::Repo;
use jig_core::{terminal, Config, Error, RepoRegistry};

use crate::op::{Op, RepoCtx};
use crate::ui;

/// Open/cd into a worktree
#[derive(Args, Debug, Clone)]
pub struct Open {
    /// Worktree name (or --all to open all in tabs)
    pub name: Option<String>,

    /// Open all worktrees in new tabs
    #[arg(long)]
    pub all: bool,
}

/// Output containing optional cd command
#[derive(Debug)]
pub enum OpenOutput {
    /// No output (when opening tabs)
    None,
    /// cd command to stdout
    Cd(PathBuf),
}

impl std::fmt::Display for OpenOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenOutput::None => Ok(()),
            OpenOutput::Cd(path) => write!(f, "cd '{}'", path.display()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
}

impl Op for Open {
    type Error = OpenError;
    type Output = OpenOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        match ctx.config() {
            Ok(cfg) => self.open_in_cfg(cfg),
            Err(_) => {
                // Auto-detect: outside a git repo, fall back to global discovery
                let registry = RepoRegistry::load().unwrap_or_default();
                let configs: Vec<_> = registry
                    .repos()
                    .iter()
                    .filter(|e| e.path.exists())
                    .filter_map(|e| Config::from_path(&e.path).ok())
                    .collect();
                let cfg = if let Some(name) = self.name.as_deref() {
                    configs
                        .iter()
                        .find(|c| c.worktrees_path.join(name).exists())
                        .ok_or(Error::WorktreeNotFound(name.to_string()))?
                } else {
                    configs.first().ok_or(Error::NotInGitRepo)?
                };
                self.open_in_cfg(cfg)
            }
        }
    }
}

impl Open {
    fn open_in_cfg(&self, cfg: &Config) -> Result<OpenOutput, OpenError> {
        if self.all {
            // Open all worktrees in new tabs
            let git_repo = Repo::open(&cfg.repo_root)?;
            let worktrees = git_repo.list_worktrees()?;

            if worktrees.is_empty() {
                eprintln!("No worktrees to open");
                return Ok(OpenOutput::None);
            }

            for wt in worktrees {
                let opened = terminal::open_tab(&wt.path())?;
                if opened {
                    ui::success(&format!(
                        "Opened '{}' in new tab",
                        ui::highlight(&wt.name())
                    ));
                }
            }

            // Don't output cd command - tabs are opened directly
            Ok(OpenOutput::None)
        } else {
            // Open specific worktree
            let name = self.name.as_deref().ok_or(Error::NameRequired)?;
            let worktree_path = cfg.worktrees_path.join(name);

            if !worktree_path.exists() {
                return Err(Error::WorktreeNotFound(name.to_string()).into());
            }

            // Output cd command for shell wrapper to eval
            let canonical = worktree_path.canonicalize()?;
            Ok(OpenOutput::Cd(canonical))
        }
    }
}
