//! Open worktree command

use clap::Args;
use std::path::PathBuf;

use jig_core::{git, terminal, Error, RepoContext, RepoRegistry};

use crate::op::{GlobalCtx, Op, RepoCtx};
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
}

impl Op for Open {
    type Error = OpenError;
    type Output = OpenOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        match ctx.repo() {
            Ok(repo) => self.open_in_repo(repo),
            Err(_) => {
                // Auto-detect: outside a git repo, fall back to global discovery
                let registry = RepoRegistry::load().unwrap_or_default();
                let repos: Vec<_> = registry
                    .repos()
                    .iter()
                    .filter(|e| e.path.exists())
                    .filter_map(|e| RepoContext::from_path(&e.path).ok())
                    .collect();
                let ctx = GlobalCtx { repos };
                self.run_global(&ctx)
            }
        }
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        let repo = if let Some(name) = self.name.as_deref() {
            ctx.repo_for_worktree(name)?
        } else {
            ctx.repos.first().ok_or(Error::NotInGitRepo)?
        };
        self.open_in_repo(repo)
    }
}

impl Open {
    fn open_in_repo(&self, repo: &RepoContext) -> Result<OpenOutput, OpenError> {
        if self.all {
            // Open all worktrees in new tabs
            let worktrees = git::list_worktree_names(&repo.worktrees_dir)?;

            if worktrees.is_empty() {
                eprintln!("No worktrees to open");
                return Ok(OpenOutput::None);
            }

            for wt_name in worktrees {
                let path = repo.worktrees_dir.join(&wt_name);
                if path.exists() {
                    let opened = terminal::open_tab(&path)?;
                    if opened {
                        ui::success(&format!("Opened '{}' in new tab", ui::highlight(&wt_name)));
                    }
                }
            }

            // Don't output cd command - tabs are opened directly
            Ok(OpenOutput::None)
        } else {
            // Open specific worktree
            let name = self.name.as_deref().ok_or(Error::NameRequired)?;
            let worktree_path = repo.worktrees_dir.join(name);

            if !worktree_path.exists() {
                return Err(Error::WorktreeNotFound(name.to_string()).into());
            }

            // Output cd command for shell wrapper to eval
            let canonical = worktree_path.canonicalize()?;
            Ok(OpenOutput::Cd(canonical))
        }
    }
}
