//! Create worktree command

use anyhow::Result;
use colored::Colorize;

use wt_core::{config, git, Error};

pub fn run(name: &str, branch: Option<&str>, open: bool, no_hooks: bool) -> Result<()> {
    let worktrees_dir = git::get_worktrees_dir()?;
    let worktree_path = worktrees_dir.join(name);

    // Check if already exists
    if worktree_path.exists() {
        return Err(Error::WorktreeExists(name.to_string()).into());
    }

    // Determine branch name
    let branch = branch.unwrap_or(name);
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
        name.cyan(),
        branch.cyan()
    );

    // Run on-create hook unless --no-hooks
    if !no_hooks {
        config::run_on_create_hook_for_repo(&worktree_path)?;
    }

    // Output cd command if -o flag
    if open {
        // Canonicalize path for cd command
        let canonical = worktree_path.canonicalize()?;
        println!("cd '{}'", canonical.display());
    }

    Ok(())
}
