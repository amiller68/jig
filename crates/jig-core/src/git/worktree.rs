//! Git worktree — a [`Repo`] that has been validated as a linked worktree.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::branch::Branch;
use super::diff::{Diff, Stats as DiffStats};
use super::error::{GitError, Result};
use super::Repo;

/// Lightweight serializable reference to a worktree on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeRef(PathBuf);

impl WorktreeRef {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    // TODO (cleanup): more foolproof check,
    //  such as seeing if we have a link to our root
    //  .git or something
    pub fn exists(&self) -> bool {
        self.0.exists()
    }

    pub fn open(&self) -> Result<Worktree> {
        Worktree::open(&self.0)
    }
}

impl std::ops::Deref for WorktreeRef {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl std::fmt::Display for WorktreeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.display().fmt(f)
    }
}

/// A validated linked git worktree. Guarantees the underlying repo
/// is a worktree, not the main clone.
pub struct Worktree {
    repo: Repo,
}

impl Worktree {
    /// Discover the worktree containing the current working directory.
    pub fn current() -> Result<Self> {
        let repo = Repo::discover()?;
        Self::validate(repo)
    }

    /// Open a worktree at `path`. Errors if it's not a linked worktree.
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repo::open(path)?;
        Self::validate(repo)
    }

    fn validate(repo: Repo) -> Result<Self> {
        if !repo.inner().is_worktree() {
            return Err(GitError::NotInWorktree);
        }
        Ok(Self { repo })
    }

    /// Create a git worktree on disk with full setup: git excludes,
    /// file copying, and on-create hook execution.
    pub fn create(repo: &Repo, branch: &Branch, base: &Branch) -> Result<Self> {
        crate::git::ensure_excluded(&repo.common_dir(), crate::config::JIG_DIR)?;

        let repo_root = repo.clone_path();
        let path = repo.create_worktree(branch, base)?;
        let wt = Self::open(&path)?;
        let wt_path = wt.path();

        let copy_files = crate::config::get_copy_files(&repo_root)
            .map_err(|e| GitError::Io(std::io::Error::other(e)))?;
        if !copy_files.is_empty() {
            crate::config::copy_worktree_files(&repo_root, &wt_path, &copy_files)
                .map_err(|e| GitError::Io(std::io::Error::other(e)))?;
        }

        crate::config::run_on_create_hook_for_repo(&repo_root, &wt_path)
            .map_err(|e| GitError::Io(std::io::Error::other(e)))?;

        Ok(wt)
    }

    pub fn as_ref(&self) -> WorktreeRef {
        WorktreeRef::new(self.path())
    }

    // ── Derived state ──

    pub fn path(&self) -> PathBuf {
        self.repo.root().expect("worktrees always have a workdir")
    }

    pub fn name(&self) -> String {
        let worktrees_path = self.repo.worktrees_path();
        self.path()
            .strip_prefix(&worktrees_path)
            .expect("worktree path must be under worktrees dir")
            .to_string_lossy()
            .to_string()
    }

    pub fn repo_root(&self) -> PathBuf {
        self.repo.clone_path()
    }

    pub fn head_sha(&self) -> Result<String> {
        Ok(self.repo.head_oid()?.to_string())
    }

    pub fn branch(&self) -> Result<Branch> {
        self.repo.current_branch()
    }

    pub fn base_branch(&self) -> Result<Branch> {
        self.repo.upstream_branch()
    }

    pub fn repo_name(&self) -> String {
        self.repo_root()
            .file_name()
            .expect("repo root must have a directory name")
            .to_string_lossy()
            .to_string()
    }

    // ── Operations ──

    /// Remove this worktree. Prunes empty parent directories up to (but
    /// not including) the directory named `stop_at`.
    pub fn remove(&self, force: bool) -> Result<()> {
        let worktrees_path = self.repo.worktrees_path();
        self.repo.remove(force)?;
        self.cleanup_empty_parents(&worktrees_path)?;
        Ok(())
    }

    // ── Git queries ──

    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        self.repo.has_uncommitted_changes()
    }

    pub fn commits_ahead(&self) -> Result<Vec<String>> {
        self.repo.commits_ahead(&self.base_branch()?)
    }

    pub fn diff(&self) -> Result<Diff<'_>> {
        self.repo.diff(&self.base_branch()?)
    }

    pub fn diff_stats(&self) -> Result<DiffStats> {
        self.diff()?.stats()
    }

    pub fn diff_stat(&self) -> Result<String> {
        self.diff()?.stat_string()
    }

    fn cleanup_empty_parents(&self, stop_at: &Path) -> Result<()> {
        let path = self.path();
        let mut parent = path.parent();
        while let Some(p) = parent {
            if p == stop_at {
                break;
            }
            if p.read_dir()?.next().is_some() {
                break;
            }
            std::fs::remove_dir(p)?;
            parent = p.parent();
        }
        Ok(())
    }
}
