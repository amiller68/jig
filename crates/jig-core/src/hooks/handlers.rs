//! Git hook handler implementations.
//!
//! Called by `jig hooks <name>` when git hook wrappers fire.
//! Each handler emits events to the worker's event log.

use std::path::Path;
use std::process::Command;

use crate::error::Result;
use crate::events::{Event, EventLog, EventType};

/// Handle post-commit hook: emit a Commit event with the HEAD SHA.
///
/// Silently does nothing if not in a jig-managed worktree or if
/// the worker can't be identified.
pub fn handle_post_commit(repo_path: &Path) -> Result<()> {
    let Some((repo_name, worker_name)) = identify_worker(repo_path) else {
        return Ok(());
    };

    let sha = head_sha(repo_path).unwrap_or_default();

    let event = Event::new(EventType::Commit)
        .with_field("sha", sha)
        .with_field("repo", repo_name.clone())
        .with_field("worker", worker_name.clone());

    let log = EventLog::for_worker(&repo_name, &worker_name)?;
    log.append(&event)?;

    Ok(())
}

/// Handle post-merge hook: emit a Push event.
pub fn handle_post_merge(repo_path: &Path) -> Result<()> {
    let Some((repo_name, worker_name)) = identify_worker(repo_path) else {
        return Ok(());
    };

    let sha = head_sha(repo_path).unwrap_or_default();

    let event = Event::new(EventType::Push)
        .with_field("sha", sha)
        .with_field("repo", repo_name.clone())
        .with_field("worker", worker_name.clone());

    let log = EventLog::for_worker(&repo_name, &worker_name)?;
    log.append(&event)?;

    Ok(())
}

/// Handle pre-commit hook: currently a no-op.
///
/// Future: validate conventional commits if configured.
pub fn handle_pre_commit(_repo_path: &Path) -> Result<()> {
    Ok(())
}

/// Try to identify the repo name and worker name from the repo path.
///
/// Returns `None` if not in a jig-managed worktree.
fn identify_worker(repo_path: &Path) -> Option<(String, String)> {
    // Check if we're inside a .jig/ worktree directory
    let path_str = repo_path.to_string_lossy();

    // Look for .jig/ in the path — the parent of .jig is the repo root,
    // and everything after .jig/ is the worker name
    if let Some(idx) = path_str.find("/.jig/") {
        let repo_root = &path_str[..idx];
        let worker_name = &path_str[idx + 6..]; // skip "/.jig/"

        let repo_name = Path::new(repo_root)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        if !worker_name.is_empty() {
            return Some((repo_name, worker_name.to_string()));
        }
    }

    None
}

/// Get HEAD SHA for the repo at the given path.
fn head_sha(repo_path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identify_worker_in_jig_worktree() {
        let path = Path::new("/home/user/myrepo/.jig/feat/add-auth");
        let result = identify_worker(path);
        assert!(result.is_some());
        let (repo, worker) = result.unwrap();
        assert_eq!(repo, "myrepo");
        assert_eq!(worker, "feat/add-auth");
    }

    #[test]
    fn identify_worker_not_in_worktree() {
        let path = Path::new("/home/user/myrepo");
        assert!(identify_worker(path).is_none());
    }

    #[test]
    fn identify_worker_simple_name() {
        let path = Path::new("/repos/project/.jig/fix-bug");
        let result = identify_worker(path);
        assert!(result.is_some());
        let (repo, worker) = result.unwrap();
        assert_eq!(repo, "project");
        assert_eq!(worker, "fix-bug");
    }

    #[test]
    fn pre_commit_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(handle_pre_commit(tmp.path()).is_ok());
    }

    #[test]
    fn post_commit_outside_worktree_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(handle_post_commit(tmp.path()).is_ok());
    }

    #[test]
    fn post_merge_outside_worktree_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(handle_post_merge(tmp.path()).is_ok());
    }
}
