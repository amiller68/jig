//! Worker discovery — scanning worktree directories for workers.

use std::collections::HashSet;

use crate::config::JIG_DIR;
use crate::registry::RepoRegistry;

/// Discover all workers by scanning worktree directories in registered repos.
///
/// The source of truth is the `.jig/` directory in each repo — each subdirectory
/// (recursively one level for `feature/foo` style names) is a worker.
pub(crate) fn discover_workers(registry: &RepoRegistry) -> Vec<(String, String)> {
    let mut workers = vec![];
    let mut seen = HashSet::new();

    for entry in registry.repos() {
        let repo_name = match entry.path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };

        let jig_dir = entry.path.join(JIG_DIR);
        if !jig_dir.is_dir() {
            continue;
        }

        let entries = match std::fs::read_dir(&jig_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for child in entries.flatten() {
            let name = child.file_name().to_string_lossy().to_string();

            // Skip non-directories and hidden dirs
            if !child.path().is_dir() || name.starts_with('.') {
                continue;
            }

            // Check if this is a nested worker (e.g., feature/foo)
            if has_git_worktree_marker(&child.path()) {
                let key = format!("{}/{}", repo_name, name);
                if seen.insert(key) {
                    workers.push((repo_name.clone(), name));
                }
            } else {
                // Scan one level deeper for grouped workers like feature/foo
                if let Ok(sub_entries) = std::fs::read_dir(child.path()) {
                    for sub in sub_entries.flatten() {
                        let sub_name = sub.file_name().to_string_lossy().to_string();
                        if !sub.path().is_dir() || sub_name.starts_with('.') {
                            continue;
                        }
                        let worker_name = format!("{}/{}", name, sub_name);
                        let key = format!("{}/{}", repo_name, worker_name);
                        if seen.insert(key) {
                            workers.push((repo_name.clone(), worker_name));
                        }
                    }
                }
            }
        }
    }

    workers
}

/// Check if a directory looks like a git worktree (has .git file or directory).
fn has_git_worktree_marker(path: &std::path::Path) -> bool {
    path.join(".git").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_git_marker_detects_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(".git"), "gitdir: /somewhere").unwrap();
        assert!(has_git_worktree_marker(tmp.path()));
    }

    #[test]
    fn has_git_marker_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!has_git_worktree_marker(tmp.path()));
    }
}
