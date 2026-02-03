//! Merge command - merge reviewed worktree into current branch

use anyhow::Result;
use colored::Colorize;

use wt_core::{git, spawn, Error};

pub fn run(name: &str) -> Result<()> {
    let worktrees_dir = git::get_worktrees_dir()?;
    let worktree_path = worktrees_dir.join(name);

    if !worktree_path.exists() {
        return Err(Error::WorktreeNotFound(name.to_string()).into());
    }

    // Check for uncommitted changes
    if git::has_uncommitted_changes(&worktree_path)? {
        return Err(Error::UncommittedChanges.into());
    }

    // Get branch name
    let branch = git::get_worktree_branch(&worktree_path)?;

    // Merge the branch
    git::merge_branch(&branch)?;

    eprintln!(
        "{} Merged branch '{}' into current branch",
        "✓".green(),
        branch.cyan()
    );

    // Unregister from spawn state
    spawn::unregister(name)?;

    // Kill tmux window if running
    spawn::kill_window(name)?;

    eprintln!();
    eprintln!(
        "  {} Remove worktree with: {}",
        "→".dimmed(),
        format!("scribe remove {}", name).cyan()
    );

    Ok(())
}
