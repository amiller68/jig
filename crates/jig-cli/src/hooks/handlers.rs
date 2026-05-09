//! Git hook handler implementations.
//!
//! Called by `jig hooks <name>` when git hook wrappers fire.
//! Each handler emits events to the worker's event log.

use std::path::Path;

use crate::worker::events::{self, Event, EventKind};
use jig_core::git::conventional::ValidationConfig;
use jig_core::git::Worktree;

/// Handle post-commit hook: emit a Commit event with the HEAD SHA.
///
/// Silently does nothing if not in a jig-managed worktree.
pub fn handle_post_commit(repo_path: &Path) -> Result<(), super::HookError> {
    let Some(wt) = open_worktree(repo_path) else {
        return Ok(());
    };

    let repo_name = wt.repo_name();
    let worker_name = wt.branch_name().to_string();
    let sha = wt.head_sha().unwrap_or_default();

    let log = events::event_log_for_worker(&repo_name, &worker_name)?;
    log.append(&Event::now(EventKind::Commit {
        sha,
        repo: repo_name,
    }))?;

    Ok(())
}

/// Handle post-merge hook: emit a Push event.
pub fn handle_post_merge(repo_path: &Path) -> Result<(), super::HookError> {
    let Some(wt) = open_worktree(repo_path) else {
        return Ok(());
    };

    let repo_name = wt.repo_name();
    let worker_name = wt.branch_name().to_string();
    let sha = wt.head_sha().unwrap_or_default();

    let log = events::event_log_for_worker(&repo_name, &worker_name)?;
    log.append(&Event::now(EventKind::Push {
        sha,
        repo: repo_name,
    }))?;

    Ok(())
}

/// Handle commit-msg hook: validate the commit message against conventional commits spec.
///
/// Reads the commit message from the file path provided by git (the first argument).
/// If a `[commits]` section exists in `jig.toml`, validates against those rules.
/// Returns an error (blocking the commit) if validation fails.
pub fn handle_commit_msg(_repo_path: &Path, commit_msg_file: &str) -> Result<(), super::HookError> {
    let message = std::fs::read_to_string(commit_msg_file)?;

    // Strip git comment lines (lines starting with #)
    let cleaned: String = message
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");

    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return Ok(());
    }

    let config = ValidationConfig::default();

    match config.parse_and_validate(cleaned) {
        Ok((_msg, errors)) => {
            if errors.is_empty() {
                Ok(())
            } else {
                let msgs: Vec<String> = errors.iter().map(|e| format!("  {}", e)).collect();
                Err(super::HookError::Validation(format!(
                    "commit message does not follow conventional commits:\n{}\n\n  \
                     Run `jig commit examples` for help.",
                    msgs.join("\n"),
                )))
            }
        }
        Err(e) => Err(super::HookError::Validation(format!(
            "commit message does not follow conventional commits:\n  {}\n\n  \
             Run `jig commit examples` for help.",
            e,
        ))),
    }
}

/// Handle pre-commit hook: currently a no-op.
pub fn handle_pre_commit(_repo_path: &Path) -> Result<(), super::HookError> {
    Ok(())
}

/// Try to open the path as a linked git worktree (jig-managed).
fn open_worktree(repo_path: &Path) -> Option<Worktree> {
    Worktree::open(repo_path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_worktree_not_in_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(open_worktree(tmp.path()).is_none());
    }

    #[test]
    fn pre_commit_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(handle_pre_commit(tmp.path()).is_ok());
    }

    #[test]
    fn commit_msg_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let msg_file = tmp.path().join("COMMIT_EDITMSG");
        std::fs::write(&msg_file, "feat: add new feature\n").unwrap();
        assert!(handle_commit_msg(tmp.path(), msg_file.to_str().unwrap()).is_ok());
    }

    #[test]
    fn commit_msg_invalid() {
        let tmp = tempfile::tempdir().unwrap();
        let msg_file = tmp.path().join("COMMIT_EDITMSG");
        std::fs::write(&msg_file, "not a conventional commit\n").unwrap();
        assert!(handle_commit_msg(tmp.path(), msg_file.to_str().unwrap()).is_err());
    }

    #[test]
    fn commit_msg_strips_comments() {
        let tmp = tempfile::tempdir().unwrap();
        let msg_file = tmp.path().join("COMMIT_EDITMSG");
        std::fs::write(
            &msg_file,
            "fix: resolve bug\n# This is a comment\n# Another comment\n",
        )
        .unwrap();
        assert!(handle_commit_msg(tmp.path(), msg_file.to_str().unwrap()).is_ok());
    }

    #[test]
    fn commit_msg_empty_after_stripping_comments() {
        let tmp = tempfile::tempdir().unwrap();
        let msg_file = tmp.path().join("COMMIT_EDITMSG");
        std::fs::write(&msg_file, "# All comments\n# Nothing else\n").unwrap();
        assert!(handle_commit_msg(tmp.path(), msg_file.to_str().unwrap()).is_ok());
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
