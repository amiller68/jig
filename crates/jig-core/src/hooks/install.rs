//! Git hook installation logic.
//!
//! Implements idempotent `init_hooks()` that installs jig wrapper scripts
//! into `.git/hooks/`, backing up existing user hooks.

use std::path::Path;

use crate::error::Result;

use super::git::{generate_hook, is_jig_managed, MANAGED_HOOKS};
use super::registry::HookRegistry;

/// What to do for a given hook during init.
#[derive(Debug, PartialEq)]
pub enum InstallDecision {
    /// No existing hook — install fresh.
    Install,
    /// Already jig-managed and in registry — skip.
    Skip,
    /// Force flag — reinstall regardless.
    Reinstall,
    /// Jig-managed on disk but missing from registry — re-register.
    UpdateRegistry,
    /// User hook exists — back up and install.
    BackupAndInstall,
}

/// Outcome for a single hook.
#[derive(Debug)]
pub enum HookResult {
    Installed(String),
    AlreadyInstalled(String),
    BackedUpAndInstalled { hook: String, backup: String },
}

/// Aggregate result of `init_hooks`.
#[derive(Debug)]
pub struct InitResult {
    pub results: Vec<HookResult>,
}

/// Decide what to do for a single hook.
pub fn should_install_hook(
    hook_path: &Path,
    registry: &HookRegistry,
    hook_name: &str,
    force: bool,
) -> Result<InstallDecision> {
    if force {
        return Ok(InstallDecision::Reinstall);
    }
    if !hook_path.exists() {
        return Ok(InstallDecision::Install);
    }
    let content = std::fs::read_to_string(hook_path)?;
    if is_jig_managed(&content) {
        if registry.is_installed(hook_name) {
            return Ok(InstallDecision::Skip);
        }
        return Ok(InstallDecision::UpdateRegistry);
    }
    Ok(InstallDecision::BackupAndInstall)
}

/// Install jig git hooks into `<repo_path>/.git/hooks/`.
///
/// Registry is saved at `<repo_path>/.jig/hooks/hooks.json`.
pub fn init_hooks(repo_path: &Path, force: bool) -> Result<InitResult> {
    let hooks_dir = repo_path.join(".git").join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;

    let mut registry = HookRegistry::load(repo_path)?;
    let mut results = Vec::new();

    for hook_name in MANAGED_HOOKS {
        let hook_path = hooks_dir.join(hook_name);
        let decision = should_install_hook(&hook_path, &registry, hook_name, force)?;

        match decision {
            InstallDecision::Skip => {
                results.push(HookResult::AlreadyInstalled(hook_name.to_string()));
            }
            InstallDecision::BackupAndInstall => {
                let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
                let backup_name = format!("{}.backup-{}", hook_name, date);
                let backup_path = hooks_dir.join(&backup_name);
                std::fs::copy(&hook_path, &backup_path)?;

                let user_path = hooks_dir.join(format!("{}.user", hook_name));
                std::fs::rename(&hook_path, &user_path)?;

                // Write the jig wrapper
                let content = generate_hook(hook_name)?;
                std::fs::write(&hook_path, &content)?;
                make_executable(&hook_path)?;

                registry.mark_installed(hook_name);
                registry.mark_existing_backed_up(hook_name, &backup_name);

                results.push(HookResult::BackedUpAndInstalled {
                    hook: hook_name.to_string(),
                    backup: backup_name,
                });
            }
            InstallDecision::UpdateRegistry
            | InstallDecision::Install
            | InstallDecision::Reinstall => {
                let content = generate_hook(hook_name)?;
                std::fs::write(&hook_path, &content)?;
                make_executable(&hook_path)?;
                registry.mark_installed(hook_name);
                results.push(HookResult::Installed(hook_name.to_string()));
            }
        }
    }

    registry.save(repo_path)?;
    Ok(InitResult { results })
}

fn make_executable(_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(_path, perms)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a minimal fake repo directory with `.git/hooks/`.
    fn fake_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().to_path_buf();
        std::fs::create_dir_all(repo.join(".git/hooks")).unwrap();
        (tmp, repo)
    }

    #[test]
    fn init_fresh_repo() {
        let (_tmp, repo) = fake_repo();
        let result = init_hooks(&repo, false).unwrap();
        assert_eq!(result.results.len(), MANAGED_HOOKS.len());

        for name in MANAGED_HOOKS {
            let path = repo.join(".git/hooks").join(name);
            assert!(path.exists(), "{} not created", name);

            let content = std::fs::read_to_string(&path).unwrap();
            assert!(is_jig_managed(&content));
        }
    }

    #[test]
    fn init_is_idempotent() {
        let (_tmp, repo) = fake_repo();

        init_hooks(&repo, false).unwrap();
        let result = init_hooks(&repo, false).unwrap();

        assert!(result
            .results
            .iter()
            .all(|r| matches!(r, HookResult::AlreadyInstalled(_))));
    }

    #[test]
    fn init_backs_up_user_hooks() {
        let (_tmp, repo) = fake_repo();
        let hook_path = repo.join(".git/hooks/post-commit");
        std::fs::write(&hook_path, "#!/bin/bash\necho 'user hook'").unwrap();

        init_hooks(&repo, false).unwrap();

        // User hook moved to .user
        let user_path = repo.join(".git/hooks/post-commit.user");
        assert!(user_path.exists());
        let user_content = std::fs::read_to_string(&user_path).unwrap();
        assert!(user_content.contains("user hook"));

        // Jig hook installed
        let installed_content = std::fs::read_to_string(&hook_path).unwrap();
        assert!(is_jig_managed(&installed_content));

        // Backup exists
        let entries: Vec<_> = std::fs::read_dir(repo.join(".git/hooks"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("post-commit.backup-")
            })
            .collect();
        assert_eq!(entries.len(), 1);

        // Registry tracks backup
        let registry = HookRegistry::load(&repo).unwrap();
        let entry = &registry.installed["post-commit"];
        assert!(entry.had_existing);
        assert!(entry.backed_up_to.is_some());
    }

    #[test]
    fn init_force_reinstalls() {
        let (_tmp, repo) = fake_repo();

        init_hooks(&repo, false).unwrap();
        let result = init_hooks(&repo, true).unwrap();

        assert!(result
            .results
            .iter()
            .all(|r| matches!(r, HookResult::Installed(_))));
    }

    #[test]
    fn hooks_are_executable() {
        let (_tmp, repo) = fake_repo();
        init_hooks(&repo, false).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in MANAGED_HOOKS {
                let path = repo.join(".git/hooks").join(name);
                let mode = std::fs::metadata(&path).unwrap().permissions().mode();
                assert!(mode & 0o111 != 0, "{} not executable", name);
            }
        }
    }

    #[test]
    fn registry_saved_after_init() {
        let (_tmp, repo) = fake_repo();
        init_hooks(&repo, false).unwrap();

        let registry = HookRegistry::load(&repo).unwrap();
        for name in MANAGED_HOOKS {
            assert!(registry.is_installed(name), "{} not in registry", name);
        }
    }

    #[test]
    fn should_install_decisions() {
        let (_tmp, repo) = fake_repo();
        let hooks_dir = repo.join(".git/hooks");
        let registry = HookRegistry::new();

        // No file → Install
        let hook_path = hooks_dir.join("post-commit");
        assert_eq!(
            should_install_hook(&hook_path, &registry, "post-commit", false).unwrap(),
            InstallDecision::Install
        );

        // Force → Reinstall
        assert_eq!(
            should_install_hook(&hook_path, &registry, "post-commit", true).unwrap(),
            InstallDecision::Reinstall
        );

        // User hook → BackupAndInstall
        std::fs::write(&hook_path, "#!/bin/bash\necho 'user'").unwrap();
        assert_eq!(
            should_install_hook(&hook_path, &registry, "post-commit", false).unwrap(),
            InstallDecision::BackupAndInstall
        );

        // Jig-managed but not in registry → UpdateRegistry
        std::fs::write(
            &hook_path,
            "#!/bin/bash\n# jig-managed: v1\njig hooks post-commit",
        )
        .unwrap();
        assert_eq!(
            should_install_hook(&hook_path, &registry, "post-commit", false).unwrap(),
            InstallDecision::UpdateRegistry
        );

        // Jig-managed and in registry → Skip
        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");
        assert_eq!(
            should_install_hook(&hook_path, &registry, "post-commit", false).unwrap(),
            InstallDecision::Skip
        );
    }
}
