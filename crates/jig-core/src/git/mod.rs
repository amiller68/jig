//! Git operations — opinionated module built on top of git2.
//!
//! - [`Repo`] wraps `git2::Repository` with jig-specific operations.
//! - [`Worktree`] / [`WorktreeRef`] represent resolved and serializable
//!   worktree handles respectively.

mod branch;
mod commit;
mod diff;
mod error;
mod repo;
mod worktree;

pub use branch::Branch;
pub use commit::conventional;
pub use commit::Oid;
pub use diff::{Diff, FileDiff, Stats as DiffStats};
pub use error::GitError;
pub use repo::Repo;
pub use worktree::{Worktree, WorktreeRef};

pub const WORKTREES_DIR: &str = ".jig";

use std::io::Write;
use std::path::Path;

use error::Result;

// ---------------------------------------------------------------------------
// Free functions (filesystem-only, no git2)
// ---------------------------------------------------------------------------

/// Ensure `dir_name` is listed in the repository's local exclude file.
pub fn ensure_excluded(git_common_dir: &Path, dir_name: &str) -> Result<()> {
    let exclude_file = git_common_dir.join("info").join("exclude");
    let exclude_entry = format!("{}/", dir_name);

    if !exclude_file.exists() {
        std::fs::create_dir_all(exclude_file.parent().unwrap())?;
        std::fs::write(&exclude_file, format!("{}\n", exclude_entry))?;
        return Ok(());
    }

    let content = std::fs::read_to_string(&exclude_file)?;
    if !content.contains(dir_name) {
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
    use tempfile::TempDir;

    fn sig() -> git2::Signature<'static> {
        git2::Signature::now("Test", "test@test.com").unwrap()
    }

    fn init_repo(dir: &Path) -> git2::Repository {
        let repo = git2::Repository::init(dir).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.email", "test@test.com").unwrap();
            config.set_str("user.name", "Test").unwrap();
            config.set_bool("commit.gpgsign", false).unwrap();
        }
        {
            let mut index = repo.index().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = sig();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
        }
        repo.head()
            .unwrap()
            .rename("refs/heads/main", true, "init main")
            .unwrap();
        repo
    }

    fn empty_commit(repo: &git2::Repository, msg: &str) -> git2::Oid {
        let sig = sig();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = head.tree().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&head])
            .unwrap()
    }

    fn add_self_remote(repo: &git2::Repository) {
        let path = repo.workdir().unwrap().to_str().unwrap();
        repo.remote("origin", path).unwrap();
        fetch(repo);
    }

    fn fetch(repo: &git2::Repository) {
        repo.find_remote("origin")
            .unwrap()
            .fetch(&[] as &[&str], None, None)
            .unwrap();
    }

    fn create_bare_upstream(dir: &Path) -> TempDir {
        let upstream_dir = TempDir::new().unwrap();
        let bare_path = upstream_dir.path().join("bare");
        let src = git2::Repository::open(dir).unwrap();
        let src_path = src.workdir().unwrap().to_str().unwrap();

        // Clone to bare
        git2::build::RepoBuilder::new()
            .bare(true)
            .clone(src_path, &bare_path)
            .unwrap();

        // Point source repo's origin at the bare
        src.remote_delete("origin").ok();
        src.remote("origin", bare_path.to_str().unwrap()).unwrap();
        drop(src);

        let repo = git2::Repository::open(dir).unwrap();
        fetch(&repo);

        upstream_dir
    }

    #[test]
    fn fast_forward_branch_up_to_date() {
        let dir = TempDir::new().unwrap();
        let git = init_repo(dir.path());
        add_self_remote(&git);

        let head = git.head().unwrap().peel_to_commit().unwrap();
        git.branch("parent-branch", &head, false).unwrap();
        fetch(&git);

        let repo = Repo::open(dir.path()).unwrap();
        let result = repo
            .fast_forward_branch(&"parent-branch".into(), false)
            .unwrap();
        assert!(!result, "should be up to date");
    }

    #[test]
    fn fast_forward_branch_advances_ref() {
        let dir = TempDir::new().unwrap();
        let git = init_repo(dir.path());

        let head = git.head().unwrap().peel_to_commit().unwrap();
        git.branch("parent-branch", &head, false).unwrap();
        let parent_oid = head.id();

        empty_commit(&git, "child-work");
        add_self_remote(&git);

        let head_oid = git.head().unwrap().target().unwrap();

        // Advance remote's view of parent-branch to HEAD
        git.reference(
            "refs/heads/parent-branch",
            head_oid,
            true,
            "force parent-branch to HEAD",
        )
        .unwrap();
        fetch(&git);

        // Reset local parent-branch back behind remote
        git.reference(
            "refs/heads/parent-branch",
            parent_oid,
            true,
            "reset parent-branch behind remote",
        )
        .unwrap();

        let repo = Repo::open(dir.path()).unwrap();
        let result = repo
            .fast_forward_branch(&"parent-branch".into(), false)
            .unwrap();
        assert!(result, "should have advanced");

        let local_ref = repo
            .inner()
            .find_reference("refs/heads/parent-branch")
            .unwrap();
        let remote_ref = repo
            .inner()
            .find_branch("origin/parent-branch", git2::BranchType::Remote)
            .unwrap();
        assert_eq!(local_ref.target(), remote_ref.get().target());
    }

    #[test]
    fn fast_forward_branch_creates_missing_local() {
        let dir = TempDir::new().unwrap();
        let git = init_repo(dir.path());
        add_self_remote(&git);

        let head = git.head().unwrap().peel_to_commit().unwrap();
        git.branch("feature", &head, false).unwrap();
        fetch(&git);

        // Delete the local branch, keep the remote tracking ref
        git.find_branch("feature", git2::BranchType::Local)
            .unwrap()
            .delete()
            .unwrap();

        let repo = Repo::open(dir.path()).unwrap();
        let result = repo.fast_forward_branch(&"feature".into(), false).unwrap();
        assert!(result, "should have created local branch");

        let local_ref = repo.inner().find_reference("refs/heads/feature").unwrap();
        let remote_ref = repo
            .inner()
            .find_branch("origin/feature", git2::BranchType::Remote)
            .unwrap();
        assert_eq!(local_ref.target(), remote_ref.get().target());
    }

    #[test]
    fn fast_forward_branch_diverged_errors() {
        let dir = TempDir::new().unwrap();
        let git = init_repo(dir.path());

        let head = git.head().unwrap().peel_to_commit().unwrap();
        git.branch("parent-branch", &head, false).unwrap();

        add_self_remote(&git);
        fetch(&git);

        // Create a local-only commit on parent-branch by checking it out
        git.set_head("refs/heads/parent-branch").unwrap();
        git.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        empty_commit(&git, "local-only");
        git.set_head("refs/heads/main").unwrap();
        git.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();

        let repo = Repo::open(dir.path()).unwrap();
        let result = repo.fast_forward_branch(&"parent-branch".into(), false);
        assert!(result.is_err(), "should error on diverged branches");
    }

    #[test]
    fn push_branch_works_with_bare_upstream() {
        let dir = TempDir::new().unwrap();
        let git = init_repo(dir.path());
        let _upstream_dir = create_bare_upstream(dir.path());

        // Create and checkout test-push branch
        let head = git.head().unwrap().peel_to_commit().unwrap();
        git.branch("test-push", &head, false).unwrap();
        git.set_head("refs/heads/test-push").unwrap();
        git.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        empty_commit(&git, "push-test");

        let repo = Repo::open(dir.path()).unwrap();
        repo.push_branch(&"test-push".into()).unwrap();

        // Fetch to update remote tracking refs, then verify
        fetch(&git);
        let remote_branch = git.find_branch("origin/test-push", git2::BranchType::Remote);
        assert!(
            remote_branch.is_ok(),
            "remote branch should exist after push"
        );
    }
}
