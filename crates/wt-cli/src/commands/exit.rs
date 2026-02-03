//! Exit worktree command

use anyhow::Result;
use colored::Colorize;

use wt_core::{git, Error};

pub fn run(force: bool) -> Result<()> {
    // Check if we're in a worktree
    let name = git::get_current_worktree_name_auto()?
        .ok_or(Error::NotInWorktree)?;

    let worktrees_dir = git::get_worktrees_dir()?;
    let worktree_path = worktrees_dir.join(&name);
    let base_repo = git::get_base_repo()?;

    // Check for uncommitted changes unless force
    if !force && git::has_uncommitted_changes(&worktree_path)? {
        return Err(Error::UncommittedChanges.into());
    }

    // Remove the worktree
    git::remove_worktree(&worktree_path, force)?;

    // Clean up empty parent directories (for nested paths)
    let mut parent = worktree_path.parent();
    while let Some(p) = parent {
        if p == worktrees_dir {
            break;
        }
        if p.read_dir()?.next().is_none() {
            std::fs::remove_dir(p)?;
        } else {
            break;
        }
        parent = p.parent();
    }

    eprintln!("{} Exited worktree '{}'", "✓".green(), name.cyan());

    // Output cd command to base repo
    let canonical = base_repo.canonicalize()?;
    println!("cd '{}'", canonical.display());

    Ok(())
}
