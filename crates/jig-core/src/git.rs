//! Git operations
//!
//! Git operations using the git2 (libgit2) library, wrapped in a `Repo` struct.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::JIG_DIR;
use crate::error::{Error, Result};
use crate::worker::DiffStats;

/// Wrapper around `git2::Repository` providing jig-specific git operations.
pub struct Repo {
    inner: git2::Repository,
}

impl Repo {
    /// Open a repository by discovering from the current directory.
    pub fn discover() -> Result<Self> {
        let inner = git2::Repository::discover(".").map_err(|_| Error::NotInGitRepo)?;
        Ok(Self { inner })
    }

    /// Open a repository at a specific path.
    pub fn open(path: &Path) -> Result<Self> {
        let inner = git2::Repository::open(path)?;
        Ok(Self { inner })
    }

    /// Access the underlying git2::Repository.
    pub fn inner(&self) -> &git2::Repository {
        &self.inner
    }

    // ------------------------------------------------------------------
    // Repository info
    // ------------------------------------------------------------------

    /// Get the root directory (workdir) of the repository.
    pub fn root(&self) -> Result<PathBuf> {
        self.inner
            .workdir()
            .map(|p| p.to_path_buf())
            .ok_or(Error::NotInGitRepo)
    }

    /// Get the common git directory (handles worktrees correctly).
    pub fn common_dir(&self) -> PathBuf {
        self.inner.commondir().to_path_buf()
    }

    /// Get the base repository directory (even when in a worktree).
    pub fn base_repo_dir(&self) -> PathBuf {
        let cd = self.common_dir();
        cd.parent().unwrap_or(&cd).to_path_buf()
    }

    // ------------------------------------------------------------------
    // Branch operations
    // ------------------------------------------------------------------

    /// Check if a branch exists (local or remote).
    pub fn branch_exists(&self, branch: &str) -> Result<bool> {
        let branch = branch.strip_prefix("origin/").unwrap_or(branch);

        if self
            .inner
            .find_branch(branch, git2::BranchType::Local)
            .is_ok()
        {
            return Ok(true);
        }

        if self
            .inner
            .find_branch(&format!("origin/{}", branch), git2::BranchType::Remote)
            .is_ok()
        {
            return Ok(true);
        }

        Ok(false)
    }

    /// Get the current branch name.
    pub fn current_branch(&self) -> Result<String> {
        let head = self.inner.head()?;
        head.shorthand()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::BranchNotFound("HEAD".to_string()))
    }

    /// Get the branch checked out in a worktree at `path`.
    pub fn worktree_branch(path: &Path) -> Result<String> {
        let repo = Self::open(path)?;
        repo.current_branch()
    }

    // ------------------------------------------------------------------
    // Worktree operations
    // ------------------------------------------------------------------

    /// List all git worktrees (including base repo).
    pub fn list_all_worktrees(&self) -> Result<Vec<(PathBuf, String)>> {
        let mut worktrees = Vec::new();

        // Main worktree
        if let Some(workdir) = self.inner.workdir() {
            let branch = self
                .inner
                .head()
                .ok()
                .and_then(|h| h.shorthand().map(|s| s.to_string()))
                .unwrap_or_default();
            worktrees.push((workdir.to_path_buf(), branch));
        }

        // Linked worktrees
        if let Ok(wt_names) = self.inner.worktrees() {
            for i in 0..wt_names.len() {
                if let Some(name) = wt_names.get(i) {
                    if let Ok(wt) = self.inner.find_worktree(name) {
                        let wt_path = wt.path().to_path_buf();
                        let branch = if let Ok(wt_repo) = git2::Repository::open(&wt_path) {
                            wt_repo
                                .head()
                                .ok()
                                .and_then(|h| h.shorthand().map(|s| s.to_string()))
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        worktrees.push((wt_path, branch));
                    }
                }
            }
        }

        Ok(worktrees)
    }

    /// Create a git worktree.
    pub fn create_worktree(&self, path: &Path, branch: &str, base_branch: &str) -> Result<()> {
        self.prune_stale_worktrees();

        let exists = self.branch_exists(branch)?;

        let wt_name = path
            .file_name()
            .ok_or_else(|| Error::InvalidPath(path.to_path_buf()))?
            .to_string_lossy()
            .to_string();

        if exists {
            let branch_ref = self.inner.find_branch(branch, git2::BranchType::Local)?;
            let reference = branch_ref.into_reference();
            let mut opts = git2::WorktreeAddOptions::new();
            opts.reference(Some(&reference));
            self.inner.worktree(&wt_name, path, Some(&opts))?;
        } else {
            let start_commit = self.find_valid_start_point(base_branch)?;
            let new_branch = self.inner.branch(branch, &start_commit, false)?;
            let reference = new_branch.into_reference();
            let mut opts = git2::WorktreeAddOptions::new();
            opts.reference(Some(&reference));
            self.inner.worktree(&wt_name, path, Some(&opts))?;

            // Set up push tracking for new branches
            let wt_repo = Self::open(path)?;
            if let Ok(mut config) = wt_repo.inner.config() {
                let _ = config.set_bool("push.autoSetupRemote", true);
            }
        }

        Ok(())
    }

    /// Remove a git worktree.
    pub fn remove_worktree(path: &Path, force: bool) -> Result<()> {
        if !force && path.exists() && Self::has_uncommitted_changes(path)? {
            return Err(Error::UncommittedChanges);
        }

        // We need the main repo to manipulate worktrees.
        let repo = if path.exists() {
            let wt_repo = Self::open(path)?;
            let main_workdir = wt_repo.base_repo_dir();
            Self::open(&main_workdir)?
        } else {
            Self::discover()?
        };

        let wt_name = repo.find_worktree_name_for_path(path)?;
        let wt = repo.inner.find_worktree(&wt_name)?;

        let mut opts = git2::WorktreePruneOptions::new();
        opts.valid(true);
        opts.working_tree(true);
        if force {
            opts.locked(true);
        }

        wt.prune(Some(&mut opts))?;
        Ok(())
    }

    /// Prune stale (invalid) worktree registrations.
    pub fn prune_stale_worktrees(&self) {
        if let Ok(wt_names) = self.inner.worktrees() {
            for i in 0..wt_names.len() {
                if let Some(name) = wt_names.get(i) {
                    if let Ok(wt) = self.inner.find_worktree(name) {
                        if wt.validate().is_err() {
                            let mut opts = git2::WorktreePruneOptions::new();
                            let _ = wt.prune(Some(&mut opts));
                        }
                    }
                }
            }
        }
    }

    /// Find the worktree name that corresponds to a given filesystem path.
    pub fn find_worktree_name_for_path(&self, path: &Path) -> Result<String> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        let wt_names = self.inner.worktrees()?;
        for i in 0..wt_names.len() {
            if let Some(name) = wt_names.get(i) {
                if let Ok(wt) = self.inner.find_worktree(name) {
                    let wt_path = wt.path().to_path_buf();
                    let wt_canonical = wt_path.canonicalize().unwrap_or(wt_path);
                    if wt_canonical == canonical {
                        return Ok(name.to_string());
                    }
                }
            }
        }

        Err(Error::WorktreeNotFound(path.display().to_string()))
    }

    /// Prune a worktree by name, removing its working directory.
    pub fn prune_worktree(&self, name: &str) -> Result<()> {
        let wt = self.inner.find_worktree(name)?;
        let mut opts = git2::WorktreePruneOptions::new();
        opts.valid(true);
        opts.working_tree(true);
        wt.prune(Some(&mut opts))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Status & diff operations
    // ------------------------------------------------------------------

    /// Check if a worktree at `path` has uncommitted changes.
    pub fn has_uncommitted_changes(path: &Path) -> Result<bool> {
        let repo = Self::open(path)?;
        let statuses = repo.inner.statuses(Some(
            git2::StatusOptions::new()
                .include_untracked(true)
                .recurse_untracked_dirs(true),
        ))?;
        Ok(!statuses.is_empty())
    }

    /// Get commits ahead of base branch.
    pub fn commits_ahead(path: &Path, base_branch: &str) -> Result<Vec<String>> {
        let repo = Self::open(path)?;

        let base_oid = match repo.inner.revparse_single(base_branch) {
            Ok(obj) => match obj.peel(git2::ObjectType::Commit) {
                Ok(c) => c.id(),
                Err(_) => return Ok(Vec::new()),
            },
            Err(_) => return Ok(Vec::new()),
        };

        let head_oid = match repo.inner.head() {
            Ok(h) => match h.target() {
                Some(oid) => oid,
                None => return Ok(Vec::new()),
            },
            Err(_) => return Ok(Vec::new()),
        };

        let mut revwalk = repo.inner.revwalk()?;
        revwalk.push(head_oid)?;
        revwalk.hide(base_oid)?;
        revwalk.set_sorting(git2::Sort::TOPOLOGICAL)?;

        let mut commits = Vec::new();
        for oid_result in revwalk {
            let oid = oid_result?;
            let commit = repo.inner.find_commit(oid)?;
            let short_id = &oid.to_string()[..7];
            let summary = commit.summary().unwrap_or("");
            commits.push(format!("{} {}", short_id, summary));
        }

        Ok(commits)
    }

    /// Get diff stat for a worktree (formatted string like `git diff --stat`).
    pub fn diff_stat(path: &Path, base_branch: &str) -> Result<String> {
        let repo = Self::open(path)?;
        let diff = repo.make_diff(base_branch)?;
        let stats = diff.stats()?;
        let buf = stats.to_buf(git2::DiffStatsFormat::FULL, 80)?;
        Ok(std::str::from_utf8(&buf).unwrap_or("").to_string())
    }

    /// Get diff stats as structured data.
    pub fn diff_stats(path: &Path, base_branch: &str) -> Result<DiffStats> {
        let repo = Self::open(path)?;
        let diff = repo.make_diff(base_branch)?;

        let mut stats = DiffStats::default();
        let num_deltas = diff.deltas().len();

        for i in 0..num_deltas {
            if let Some(patch) = git2::Patch::from_diff(&diff, i)? {
                let (_, insertions, deletions) = patch.line_stats()?;
                let file_path = diff
                    .get_delta(i)
                    .and_then(|d| d.new_file().path().map(|p| p.to_string_lossy().to_string()))
                    .unwrap_or_default();

                stats.files_changed += 1;
                stats.insertions += insertions;
                stats.deletions += deletions;
                stats.files.push(crate::worker::FileDiff {
                    path: file_path,
                    insertions,
                    deletions,
                });
            }
        }

        Ok(stats)
    }

    /// Get full diff for a worktree.
    pub fn diff(path: &Path, base_branch: &str) -> Result<String> {
        let repo = Self::open(path)?;
        let diff = repo.make_diff(base_branch)?;

        let mut output = Vec::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let origin = line.origin();
            match origin {
                '+' | '-' | ' ' => output.push(origin as u8),
                // File headers, hunk headers, etc. — content already includes
                // the full line so we emit it without prepending the origin char.
                _ => {}
            }
            output.extend_from_slice(line.content());
            true
        })?;
        Ok(String::from_utf8_lossy(&output).to_string())
    }

    // ------------------------------------------------------------------
    // Merge
    // ------------------------------------------------------------------

    /// Merge a branch into the current branch.
    pub fn merge_branch(&self, branch: &str) -> Result<()> {
        let branch_ref = self
            .inner
            .find_branch(branch, git2::BranchType::Local)
            .or_else(|_| {
                self.inner
                    .find_branch(&format!("origin/{}", branch), git2::BranchType::Remote)
            })
            .map_err(|_| Error::BranchNotFound(branch.to_string()))?;

        let annotated = self
            .inner
            .reference_to_annotated_commit(&branch_ref.into_reference())?;
        let (analysis, _) = self.inner.merge_analysis(&[&annotated])?;

        if analysis.is_up_to_date() {
            return Ok(());
        }

        if analysis.is_fast_forward() {
            let target_oid = annotated.id();
            let mut reference = self.inner.head()?;
            reference.set_target(target_oid, &format!("merge {}: Fast-forward", branch))?;
            self.inner
                .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
            return Ok(());
        }

        if analysis.is_normal() {
            self.inner.merge(&[&annotated], None, None)?;

            let mut index = self.inner.index()?;
            if index.has_conflicts() {
                self.inner.cleanup_state()?;
                return Err(Error::MergeConflict(branch.to_string()));
            }

            let tree_oid = index.write_tree()?;
            let tree = self.inner.find_tree(tree_oid)?;
            let head_commit = self
                .inner
                .head()?
                .peel(git2::ObjectType::Commit)?
                .into_commit()
                .map_err(|_| git2::Error::from_str("HEAD is not a commit"))?;
            let merge_commit = self.inner.find_commit(annotated.id())?;
            let sig = self
                .inner
                .signature()
                .or_else(|_| git2::Signature::now("jig", "jig@localhost"))?;

            self.inner.commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("Merge branch '{}'", branch),
                &tree,
                &[&head_commit, &merge_commit],
            )?;
            self.inner.cleanup_state()?;
            return Ok(());
        }

        Err(Error::MergeConflict(branch.to_string()))
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Resolve a revspec to a commit.
    fn resolve_to_commit(&self, spec: &str) -> Result<git2::Commit<'_>> {
        let obj = self
            .inner
            .revparse_single(spec)
            .map_err(|_| Error::BranchNotFound(spec.to_string()))?;
        Ok(obj
            .peel(git2::ObjectType::Commit)?
            .into_commit()
            .map_err(|_| git2::Error::from_str("not a commit"))?)
    }

    /// Build a diff between a base branch and HEAD.
    fn make_diff(&self, base_branch: &str) -> Result<git2::Diff<'_>> {
        let base_tree = self.resolve_to_commit(base_branch)?.tree()?;
        let head_tree = self
            .inner
            .head()?
            .peel(git2::ObjectType::Commit)?
            .into_commit()
            .map_err(|_| git2::Error::from_str("HEAD is not a commit"))?
            .tree()?;

        Ok(self
            .inner
            .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?)
    }

    /// Find a valid start-point commit for creating a new branch.
    fn find_valid_start_point(&self, base_branch: &str) -> Result<git2::Commit<'_>> {
        if let Ok(commit) = self.resolve_to_commit(base_branch) {
            return Ok(commit);
        }

        // Try with origin/ prefix
        if !base_branch.starts_with("origin/") {
            if let Ok(commit) = self.resolve_to_commit(&format!("origin/{}", base_branch)) {
                return Ok(commit);
            }
        }

        // Fall back to HEAD
        self.resolve_to_commit("HEAD")
            .map_err(|_| Error::BranchNotFound(base_branch.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Free functions (filesystem-only, no git2)
// ---------------------------------------------------------------------------

/// Check if we're inside a worktree (not the main repo).
pub fn is_in_worktree(worktrees_dir: &Path) -> Result<bool> {
    let cwd = std::env::current_dir()?;
    Ok(cwd.starts_with(worktrees_dir))
}

/// Get current worktree name (if in one).
pub fn get_current_worktree_name(worktrees_dir: &Path) -> Result<Option<String>> {
    let cwd = std::env::current_dir()?;

    if !cwd.starts_with(worktrees_dir) {
        return Ok(None);
    }

    let mut current = cwd.as_path();
    while current.starts_with(worktrees_dir) && current != worktrees_dir {
        if current.join(".git").is_file() {
            let name = current
                .strip_prefix(worktrees_dir)
                .map_err(|_| Error::InvalidPath(current.to_path_buf()))?
                .to_string_lossy()
                .to_string();
            return Ok(Some(name));
        }
        current = current.parent().unwrap_or(current);
    }

    Ok(None)
}

/// List all worktree names in a directory.
pub fn list_worktree_names(worktrees_dir: &Path) -> Result<Vec<String>> {
    if !worktrees_dir.exists() {
        return Ok(Vec::new());
    }

    let mut worktrees = Vec::new();
    find_worktrees_recursive(worktrees_dir, worktrees_dir, &mut worktrees)?;

    worktrees.sort();
    Ok(worktrees)
}

fn find_worktrees_recursive(
    base: &Path,
    current: &Path,
    worktrees: &mut Vec<String>,
) -> Result<()> {
    if !current.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }

            if path.join(".git").is_file() {
                let name = path
                    .strip_prefix(base)
                    .map_err(|_| Error::InvalidPath(path.clone()))?
                    .to_string_lossy()
                    .to_string();
                worktrees.push(name);
            } else {
                find_worktrees_recursive(base, &path, worktrees)?;
            }
        }
    }

    Ok(())
}

/// Ensure jig directory is in git exclude.
pub fn ensure_worktrees_excluded(git_common_dir: &Path) -> Result<()> {
    let exclude_file = git_common_dir.join("info").join("exclude");
    let exclude_entry = format!("{}/", JIG_DIR);

    if !exclude_file.exists() {
        std::fs::create_dir_all(exclude_file.parent().unwrap())?;
        std::fs::write(&exclude_file, format!("{}\n", exclude_entry))?;
        return Ok(());
    }

    let content = std::fs::read_to_string(&exclude_file)?;
    if !content.contains(JIG_DIR) {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&exclude_file)?;
        writeln!(file, "{}", exclude_entry)?;
    }

    Ok(())
}
