//! Create worktree command

use clap::Args;
use std::path::PathBuf;

use jig_core::worktree::Worktree;
use jig_core::{config, Error};

use crate::op::{Op, RepoCtx};
use crate::ui;

/// Create a new worktree
#[derive(Args, Debug, Clone)]
pub struct Create {
    /// Worktree name
    pub name: String,

    /// Branch name (defaults to worktree name)
    pub branch: Option<String>,

    /// Open/cd into worktree after creating
    #[arg(short = 'o')]
    pub open: bool,

    /// Base branch to create worktree from (overrides jig.toml default)
    #[arg(long, short = 'b')]
    pub base: Option<String>,

    /// Skip on-create hook execution
    #[arg(long = "no-hooks")]
    pub no_hooks: bool,
}

/// Output containing optional cd command
#[derive(Debug)]
pub enum CreateOutput {
    /// No output (created without -o flag)
    None,
    /// cd command to stdout
    Cd(PathBuf),
}

impl std::fmt::Display for CreateOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateOutput::None => Ok(()),
            CreateOutput::Cd(path) => write!(f, "cd '{}'", path.display()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Op for Create {
    type Error = CreateError;
    type Output = CreateOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;

        let base = self.base.as_deref().unwrap_or(&repo.base_branch);
        let copy_files = config::get_copy_files(&repo.repo_root)?;
        let on_create_hook = if self.no_hooks {
            None
        } else {
            config::get_on_create_hook(&repo.repo_root)?
        };

        let wt = Worktree::create(
            &repo.repo_root,
            &repo.worktrees_dir,
            &repo.git_common_dir,
            &self.name,
            self.branch.as_deref(),
            base,
            on_create_hook.as_deref(),
            &copy_files,
            false,
        )?;

        let branch = self.branch.as_deref().unwrap_or(&self.name);
        ui::success(&format!(
            "Created worktree '{}' on branch '{}'",
            ui::highlight(&self.name),
            ui::highlight(branch)
        ));

        // Log copied files
        for file in &copy_files {
            if repo.repo_root.join(file).exists() {
                ui::detail(&format!("Copied {}", file));
            }
        }

        // Output cd command if -o flag
        if self.open {
            let canonical = wt.path.canonicalize()?;
            Ok(CreateOutput::Cd(canonical))
        } else {
            Ok(CreateOutput::None)
        }
    }
}
