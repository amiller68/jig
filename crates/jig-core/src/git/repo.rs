//! Git repository handle — thin wrapper around `git2::Repository`.

use std::path::{Path, PathBuf};

use super::branch::Branch;
use super::commit::Oid;
use super::diff::Diff;
use super::error::{GitError, Result};

use crate::config;

/// Wrapper around `git2::Repository` providing jig-specific git operations.
pub struct Repo {
    inner: git2::Repository,
}

impl Repo {
    /// Open a repository by discovering from the current directory.
    pub fn discover() -> Result<Self> {
        let inner = git2::Repository::discover(".").map_err(|_| GitError::NotInRepo)?;
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
            .ok_or(GitError::NotInRepo)
    }

    /// Get the common git directory (`.git/` for clones, `.git/worktrees/<name>/` for linked worktrees).
    pub fn common_dir(&self) -> PathBuf {
        self.inner.commondir().to_path_buf()
    }

    /// Path to the original clone's working directory.
    pub fn clone_path(&self) -> PathBuf {
        self.inner
            .commondir()
            .parent()
            .expect("git common dir must have a parent")
            .to_path_buf()
    }

    /// Whether this repo is the original clone (not a linked worktree).
    pub fn is_clone(&self) -> bool {
        !self.inner.is_worktree()
    }

    /// Open the original clone repo. Re-opens at root if already the clone.
    pub fn open_clone(&self) -> Result<Self> {
        if self.is_clone() {
            Self::open(&self.root()?)
        } else {
            Self::open(&self.clone_path())
        }
    }

    /// Jig worktrees directory (`.jig/` under clone root).
    pub fn worktrees_path(&self) -> PathBuf {
        self.clone_path().join(config::JIG_DIR)
    }

    // ------------------------------------------------------------------
    // Remote operations
    // ------------------------------------------------------------------

    /// Fetch from a remote. If `refspecs` is empty, fetches all refs.
    pub fn fetch(&self, remote: &str, refspecs: &[&str]) -> Result<()> {
        let mut remote = self.inner.find_remote(remote)?;
        remote.fetch(refspecs, None, None)?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Branch operations
    // ------------------------------------------------------------------

    /// Check if a branch exists (local or remote).
    pub fn remote_branch_exists(&self, branch: &Branch) -> bool {
        let name: &str = branch;
        let remote_ref = if name.starts_with("origin/") {
            name.to_string()
        } else {
            format!("origin/{}", name)
        };
        self.inner
            .find_branch(&remote_ref, git2::BranchType::Remote)
            .is_ok()
    }

    pub fn branch_exists(&self, branch: &Branch) -> Result<bool> {
        let name: &str = branch;
        let local = name.strip_prefix("origin/").unwrap_or(name);

        if self
            .inner
            .find_branch(local, git2::BranchType::Local)
            .is_ok()
        {
            return Ok(true);
        }
        if self
            .inner
            .find_branch(&format!("origin/{}", local), git2::BranchType::Remote)
            .is_ok()
        {
            return Ok(true);
        }

        Ok(false)
    }

    /// Get the HEAD commit OID.
    pub fn head_oid(&self) -> Result<Oid> {
        let head = self.inner.head()?;
        head.target()
            .map(Oid::new)
            .ok_or_else(|| GitError::BranchNotFound("HEAD".to_string()))
    }

    /// Get the current branch name.
    pub fn current_branch(&self) -> Result<Branch> {
        let head = self.inner.head()?;
        head.shorthand()
            .map(Branch::new)
            .ok_or_else(|| GitError::BranchNotFound("HEAD".to_string()))
    }

    /// Get the upstream (tracking) branch for the current HEAD.
    pub fn upstream_branch(&self) -> Result<Branch> {
        let head = self.inner.head()?;
        let branch_name = head
            .shorthand()
            .ok_or_else(|| GitError::BranchNotFound("HEAD".to_string()))?;
        let local = self
            .inner
            .find_branch(branch_name, git2::BranchType::Local)?;
        let upstream = local
            .upstream()
            .map_err(|_| GitError::BranchNotFound(format!("upstream of {}", branch_name)))?;
        let name = upstream
            .name()?
            .ok_or_else(|| GitError::BranchNotFound("upstream".to_string()))?;
        Ok(Branch::new(name))
    }

    // ------------------------------------------------------------------
    // Worktree operations
    // ------------------------------------------------------------------

    /// List all linked worktrees (not the main clone).
    pub fn list_worktrees(&self) -> Result<Vec<super::Worktree>> {
        let wt_names = self.inner.worktrees()?;
        let mut worktrees = Vec::with_capacity(wt_names.len());
        for i in 0..wt_names.len() {
            let Some(name) = wt_names.get(i) else {
                continue;
            };
            let Ok(wt) = self.inner.find_worktree(name) else {
                continue;
            };
            let Ok(worktree) = super::Worktree::open(&wt.path().to_path_buf()) else {
                continue;
            };
            worktrees.push(worktree);
        }
        Ok(worktrees)
    }

    /// Create a worktree for `branch`, forking from `base` if the branch
    /// doesn't exist yet. Ensures the base branch is also checked out.
    pub fn create_worktree(&self, branch: &Branch, base: &Branch) -> Result<PathBuf> {
        self.prune_stale_worktrees();

        if self.is_branch_checked_out(branch)? {
            return Err(GitError::WorktreeExists(branch.to_string()));
        }

        self.ensure_worktree(base)?;

        let branch_str: &str = branch;
        let path = self.worktrees_path().join(branch_str);
        self.add_worktree(&path, branch, base)?;

        Ok(path)
    }

    /// Remove this repo's worktree registration. Errors if not a linked worktree.
    pub fn remove(&self, force: bool) -> Result<()> {
        if !self.inner.is_worktree() {
            return Err(GitError::NotInWorktree);
        }
        if !force && self.has_uncommitted_changes()? {
            return Err(GitError::UncommittedChanges);
        }

        let branch = self.current_branch()?;
        let clone = self.open_clone()?;
        clone.prune_worktree(&branch, force)?;
        Ok(())
    }

    /// Prune stale (invalid) worktree registrations.
    pub fn prune_stale_worktrees(&self) {
        self.for_each_worktree(|_name, wt| {
            if wt.validate().is_err() {
                let mut opts = git2::WorktreePruneOptions::new();
                let _ = wt.prune(Some(&mut opts));
            }
            Ok(())
        })
        .ok();
    }

    /// Prune a worktree by its branch name.
    pub fn prune_worktree(&self, branch: &Branch, force: bool) -> Result<()> {
        let name: &str = branch;
        let local = name.strip_prefix("origin/").unwrap_or(name);
        self.prune_worktree_named(local, force)
    }

    // ------------------------------------------------------------------
    // Status & diff
    // ------------------------------------------------------------------

    /// Check for uncommitted changes.
    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        let statuses = self.inner.statuses(Some(
            git2::StatusOptions::new()
                .include_untracked(true)
                .recurse_untracked_dirs(true),
        ))?;
        Ok(!statuses.is_empty())
    }

    /// Get commits ahead of a base branch.
    pub fn commits_ahead(&self, base: &Branch) -> Result<Vec<String>> {
        let base_str: &str = base;
        let base_oid = match self.inner.revparse_single(base_str) {
            Ok(obj) => match obj.peel(git2::ObjectType::Commit) {
                Ok(c) => c.id(),
                Err(_) => return Ok(Vec::new()),
            },
            Err(_) => return Ok(Vec::new()),
        };

        let head_oid = match self.head_oid() {
            Ok(oid) => oid,
            Err(_) => return Ok(Vec::new()),
        };

        let mut revwalk = self.inner.revwalk()?;
        revwalk.push(head_oid.inner())?;
        revwalk.hide(base_oid)?;
        revwalk.set_sorting(git2::Sort::TOPOLOGICAL)?;

        let mut commits = Vec::new();
        for oid_result in revwalk {
            let oid = oid_result?;
            let commit = self.inner.find_commit(oid)?;
            let short_id = &oid.to_string()[..7];
            let summary = commit.summary().unwrap_or("");
            commits.push(format!("{} {}", short_id, summary));
        }

        Ok(commits)
    }

    /// Get the diff against a base branch.
    pub fn diff(&self, base: &Branch) -> Result<Diff<'_>> {
        let base_str: &str = base;
        let base_tree = self.resolve_to_commit(base_str)?.tree()?;
        let head_tree = self
            .inner
            .head()?
            .peel(git2::ObjectType::Commit)?
            .into_commit()
            .map_err(|_| git2::Error::from_str("HEAD is not a commit"))?
            .tree()?;

        let raw = self
            .inner
            .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;
        Ok(Diff::new(raw))
    }

    // ------------------------------------------------------------------
    // Merge
    // ------------------------------------------------------------------

    /// Merge a branch into the current HEAD.
    pub fn merge_branch(&self, branch: &Branch) -> Result<()> {
        let name: &str = branch;
        let local = name.strip_prefix("origin/").unwrap_or(name);

        let branch_ref = self
            .inner
            .find_branch(local, git2::BranchType::Local)
            .or_else(|_| {
                self.inner
                    .find_branch(&format!("origin/{}", local), git2::BranchType::Remote)
            })
            .map_err(|_| GitError::BranchNotFound(branch.to_string()))?;

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
                return Err(GitError::MergeConflict(branch.to_string()));
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

        Err(GitError::MergeConflict(branch.to_string()))
    }

    // ------------------------------------------------------------------
    // Remote operations
    // ------------------------------------------------------------------

    /// Fast-forward the current branch to match its remote tracking ref.
    pub fn fast_forward(&self) -> Result<bool> {
        let branch = self.current_branch()?;
        self.fast_forward_branch(&branch, true)
    }

    /// Fast-forward a local branch to match its remote tracking ref.
    ///
    /// When `checkout` is true, also updates the working tree (use when the
    /// branch is currently checked out). When false, only the ref is moved.
    /// Creates the local branch if it doesn't exist yet.
    ///
    /// Returns `true` if the ref was advanced, `false` if already up to date.
    pub fn fast_forward_branch(&self, branch: &Branch, checkout: bool) -> Result<bool> {
        let name: &str = branch;
        let local = name.strip_prefix("origin/").unwrap_or(name);
        let remote_ref_name = format!("origin/{}", local);

        let remote_branch = self
            .inner
            .find_branch(&remote_ref_name, git2::BranchType::Remote)
            .map_err(|_| GitError::BranchNotFound(remote_ref_name.clone()))?;

        let remote_oid = remote_branch
            .get()
            .target()
            .ok_or_else(|| GitError::BranchNotFound(remote_ref_name.clone()))?;

        let local_ref_name = format!("refs/heads/{}", local);

        let local_oid = match self.inner.find_reference(&local_ref_name) {
            Ok(r) => match r.target() {
                Some(oid) => oid,
                None => return Err(GitError::BranchNotFound(local.to_string())),
            },
            Err(_) => {
                self.inner.reference(
                    &local_ref_name,
                    remote_oid,
                    false,
                    &format!("create {} from {}", local, remote_ref_name),
                )?;
                if checkout {
                    self.inner
                        .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
                }
                return Ok(true);
            }
        };

        if local_oid == remote_oid {
            return Ok(false);
        }

        if !self.inner.graph_descendant_of(remote_oid, local_oid)? {
            return Err(GitError::MergeConflict(remote_ref_name));
        }

        self.inner.reference(
            &local_ref_name,
            remote_oid,
            true,
            &format!("fast-forward {} to {}", local, remote_ref_name),
        )?;

        if checkout {
            self.inner
                .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        }

        Ok(true)
    }

    /// Push a branch to origin.
    pub fn push_branch(&self, branch: &Branch) -> Result<()> {
        let name: &str = branch;
        let local = name.strip_prefix("origin/").unwrap_or(name);
        let refspec = format!("refs/heads/{local}:refs/heads/{local}");

        let mut remote = self
            .inner
            .find_remote("origin")
            .map_err(|e| GitError::PushFailed(format!("no remote 'origin': {e}")))?;

        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.credentials(|url, username_from_url, allowed_types| {
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                git2::Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"))
            } else if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
                git2::Cred::credential_helper(
                    &git2::Config::open_default()?,
                    url,
                    username_from_url,
                )
            } else if allowed_types.contains(git2::CredentialType::DEFAULT) {
                git2::Cred::default()
            } else {
                Err(git2::Error::from_str("no available credentials"))
            }
        });

        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        remote
            .push(&[&refspec], Some(&mut push_opts))
            .map_err(|e| GitError::PushFailed(format!("push origin {local} failed: {e}")))?;

        Ok(())
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Ensure a worktree exists for `branch`. No-op if it's already
    /// checked out in the main workdir or any linked worktree.
    fn ensure_worktree(&self, branch: &Branch) -> Result<()> {
        if self.is_branch_checked_out(branch)? {
            return Ok(());
        }

        let branch_str: &str = branch;
        let wt_path = self.worktrees_path().join(branch_str);
        self.add_worktree(&wt_path, branch, branch)?;

        Ok(())
    }

    /// Check if a branch is checked out in the main workdir or any linked worktree.
    fn is_branch_checked_out(&self, branch: &Branch) -> Result<bool> {
        let clone = self.open_clone()?;
        let name: &str = branch;
        let local = name.strip_prefix("origin/").unwrap_or(name);

        if let Ok(b) = clone.current_branch() {
            if &*b == local {
                return Ok(true);
            }
        }

        Ok(clone
            .list_worktrees()?
            .iter()
            .any(|wt| wt.branch().map_or(false, |b| &*b == local)))
    }

    /// Low-level: create a git2 worktree at `path` for `branch`, forking
    /// from `base` if the branch doesn't exist yet. Creates parent dirs.
    fn add_worktree(&self, path: &Path, branch: &Branch, base: &Branch) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let branch_str: &str = branch;
        let local = branch_str.strip_prefix("origin/").unwrap_or(branch_str);

        if self.branch_exists(branch)? {
            let branch_ref = self.inner.find_branch(local, git2::BranchType::Local)?;
            let reference = branch_ref.into_reference();
            let mut opts = git2::WorktreeAddOptions::new();
            opts.reference(Some(&reference));
            self.inner.worktree(local, path, Some(&opts))?;
        } else {
            let base_str: &str = base;
            let start_commit = self.find_valid_start_point(base_str)?;
            let new_branch = self.inner.branch(local, &start_commit, false)?;
            let reference = new_branch.into_reference();
            let mut opts = git2::WorktreeAddOptions::new();
            opts.reference(Some(&reference));
            self.inner.worktree(local, path, Some(&opts))?;

            let wt_repo = Self::open(path)?;
            if let Ok(mut config) = wt_repo.inner.config() {
                let _ = config.set_bool("push.autoSetupRemote", true);
            }
        }

        let base_str: &str = base;
        let base_local = base_str.strip_prefix("origin/").unwrap_or(base_str);
        let wt_repo = Self::open(path)?;
        if let Ok(mut local_branch) = wt_repo.inner.find_branch(local, git2::BranchType::Local) {
            let _ = local_branch.set_upstream(Some(base_local));
        }

        Ok(())
    }

    fn for_each_worktree(
        &self,
        mut f: impl FnMut(&str, git2::Worktree) -> Result<()>,
    ) -> Result<()> {
        let wt_names = self.inner.worktrees()?;
        for i in 0..wt_names.len() {
            let Some(name) = wt_names.get(i) else {
                continue;
            };
            let Ok(wt) = self.inner.find_worktree(name) else {
                continue;
            };
            f(name, wt)?;
        }
        Ok(())
    }

    fn prune_worktree_named(&self, name: &str, force: bool) -> Result<()> {
        let wt = self.inner.find_worktree(name)?;
        let mut opts = git2::WorktreePruneOptions::new();
        opts.valid(true);
        opts.working_tree(true);
        if force {
            opts.locked(true);
        }
        wt.prune(Some(&mut opts))?;
        Ok(())
    }

    fn resolve_to_commit(&self, spec: &str) -> Result<git2::Commit<'_>> {
        let obj = self
            .inner
            .revparse_single(spec)
            .map_err(|_| GitError::BranchNotFound(spec.to_string()))?;
        Ok(obj
            .peel(git2::ObjectType::Commit)?
            .into_commit()
            .map_err(|_| git2::Error::from_str("not a commit"))?)
    }

    fn find_valid_start_point(&self, base_branch: &str) -> Result<git2::Commit<'_>> {
        if let Ok(commit) = self.resolve_to_commit(base_branch) {
            return Ok(commit);
        }

        if !base_branch.starts_with("origin/") {
            if let Ok(commit) = self.resolve_to_commit(&format!("origin/{}", base_branch)) {
                return Ok(commit);
            }
        }

        self.resolve_to_commit("HEAD")
            .map_err(|_| GitError::BranchNotFound(base_branch.to_string()))
    }
}
