//! Create worktree command

use clap::Args;
use colored::Colorize;
use std::path::PathBuf;

use jig_core::{config, git, Error};

use crate::op::{Op, OpContext};

/// Create a new worktree
#[derive(Args, Debug, Clone)]
pub struct Create {
    /// Worktree name
    pub name: String,

    /// Branch name (defaults to worktree name)
    pub branch: Option<String>,
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

    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let repo_root = git::get_base_repo()?;
        let worktrees_dir = git::get_worktrees_dir()?;
        let worktree_path = worktrees_dir.join(&self.name);

        // Check if already exists
        if worktree_path.exists() {
            return Err(Error::WorktreeExists(self.name.clone()).into());
        }

        // Determine branch name
        let branch = self.branch.as_deref().unwrap_or(&self.name);
        let base_branch = config::get_base_branch()?;

        // Ensure .worktrees is gitignored
        git::ensure_worktrees_excluded_auto()?;

        // Create parent directories if needed (for nested paths like feature/auth/login)
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create the worktree
        git::create_worktree(&worktree_path, branch, &base_branch)?;

        eprintln!(
            "{} Created worktree '{}' on branch '{}'",
            "✓".green(),
            self.name.cyan(),
            branch.cyan()
        );

        // Copy configured files (e.g., .env)
        let copy_files = config::get_copy_files()?;
        if !copy_files.is_empty() {
            config::copy_worktree_files(&repo_root, &worktree_path, &copy_files)?;
            for file in &copy_files {
                if repo_root.join(file).exists() {
                    eprintln!("  {} Copied {}", "→".dimmed(), file);
                }
            }
        }

        // Run on-create hook unless --no-hooks
        if !ctx.no_hooks {
            config::run_on_create_hook_for_repo(&worktree_path)?;
        }

        // Output cd command if -o flag
        if ctx.open {
            // Canonicalize path for cd command
            let canonical = worktree_path.canonicalize()?;
            Ok(CreateOutput::Cd(canonical))
        } else {
            Ok(CreateOutput::None)
        }
    }
}
