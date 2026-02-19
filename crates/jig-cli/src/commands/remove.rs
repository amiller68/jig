//! Remove worktree command

use clap::Args;
use colored::Colorize;
use glob::Pattern;
use std::path::Path;

use jig_core::{git, Error};

use crate::op::{NoOutput, Op, OpContext};

/// Remove worktree(s)
#[derive(Args, Debug, Clone)]
pub struct Remove {
    /// Worktree name or glob pattern
    pub pattern: String,

    /// Force removal even with uncommitted changes
    #[arg(long, short)]
    pub force: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RemoveError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error("Invalid pattern: {0}")]
    InvalidPattern(#[from] glob::PatternError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Op for Remove {
    type Error = RemoveError;
    type Output = NoOutput;

    fn execute(&self, _ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let worktrees_dir = git::get_worktrees_dir()?;
        let worktrees = git::list_worktrees()?;

        // Find matching worktrees
        let pattern = Pattern::new(&self.pattern)?;

        let matching: Vec<_> = worktrees
            .iter()
            .filter(|name| pattern.matches(name) || *name == pattern.as_str())
            .cloned()
            .collect();

        if matching.is_empty() {
            // If not a pattern match, try exact match
            let exact_path = worktrees_dir.join(pattern.as_str());
            if exact_path.exists() {
                remove_single(&exact_path, pattern.as_str(), self.force)?;
                return Ok(NoOutput);
            }
            return Err(Error::WorktreeNotFound(pattern.as_str().to_string()).into());
        }

        // Remove each matching worktree
        for name in matching {
            let path = worktrees_dir.join(&name);
            remove_single(&path, &name, self.force)?;
        }

        Ok(NoOutput)
    }
}

fn remove_single(path: &Path, name: &str, force: bool) -> Result<(), RemoveError> {
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
