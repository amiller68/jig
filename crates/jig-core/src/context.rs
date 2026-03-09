//! Repository context — derived once at startup, threaded through all operations.

use std::path::PathBuf;

use crate::config::{Config, JigToml, JIG_DIR};
use crate::error::Result;
use crate::git;

/// All repo-derived state needed by jig operations.
/// Created once at startup to avoid redundant git subprocess calls.
pub struct RepoContext {
    /// Base repository root (even when invoked from a worktree)
    pub repo_root: PathBuf,
    /// Directory containing jig-managed worktrees (<repo_root>/.jig)
    pub worktrees_dir: PathBuf,
    /// The .git common directory (for exclude file, etc.)
    pub git_common_dir: PathBuf,
    /// Effective base branch (jig.toml > repo config > global config > fallback)
    pub base_branch: String,
    /// Tmux session name for this repo ("jig-<repo_name>")
    pub session_name: String,
}

impl RepoContext {
    /// Derive full repo context from the current working directory.
    pub fn from_cwd() -> Result<Self> {
        let git_common_dir = git::get_git_common_dir()?;
        let repo_root = git_common_dir
            .parent()
            .unwrap_or(&git_common_dir)
            .to_path_buf();
        Self::build(repo_root, git_common_dir)
    }

    /// Derive full repo context from a specific path.
    pub fn from_path(path: &std::path::Path) -> Result<Self> {
        let git_common_dir = git::get_git_common_dir_for(path)?;
        let repo_root = git_common_dir
            .parent()
            .unwrap_or(&git_common_dir)
            .to_path_buf();
        Self::build(repo_root, git_common_dir)
    }

    fn build(repo_root: PathBuf, git_common_dir: PathBuf) -> Result<Self> {
        let worktrees_dir = repo_root.join(JIG_DIR);

        let base_branch = Self::resolve_base_branch(&repo_root)?;

        let repo_name = repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let session_name = format!("jig-{}", repo_name);

        Ok(Self {
            repo_root,
            worktrees_dir,
            git_common_dir,
            base_branch,
            session_name,
        })
    }

    /// Resolve the effective base branch for an arbitrary repo path.
    /// Useful for daemon code that needs to resolve base branches without a full RepoContext.
    pub fn resolve_base_branch_for(repo_root: &std::path::Path) -> Result<String> {
        Self::resolve_base_branch(repo_root)
    }

    /// Resolve the effective base branch.
    /// Priority: jig.toml > repo-specific config > global default > hardcoded fallback.
    fn resolve_base_branch(repo_root: &std::path::Path) -> Result<String> {
        // Check jig.toml first — parse errors are non-fatal so a malformed
        // jig.toml doesn't prevent basic repo operations.
        if let Ok(Some(jig_toml)) = JigToml::load(repo_root) {
            if let Some(base) = jig_toml.worktree.base {
                return Ok(base);
            }
        }

        // Fall back to global config
        let config = Config::load()?;
        Ok(config.get_base_branch(repo_root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn test_from_cwd_in_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = tempfile::tempdir().unwrap();

        // Initialize a git repo
        Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "init", "-q"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Use isolated XDG config so global config doesn't interfere
        std::env::set_var("XDG_CONFIG_HOME", config_dir.path());

        // Save original dir, cd into temp repo
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let ctx = RepoContext::from_cwd();

        // Restore cwd before asserting (so cleanup works)
        std::env::set_current_dir(&original).unwrap();

        let ctx = ctx.expect("from_cwd should succeed in a git repo");
        assert_eq!(
            ctx.repo_root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
        assert!(ctx.worktrees_dir.ends_with(JIG_DIR));
        assert!(ctx.session_name.starts_with("jig-"));
        // Default fallback base branch
        assert_eq!(ctx.base_branch, "origin/main");
    }
}
