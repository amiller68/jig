//! Worktree operations
//!
//! High-level worktree management.

use std::path::{Path, PathBuf};

use crate::config::run_on_create_hook;
use crate::error::{Error, Result};
use crate::git;

/// Represents a git worktree
#[derive(Debug, Clone)]
pub struct Worktree {
    /// Name of the worktree (relative path from .worktrees/)
    pub name: String,
    /// Full path to the worktree
    pub path: PathBuf,
    /// Branch name
    pub branch: String,
}

impl Worktree {
    /// Create a new worktree
    pub fn create(
        worktrees_dir: &Path,
        git_common_dir: &Path,
        name: &str,
        branch: Option<&str>,
        base_branch: &str,
        on_create_hook: Option<&str>,
    ) -> Result<Self> {
        let worktree_path = worktrees_dir.join(name);

        // Check if already exists
        if worktree_path.exists() {
            return Err(Error::WorktreeExists(name.to_string()));
        }

        // Ensure .worktrees is gitignored
        git::ensure_worktrees_excluded(git_common_dir)?;

        // Create parent directories if needed (for nested paths like feature/auth/login)
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Determine branch name
        let branch = branch.unwrap_or(name);

        // Create the worktree
        git::create_worktree(&worktree_path, branch, base_branch)?;

        // Run on-create hook if configured
        if let Some(hook) = on_create_hook {
            run_on_create_hook(hook, &worktree_path)?;
        }

        Ok(Self {
            name: name.to_string(),
            path: worktree_path,
            branch: branch.to_string(),
        })
    }

    /// List all worktrees in a directory
    pub fn list(worktrees_dir: &Path) -> Result<Vec<Self>> {
        let names = git::list_worktree_names(worktrees_dir)?;

        names
            .into_iter()
            .map(|name| {
                let path = worktrees_dir.join(&name);
                let branch = git::get_worktree_branch(&path).unwrap_or_else(|_| name.clone());
                Ok(Self { name, path, branch })
            })
            .collect()
    }

    /// Open/get an existing worktree by name
    pub fn open(worktrees_dir: &Path, name: &str) -> Result<Self> {
        let path = worktrees_dir.join(name);

        if !path.exists() {
            return Err(Error::WorktreeNotFound(name.to_string()));
        }

        let branch = git::get_worktree_branch(&path)?;

        Ok(Self {
            name: name.to_string(),
            path,
            branch,
        })
    }

    /// Remove this worktree
    pub fn remove(&self, force: bool) -> Result<()> {
        // Check for uncommitted changes unless force
        if !force && git::has_uncommitted_changes(&self.path)? {
            return Err(Error::UncommittedChanges);
        }

        git::remove_worktree(&self.path, force)?;

        // Clean up empty parent directories (for nested paths)
        self.cleanup_empty_parents()?;

        Ok(())
    }

    /// Clean up empty parent directories
    fn cleanup_empty_parents(&self) -> Result<()> {
        let mut parent = self.path.parent();

        while let Some(p) = parent {
            // Stop if we've reached .worktrees directory
            if p.file_name().map(|n| n == ".worktrees").unwrap_or(false) {
                break;
            }

            // Stop if directory is not empty
            if p.read_dir()?.next().is_some() {
                break;
            }

            std::fs::remove_dir(p)?;
            parent = p.parent();
        }

        Ok(())
    }

    /// Check if this worktree has uncommitted changes
    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        git::has_uncommitted_changes(&self.path)
    }

    /// Get commits ahead of base branch
    pub fn get_commits_ahead(&self, base_branch: &str) -> Result<Vec<String>> {
        git::get_commits_ahead(&self.path, base_branch)
    }

    /// Get diff stats
    pub fn get_diff_stats(&self, base_branch: &str) -> Result<crate::worker::DiffStats> {
        git::get_diff_stats(&self.path, base_branch)
    }

    /// Get full diff
    pub fn get_diff(&self, base_branch: &str) -> Result<String> {
        git::get_diff(&self.path, base_branch)
    }

    /// Get diff stat (summary)
    pub fn get_diff_stat(&self, base_branch: &str) -> Result<String> {
        git::get_diff_stat(&self.path, base_branch)
    }
}
