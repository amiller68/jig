//! Remove worktree command

use anyhow::Result;
use colored::Colorize;
use glob::Pattern;

use wt_core::{git, Error};

pub fn run(pattern: &str, force: bool) -> Result<()> {
    let worktrees_dir = git::get_worktrees_dir()?;
    let worktrees = git::list_worktrees()?;

    // Find matching worktrees
    let pattern = Pattern::new(pattern).map_err(|e| Error::Custom(format!("Invalid pattern: {}", e)))?;

    let matching: Vec<_> = worktrees
        .iter()
        .filter(|name| pattern.matches(name) || *name == &pattern.as_str())
        .cloned()
        .collect();

    if matching.is_empty() {
        // If not a pattern match, try exact match
        let exact_path = worktrees_dir.join(pattern.as_str());
        if exact_path.exists() {
            remove_single(&exact_path, pattern.as_str(), force)?;
            return Ok(());
        }
        return Err(Error::WorktreeNotFound(pattern.as_str().to_string()).into());
    }

    // Remove each matching worktree
    for name in matching {
        let path = worktrees_dir.join(&name);
        remove_single(&path, &name, force)?;
    }

    Ok(())
}

fn remove_single(path: &std::path::Path, name: &str, force: bool) -> Result<()> {
    // Check for uncommitted changes unless force
    if !force && git::has_uncommitted_changes(path)? {
        return Err(Error::UncommittedChanges.into());
    }

    git::remove_worktree(path, force)?;

    // Clean up empty parent directories (for nested paths)
    let mut parent = path.parent();
    let worktrees_dir = git::get_worktrees_dir()?;

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

    eprintln!("{} Removed worktree '{}'", "✓".green(), name.cyan());
    Ok(())
}
