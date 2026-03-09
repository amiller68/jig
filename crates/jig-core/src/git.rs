//! Git operations
//!
//! Git operations using the git2 (libgit2) library.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::JIG_DIR;
use crate::error::{Error, Result};
use crate::worker::DiffStats;

/// Open a repository by discovering from the current directory.
fn discover_repo() -> Result<git2::Repository> {
    git2::Repository::discover(".").map_err(|_| Error::NotInGitRepo)
}

/// Open a repository at a specific path.
fn open_repo(path: &Path) -> Result<git2::Repository> {
    git2::Repository::open(path).map_err(|e| Error::Git(e.message().to_string()))
}

/// Resolve a revspec string to a commit.
fn resolve_to_commit<'repo>(
    repo: &'repo git2::Repository,
    spec: &str,
) -> Result<git2::Commit<'repo>> {
    let obj = repo
        .revparse_single(spec)
        .map_err(|e| Error::BranchNotFound(format!("{}: {}", spec, e.message())))?;
    obj.peel(git2::ObjectType::Commit)?
        .into_commit()
        .map_err(|_| Error::Git(format!("'{}' is not a commit", spec)))
}

/// Build a diff between a base branch and HEAD for a repo at `path`.
fn make_diff<'repo>(repo: &'repo git2::Repository, base_branch: &str) -> Result<git2::Diff<'repo>> {
    let base_commit = resolve_to_commit(repo, base_branch)?;
    let base_tree = base_commit.tree()?;

    let head = repo.head()?;
    let head_commit = head
        .peel(git2::ObjectType::Commit)?
        .into_commit()
        .map_err(|_| Error::Git("HEAD is not a commit".to_string()))?;
    let head_tree = head_commit.tree()?;

    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;
    Ok(diff)
}

/// Find a valid start-point commit for creating a new branch.
fn find_valid_start_point<'repo>(
    repo: &'repo git2::Repository,
    base_branch: &str,
) -> Result<git2::Commit<'repo>> {
    // revparse_single handles refs/heads/*, refs/remotes/*, SHAs, etc.
    if let Ok(obj) = repo.revparse_single(base_branch) {
        if let Ok(commit) = obj.peel(git2::ObjectType::Commit).and_then(|o| {
            o.into_commit()
                .map_err(|_| git2::Error::from_str("not a commit"))
        }) {
            return Ok(commit);
        }
    }

    // Try with origin/ prefix if not already prefixed
    if !base_branch.starts_with("origin/") {
        if let Ok(obj) = repo.revparse_single(&format!("origin/{}", base_branch)) {
            if let Ok(commit) = obj.peel(git2::ObjectType::Commit).and_then(|o| {
                o.into_commit()
                    .map_err(|_| git2::Error::from_str("not a commit"))
            }) {
                return Ok(commit);
            }
        }
    }

    // Fall back to HEAD
    let head = repo
        .head()
        .map_err(|_| Error::BranchNotFound(base_branch.to_string()))?;
    head.peel(git2::ObjectType::Commit)?
        .into_commit()
        .map_err(|_| Error::BranchNotFound(base_branch.to_string()))
}

/// Check if a branch exists (local or remote).
fn branch_exists_impl(repo: &git2::Repository, branch: &str) -> Result<bool> {
    let branch = branch.strip_prefix("origin/").unwrap_or(branch);

    // Check local branch
    if repo.find_branch(branch, git2::BranchType::Local).is_ok() {
        return Ok(true);
    }

    // Check remote branch
    if repo
        .find_branch(&format!("origin/{}", branch), git2::BranchType::Remote)
        .is_ok()
    {
        return Ok(true);
    }

    Ok(false)
}

/// Prune stale (invalid) worktree registrations.
fn prune_stale_worktrees(repo: &git2::Repository) {
    if let Ok(wt_names) = repo.worktrees() {
        for i in 0..wt_names.len() {
            if let Some(name) = wt_names.get(i) {
                if let Ok(wt) = repo.find_worktree(name) {
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
fn find_worktree_name_for_path(repo: &git2::Repository, path: &Path) -> Result<String> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let wt_names = repo.worktrees()?;
    for i in 0..wt_names.len() {
        if let Some(name) = wt_names.get(i) {
            if let Ok(wt) = repo.find_worktree(name) {
                let wt_path = wt.path().to_path_buf();
                let wt_canonical = wt_path.canonicalize().unwrap_or(wt_path);
                if wt_canonical == canonical {
                    return Ok(name.to_string());
                }
            }
        }
    }

    Err(Error::Git(format!(
        "no worktree found for path: {}",
        path.display()
    )))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Get the root directory of the git repository
pub fn get_repo_root() -> Result<PathBuf> {
    let repo = discover_repo()?;
    repo.workdir()
        .map(|p| p.to_path_buf())
        .ok_or(Error::NotInGitRepo)
}

/// Get the common git directory (handles worktrees correctly)
pub fn get_git_common_dir() -> Result<PathBuf> {
    let repo = discover_repo()?;
    Ok(repo.commondir().to_path_buf())
}

/// Get the common git directory for a specific repo path.
pub fn get_git_common_dir_for(repo_root: &Path) -> Result<PathBuf> {
    let repo = open_repo(repo_root)?;
    Ok(repo.commondir().to_path_buf())
}

/// Get the base repository directory (even when in a worktree)
pub fn get_base_repo() -> Result<PathBuf> {
    let git_common = get_git_common_dir()?;
    // The common dir is .git in the base repo
    Ok(git_common.parent().unwrap_or(&git_common).to_path_buf())
}

/// Check if we're inside a worktree (not the main repo)
pub fn is_in_worktree(worktrees_dir: &Path) -> Result<bool> {
    let cwd = std::env::current_dir()?;
    Ok(cwd.starts_with(worktrees_dir))
}

/// Get current worktree name (if in one)
pub fn get_current_worktree_name(worktrees_dir: &Path) -> Result<Option<String>> {
    let cwd = std::env::current_dir()?;

    if !cwd.starts_with(worktrees_dir) {
        return Ok(None);
    }

    // Find the worktree root (directory containing .git file)
    let mut current = cwd.as_path();
    while current.starts_with(worktrees_dir) && current != worktrees_dir {
        if current.join(".git").is_file() {
            // Found the worktree root
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

/// List all worktree names in a directory
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
            // Skip hidden directories (e.g. .state)
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }

            // Check if this is a worktree (has .git file)
            if path.join(".git").is_file() {
                let name = path
                    .strip_prefix(base)
                    .map_err(|_| Error::InvalidPath(path.clone()))?
                    .to_string_lossy()
                    .to_string();
                worktrees.push(name);
            } else {
                // Recurse into subdirectories
                find_worktrees_recursive(base, &path, worktrees)?;
            }
        }
    }

    Ok(())
}

/// List all git worktrees (including base repo)
pub fn list_all_worktrees() -> Result<Vec<(PathBuf, String)>> {
    let repo = discover_repo()?;
    let mut worktrees = Vec::new();

    // Add main worktree
    if let Some(workdir) = repo.workdir() {
        let branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()))
            .unwrap_or_default();
        worktrees.push((workdir.to_path_buf(), branch));
    }

    // Add linked worktrees
    if let Ok(wt_names) = repo.worktrees() {
        for i in 0..wt_names.len() {
            if let Some(name) = wt_names.get(i) {
                if let Ok(wt) = repo.find_worktree(name) {
                    let wt_path = wt.path().to_path_buf();
                    // Open repo at worktree path to get its branch
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

/// Check if a branch exists
pub fn branch_exists(branch: &str) -> Result<bool> {
    let repo = discover_repo()?;
    branch_exists_impl(&repo, branch)
}

/// Get the current branch name
pub fn get_current_branch() -> Result<String> {
    let repo = discover_repo()?;
    let head = repo
        .head()
        .map_err(|_| Error::Git("Failed to get current branch".to_string()))?;
    head.shorthand()
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Git("HEAD is not a symbolic reference".to_string()))
}

/// Create a git worktree
pub fn create_worktree(path: &Path, branch: &str, base_branch: &str) -> Result<()> {
    let repo = discover_repo()?;

    // Clean up stale worktree registrations
    prune_stale_worktrees(&repo);

    let exists = branch_exists_impl(&repo, branch)?;

    // Worktree name: use last path component (matches git default behavior)
    let wt_name = path
        .file_name()
        .ok_or_else(|| Error::InvalidPath(path.to_path_buf()))?
        .to_string_lossy()
        .to_string();

    if exists {
        let branch_ref = repo
            .find_branch(branch, git2::BranchType::Local)
            .map_err(|e| Error::Git(e.message().to_string()))?;
        let reference = branch_ref.into_reference();
        let mut opts = git2::WorktreeAddOptions::new();
        opts.reference(Some(&reference));
        repo.worktree(&wt_name, path, Some(&opts))?;
    } else {
        // Create new branch from base
        let start_commit = find_valid_start_point(&repo, base_branch)?;
        let new_branch = repo.branch(branch, &start_commit, false)?;
        let reference = new_branch.into_reference();
        let mut opts = git2::WorktreeAddOptions::new();
        opts.reference(Some(&reference));
        repo.worktree(&wt_name, path, Some(&opts))?;

        // Set up push tracking for new branches
        let wt_repo = open_repo(path)?;
        if let Ok(mut config) = wt_repo.config() {
            let _ = config.set_bool("push.autoSetupRemote", true);
        }
    }

    Ok(())
}

/// Remove a git worktree
pub fn remove_worktree(path: &Path, force: bool) -> Result<()> {
    // If not forcing, check for uncommitted changes first
    if !force && path.exists() && has_uncommitted_changes(path)? {
        return Err(Error::UncommittedChanges);
    }

    // We need the main repo to manipulate worktrees.
    // Open the worktree repo to find the common dir, then open the main repo.
    let repo = if path.exists() {
        let wt_repo = open_repo(path)?;
        let commondir = wt_repo.commondir().to_path_buf();
        let main_workdir = commondir.parent().unwrap_or(&commondir);
        open_repo(main_workdir)?
    } else {
        discover_repo()?
    };

    let wt_name = find_worktree_name_for_path(&repo, path)?;
    let wt = repo
        .find_worktree(&wt_name)
        .map_err(|e| Error::Git(e.message().to_string()))?;

    let mut opts = git2::WorktreePruneOptions::new();
    opts.valid(true); // prune even if valid
    opts.working_tree(true); // remove the working directory
    if force {
        opts.locked(true);
    }

    wt.prune(Some(&mut opts))
        .map_err(|e| Error::Git(e.message().to_string()))?;

    Ok(())
}

/// Check if worktree has uncommitted changes
pub fn has_uncommitted_changes(path: &Path) -> Result<bool> {
    let repo = open_repo(path)?;
    let statuses = repo.statuses(Some(
        git2::StatusOptions::new()
            .include_untracked(true)
            .recurse_untracked_dirs(true),
    ))?;
    Ok(!statuses.is_empty())
}

/// Get commits ahead of base branch
pub fn get_commits_ahead(path: &Path, base_branch: &str) -> Result<Vec<String>> {
    let repo = open_repo(path)?;

    let base_oid = match repo.revparse_single(base_branch) {
        Ok(obj) => match obj.peel(git2::ObjectType::Commit) {
            Ok(c) => c.id(),
            Err(_) => return Ok(Vec::new()),
        },
        Err(_) => return Ok(Vec::new()),
    };

    let head_oid = match repo.head() {
        Ok(h) => match h.target() {
            Some(oid) => oid,
            None => return Ok(Vec::new()),
        },
        Err(_) => return Ok(Vec::new()),
    };

    let mut revwalk = repo.revwalk()?;
    revwalk.push(head_oid)?;
    revwalk.hide(base_oid)?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL)?;

    let mut commits = Vec::new();
    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        let short_id = &oid.to_string()[..7];
        let summary = commit.summary().unwrap_or("");
        commits.push(format!("{} {}", short_id, summary));
    }

    Ok(commits)
}

/// Get diff stat for a worktree (formatted string like `git diff --stat`)
pub fn get_diff_stat(path: &Path, base_branch: &str) -> Result<String> {
    let repo = open_repo(path)?;
    let diff = make_diff(&repo, base_branch)?;
    let stats = diff.stats()?;
    let buf = stats.to_buf(git2::DiffStatsFormat::FULL, 80)?;
    Ok(std::str::from_utf8(&buf).unwrap_or("").to_string())
}

/// Get diff stats as structured data
pub fn get_diff_stats(path: &Path, base_branch: &str) -> Result<DiffStats> {
    let repo = open_repo(path)?;
    let diff = make_diff(&repo, base_branch)?;

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

/// Get full diff for a worktree
pub fn get_diff(path: &Path, base_branch: &str) -> Result<String> {
    let repo = open_repo(path)?;
    let diff = make_diff(&repo, base_branch)?;

    let mut output = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        match origin {
            '+' | '-' | ' ' => output.push(origin as u8),
            _ => {}
        }
        output.extend_from_slice(line.content());
        true
    })?;
    Ok(String::from_utf8_lossy(&output).to_string())
}

/// Merge a branch into the current branch
pub fn merge_branch(branch: &str) -> Result<()> {
    let repo = discover_repo()?;

    // Find the branch reference
    let branch_ref = repo
        .find_branch(branch, git2::BranchType::Local)
        .or_else(|_| repo.find_branch(&format!("origin/{}", branch), git2::BranchType::Remote))
        .map_err(|_| Error::BranchNotFound(branch.to_string()))?;

    let annotated = repo.reference_to_annotated_commit(&branch_ref.into_reference())?;
    let (analysis, _) = repo.merge_analysis(&[&annotated])?;

    if analysis.is_up_to_date() {
        return Ok(());
    }

    if analysis.is_fast_forward() {
        let target_oid = annotated.id();
        let mut reference = repo.head()?;
        reference.set_target(target_oid, &format!("merge {}: Fast-forward", branch))?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        return Ok(());
    }

    if analysis.is_normal() {
        repo.merge(&[&annotated], None, None)?;

        let mut index = repo.index()?;
        if index.has_conflicts() {
            repo.cleanup_state()?;
            return Err(Error::Git(format!(
                "Merge conflict with branch '{}'",
                branch
            )));
        }

        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;
        let head_commit = repo
            .head()?
            .peel(git2::ObjectType::Commit)?
            .into_commit()
            .map_err(|_| Error::Git("HEAD is not a commit".to_string()))?;
        let merge_commit = repo.find_commit(annotated.id())?;
        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("jig", "jig@localhost"))?;

        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("Merge branch '{}'", branch),
            &tree,
            &[&head_commit, &merge_commit],
        )?;
        repo.cleanup_state()?;
        return Ok(());
    }

    Err(Error::Git(format!("Cannot merge branch '{}'", branch)))
}

/// Get worktree branch
pub fn get_worktree_branch(path: &Path) -> Result<String> {
    let repo = open_repo(path)?;
    let head = repo
        .head()
        .map_err(|_| Error::Git("Failed to get worktree branch".to_string()))?;
    head.shorthand()
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Git("HEAD is not a symbolic reference".to_string()))
}

/// Ensure jig directory is in git exclude
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
