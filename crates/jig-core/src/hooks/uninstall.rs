//! Git hook uninstall and rollback logic.

use std::path::Path;

use crate::error::Result;

use super::registry::{self, HookRegistry};

/// Outcome for a single hook uninstall.
#[derive(Debug)]
pub enum UninstallOutcome {
    /// Hook removed, no previous hook to restore.
    Removed(String),
    /// Restored from backup file.
    RestoredBackup { hook: String, backup: String },
    /// Restored from `.user` suffix.
    RestoredUser(String),
}

/// Aggregate result of `uninstall_hooks`.
#[derive(Debug)]
pub struct UninstallResult {
    pub outcomes: Vec<UninstallOutcome>,
}

/// Uninstall jig git hooks from the repo.
///
/// If `specific_hook` is `Some`, only that hook is removed.
/// Otherwise all hooks tracked in the registry are removed.
pub fn uninstall_hooks(repo_path: &Path, specific_hook: Option<&str>) -> Result<UninstallResult> {
    let hooks_dir = repo_path.join(".git").join("hooks");
    let registry_path = registry::registry_path(repo_path);

    let mut registry = HookRegistry::load(repo_path)?;
    let mut outcomes = Vec::new();

    let hooks_to_remove: Vec<String> = if let Some(hook) = specific_hook {
        vec![hook.to_string()]
    } else {
        registry.installed.keys().cloned().collect()
    };

    for hook_name in hooks_to_remove {
        if let Some(entry) = registry.remove(&hook_name) {
            let hook_path = hooks_dir.join(&hook_name);

            // Remove jig wrapper
            if hook_path.exists() {
                std::fs::remove_file(&hook_path)?;
            }

            // Try restoring from .user first (it's the renamed original)
            let user_path = hooks_dir.join(format!("{}.user", hook_name));
            if user_path.exists() {
                std::fs::rename(&user_path, &hook_path)?;
                outcomes.push(UninstallOutcome::RestoredUser(hook_name.clone()));

                // Clean up backup if it exists
                if let Some(backup_name) = &entry.backed_up_to {
                    let backup_path = hooks_dir.join(backup_name);
                    if backup_path.exists() {
                        let _ = std::fs::remove_file(&backup_path);
                    }
                }
                continue;
            }

            // Try restoring from backup
            if let Some(backup_name) = &entry.backed_up_to {
                let backup_path = hooks_dir.join(backup_name);
                if backup_path.exists() {
                    std::fs::copy(&backup_path, &hook_path)?;
                    let _ = std::fs::remove_file(&backup_path);
                    outcomes.push(UninstallOutcome::RestoredBackup {
                        hook: hook_name,
                        backup: backup_name.clone(),
                    });
                    continue;
                }
            }

            outcomes.push(UninstallOutcome::Removed(hook_name));
        }
    }

    // If all hooks removed, delete the registry file
    if specific_hook.is_none() && registry.installed.is_empty() {
        if registry_path.exists() {
            std::fs::remove_file(&registry_path)?;
        }
    } else {
        registry.save(repo_path)?;
    }

    Ok(UninstallResult { outcomes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::git::MANAGED_HOOKS;
    use crate::hooks::install::init_hooks;

    fn fake_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().to_path_buf();
        std::fs::create_dir_all(repo.join(".git/hooks")).unwrap();
        (tmp, repo)
    }

    #[test]
    fn uninstall_removes_all_hooks() {
        let (_tmp, repo) = fake_repo();
        init_hooks(&repo, false).unwrap();

        let result = uninstall_hooks(&repo, None).unwrap();
        assert_eq!(result.outcomes.len(), MANAGED_HOOKS.len());

        // Hooks should be gone
        for name in MANAGED_HOOKS {
            assert!(!repo.join(".git/hooks").join(name).exists());
        }

        // Registry should be gone
        assert!(!registry::registry_path(&repo).exists());
    }

    #[test]
    fn uninstall_restores_user_hooks() {
        let (_tmp, repo) = fake_repo();
        let hook_path = repo.join(".git/hooks/post-commit");
        std::fs::write(&hook_path, "#!/bin/bash\necho 'original'").unwrap();

        init_hooks(&repo, false).unwrap();

        // Verify user hook was moved
        assert!(repo.join(".git/hooks/post-commit.user").exists());

        uninstall_hooks(&repo, None).unwrap();

        // Original hook should be restored
        let content = std::fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("echo 'original'"));

        // .user file should be cleaned up
        assert!(!repo.join(".git/hooks/post-commit.user").exists());
    }

    #[test]
    fn uninstall_specific_hook() {
        let (_tmp, repo) = fake_repo();
        init_hooks(&repo, false).unwrap();

        uninstall_hooks(&repo, Some("post-commit")).unwrap();

        // post-commit gone, others remain
        assert!(!repo.join(".git/hooks/post-commit").exists());
        assert!(repo.join(".git/hooks/post-merge").exists());
        assert!(repo.join(".git/hooks/pre-commit").exists());

        // Registry still exists with remaining hooks
        let registry = HookRegistry::load(&repo).unwrap();
        assert!(!registry.is_installed("post-commit"));
        assert!(registry.is_installed("post-merge"));
    }

    #[test]
    fn uninstall_is_idempotent() {
        let (_tmp, repo) = fake_repo();
        init_hooks(&repo, false).unwrap();

        uninstall_hooks(&repo, None).unwrap();
        // Second uninstall should be a no-op
        let result = uninstall_hooks(&repo, None).unwrap();
        assert!(result.outcomes.is_empty());
    }

    #[test]
    fn uninstall_handles_missing_hooks() {
        let (_tmp, repo) = fake_repo();
        init_hooks(&repo, false).unwrap();

        // Manually delete a hook file
        std::fs::remove_file(repo.join(".git/hooks/post-commit")).unwrap();

        // Uninstall should still work
        let result = uninstall_hooks(&repo, None).unwrap();
        assert_eq!(result.outcomes.len(), MANAGED_HOOKS.len());
    }
}
