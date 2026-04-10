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

    /// Get the HEAD commit SHA as a hex string.
    pub fn head_sha(&self) -> Result<String> {
        let head = self.inner.head()?;
        head.target()
            .map(|oid| oid.to_string())
            .ok_or_else(|| Error::BranchNotFound("HEAD".to_string()))
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
    ///
    /// If `repo_root` is provided, uses `Repo::open(repo_root)` instead of
    /// falling back to `Repo::discover()` when the worktree path doesn't exist.
    pub fn remove_worktree(path: &Path, force: bool, repo_root: Option<&Path>) -> Result<()> {
        if !force && path.exists() && Self::has_uncommitted_changes(path)? {
            return Err(Error::UncommittedChanges);
        }

        // We need the main repo to manipulate worktrees.
        let repo = if path.exists() {
            let wt_repo = Self::open(path)?;
            let main_workdir = wt_repo.base_repo_dir();
            Self::open(&main_workdir)?
        } else if let Some(root) = repo_root {
            Self::open(root)?
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
    // Remote operations
    // ------------------------------------------------------------------

    /// Fast-forward the current branch to match its remote tracking branch.
    ///
    /// Returns `true` if new commits were pulled, `false` if already up to date.
    /// Returns `Err(MergeConflict)` if fast-forward is not possible.
    pub fn fast_forward_to_remote(&self, branch: &str) -> Result<bool> {
        let remote_ref = format!("origin/{}", branch);
        let remote_branch = self
            .inner
            .find_branch(&remote_ref, git2::BranchType::Remote)
            .map_err(|_| Error::BranchNotFound(remote_ref.clone()))?;

        let annotated = self
            .inner
            .reference_to_annotated_commit(&remote_branch.into_reference())?;
        let (analysis, _) = self.inner.merge_analysis(&[&annotated])?;

        if analysis.is_up_to_date() {
            return Ok(false);
        }

        if analysis.is_fast_forward() {
            let target_oid = annotated.id();
            let mut reference = self.inner.head()?;
            reference.set_target(
                target_oid,
                &format!("fast-forward {} to {}", branch, remote_ref),
            )?;
            self.inner
                .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
            return Ok(true);
        }

        Err(Error::MergeConflict(remote_ref))
    }

    /// Fast-forward a local branch ref to match its remote tracking branch,
    /// without requiring the branch to be checked out or a worktree to exist.
    ///
    /// This operates directly on the branch reference (refs/heads/<branch>),
    /// so it works from the main repo even when no worktree exists for the branch.
    ///
    /// Returns `true` if the ref was advanced, `false` if already up to date.
    /// Returns `Err(MergeConflict)` if the local ref is not an ancestor of remote.
    pub fn fast_forward_branch_ref(&self, branch: &str) -> Result<bool> {
        let remote_ref_name = format!("origin/{}", branch);
        let remote_branch = self
            .inner
            .find_branch(&remote_ref_name, git2::BranchType::Remote)
            .map_err(|_| Error::BranchNotFound(remote_ref_name.clone()))?;

        let remote_oid = remote_branch
            .get()
            .target()
            .ok_or_else(|| Error::BranchNotFound(remote_ref_name.clone()))?;

        let local_ref_name = format!("refs/heads/{}", branch);

        // Check if local branch exists
        let local_oid = match self.inner.find_reference(&local_ref_name) {
            Ok(r) => match r.target() {
                Some(oid) => oid,
                None => return Err(Error::BranchNotFound(branch.to_string())),
            },
            Err(_) => {
                // Local branch doesn't exist — create it pointing at remote
                self.inner.reference(
                    &local_ref_name,
                    remote_oid,
                    false,
                    &format!("create {} from {}", branch, remote_ref_name),
                )?;
                return Ok(true);
            }
        };

        // Already up to date
        if local_oid == remote_oid {
            return Ok(false);
        }

        // Verify fast-forward: local must be ancestor of remote
        if !self.inner.graph_descendant_of(remote_oid, local_oid)? {
            return Err(Error::MergeConflict(remote_ref_name));
        }

        // Advance the local ref
        self.inner.reference(
            &local_ref_name,
            remote_oid,
            true,
            &format!("fast-forward {} to {}", branch, remote_ref_name),
        )?;

        Ok(true)
    }

    /// Push a branch to origin using a subprocess.
    ///
    /// Uses `git push origin <branch>` from the given repo path.
    /// This is used after bare ref updates (no worktree) to sync with the remote.
    pub fn push_branch(repo_path: &std::path::Path, branch: &str) -> Result<()> {
        let output = std::process::Command::new("git")
            .args(["push", "origin", branch])
            .current_dir(repo_path)
            .stdin(std::process::Stdio::null())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(Error::Custom(format!(
                "git push origin {} failed: {}",
                branch, stderr
            )));
        }

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    /// Helper: create a git repo with an initial commit.
    fn init_repo(dir: &Path) {
        Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "init", "-q"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// Helper: add self as remote "origin" (for fetch to work locally).
    fn add_self_remote(dir: &Path) {
        let path_str = dir.to_string_lossy().to_string();
        Command::new("git")
            .args(["remote", "add", "origin", &path_str])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// Helper: create a bare "upstream" repo that acts as origin.
    fn create_bare_upstream(dir: &Path) -> TempDir {
        let upstream = TempDir::new().unwrap();
        Command::new("git")
            .args(["clone", "--bare", "-q"])
            .arg(dir)
            .arg(upstream.path())
            .output()
            .unwrap();

        // Remove existing origin if any, then add pointing to bare repo
        let _ = Command::new("git")
            .args(["remote", "remove", "origin"])
            .current_dir(dir)
            .output();
        Command::new("git")
            .args(["remote", "add", "origin"])
            .arg(upstream.path())
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir)
            .output()
            .unwrap();

        upstream
    }

    #[test]
    fn fast_forward_branch_ref_up_to_date() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        add_self_remote(dir.path());

        Command::new("git")
            .args(["branch", "parent-branch"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let repo = Repo::open(dir.path()).unwrap();
        let result = repo.fast_forward_branch_ref("parent-branch").unwrap();
        assert!(!result, "should be up to date");
    }

    #[test]
    fn fast_forward_branch_ref_advances_ref() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());

        // Create parent-branch at the initial commit
        Command::new("git")
            .args(["branch", "parent-branch"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Add a new commit on main
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "child-work", "-q"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        add_self_remote(dir.path());

        // Get SHAs
        let head_sha = String::from_utf8_lossy(
            &Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(dir.path())
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();

        let parent_sha = String::from_utf8_lossy(
            &Command::new("git")
                .args(["rev-parse", "HEAD~1"])
                .current_dir(dir.path())
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();

        // Set origin/parent-branch to HEAD (new commit)
        Command::new("git")
            .args(["branch", "-f", "parent-branch", &head_sha])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Reset local parent-branch to old commit
        Command::new("git")
            .args(["branch", "-f", "parent-branch", &parent_sha])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let repo = Repo::open(dir.path()).unwrap();
        let result = repo.fast_forward_branch_ref("parent-branch").unwrap();
        assert!(result, "should have advanced");

        // Verify local matches remote
        let local_ref = repo
            .inner
            .find_reference("refs/heads/parent-branch")
            .unwrap();
        let remote_ref = repo
            .inner
            .find_branch("origin/parent-branch", git2::BranchType::Remote)
            .unwrap();
        assert_eq!(local_ref.target(), remote_ref.get().target());
    }

    #[test]
    fn fast_forward_branch_ref_creates_missing_local() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        add_self_remote(dir.path());

        // Create and fetch so origin/feature exists
        Command::new("git")
            .args(["branch", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Delete local branch
        Command::new("git")
            .args(["branch", "-D", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let repo = Repo::open(dir.path()).unwrap();
        let result = repo.fast_forward_branch_ref("feature").unwrap();
        assert!(result, "should have created local branch");

        // Verify local branch was created matching remote
        let local_ref = repo.inner.find_reference("refs/heads/feature").unwrap();
        let remote_ref = repo
            .inner
            .find_branch("origin/feature", git2::BranchType::Remote)
            .unwrap();
        assert_eq!(local_ref.target(), remote_ref.get().target());
    }

    #[test]
    fn fast_forward_branch_ref_diverged_errors() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());

        Command::new("git")
            .args(["branch", "parent-branch"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        add_self_remote(dir.path());
        Command::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Add a local-only commit to parent-branch
        Command::new("git")
            .args(["checkout", "-q", "parent-branch"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "local-only", "-q"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["checkout", "-q", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // origin/parent-branch is at init commit, local is at init+1
        // origin is NOT a descendant of local — it's an ancestor.
        // graph_descendant_of(remote, local) = false → MergeConflict
        let repo = Repo::open(dir.path()).unwrap();
        let result = repo.fast_forward_branch_ref("parent-branch");
        assert!(result.is_err(), "should error on diverged branches");
    }

    #[test]
    fn push_branch_works_with_bare_upstream() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        let _upstream = create_bare_upstream(dir.path());

        Command::new("git")
            .args(["checkout", "-q", "-b", "test-push"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "push-test", "-q"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Repo::push_branch(dir.path(), "test-push").unwrap();

        // Verify origin has the branch
        Command::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let repo = Repo::open(dir.path()).unwrap();
        let remote_branch = repo
            .inner
            .find_branch("origin/test-push", git2::BranchType::Remote);
        assert!(
            remote_branch.is_ok(),
            "remote branch should exist after push"
        );
    }
}
