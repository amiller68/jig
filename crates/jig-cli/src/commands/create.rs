//! Create worktree command

use clap::Args;
use colored::Colorize;
use std::path::PathBuf;

use jig_core::git::Repo;
use jig_core::{config, git, Error};

use crate::op::{Op, RepoCtx};

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
        let worktree_path = repo.worktrees_dir.join(&self.name);

        // Check if already exists
        if worktree_path.exists() {
            return Err(Error::WorktreeExists(self.name.clone()).into());
        }

        // Determine branch name
        let branch = self.branch.as_deref().unwrap_or(&self.name);

        // Ensure .jig is gitignored
        git::ensure_worktrees_excluded(&repo.git_common_dir)?;

        // Create parent directories if needed (for nested paths like feature/auth/login)
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create the worktree
        let base = self.base.as_deref().unwrap_or(&repo.base_branch);
        let git_repo = Repo::discover()?;
        git_repo.create_worktree(&worktree_path, branch, base)?;

        eprintln!(
            "{} Created worktree '{}' on branch '{}'",
            "✓".green(),
            self.name.cyan(),
            branch.cyan()
        );

        // Copy configured files (e.g., .env)
        let copy_files = config::get_copy_files(&repo.repo_root)?;
        if !copy_files.is_empty() {
            config::copy_worktree_files(&repo.repo_root, &worktree_path, &copy_files)?;
            for file in &copy_files {
                if repo.repo_root.join(file).exists() {
                    eprintln!("  {} Copied {}", "→".dimmed(), file);
                }
            }
        }

        // Run on-create hook unless --no-hooks
        if !self.no_hooks {
            config::run_on_create_hook_for_repo(&repo.repo_root, &worktree_path)?;
        }

        // Output cd command if -o flag
        if self.open {
            // Canonicalize path for cd command
            let canonical = worktree_path.canonicalize()?;
            Ok(CreateOutput::Cd(canonical))
        } else {
            Ok(CreateOutput::None)
        }
    }
}
