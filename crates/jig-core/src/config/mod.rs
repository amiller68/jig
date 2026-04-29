//! Unified configuration module.
//!
//! Three tiers:
//! - Global: `~/.config/jig/config.toml` (user-wide defaults)
//! - Repo committed: `jig.toml` (checked into the repo)
//! - Repo local: `jig.local.toml` (gitignored overrides, merged on top)

pub mod global;
pub mod hooks;
pub mod paths;
pub mod registry;
pub mod repo;

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::git::Repo;
use crate::issues::{self, IssueProvider, LinearProvider};

// Re-export commonly used types
pub use global::{
    GitHubConfig, GlobalConfig, GlobalDaemonConfig, GlobalSpawnConfig, HealthConfig, LinearConfig,
    LinearProfile, NotifyConfig,
};
pub use hooks::{
    copy_worktree_files, get_copy_files, run_on_create_hook, run_on_create_hook_for_repo,
};
pub use paths::{
    daemon_log_path, ensure_global_dirs, global_config_dir, global_config_path, global_events_dir,
    global_hooks_dir, global_state_dir, hook_registry_path, notifications_path, repo_registry_path,
    triages_path, worker_events_dir, workers_state_path,
};
pub use registry::{RepoEntry, RepoRegistry};
pub use repo::{
    AgentConfig, ConventionalCommitsConfig, IssuesConfig, JigToml, LinearIssuesConfig,
    NudgeTypeConfig, NudgeTypeConfigs, RepoHealthConfig, ResolvedNudgeConfig, ReviewConfig,
    SpawnConfig, TriageConfig, WorktreeConfig,
};
pub use crate::worker::state::{WorkerEntry, WorkersState};

/// Directory name for jig-managed worktrees (relative to repo root)
pub const JIG_DIR: &str = ".jig";
/// Repo config file name
pub const JIG_TOML: &str = "jig.toml";
/// Local (gitignored) config overlay file name
pub const JIG_LOCAL_TOML: &str = "jig.local.toml";
/// Subdirectory within JIG_DIR (inside a worktree) for review files
pub const REVIEWS_DIR: &str = "reviews";
/// Default base branch when nothing is configured
pub const DEFAULT_BASE_BRANCH: &str = "origin/main";

/// Build the worktree path for a worker within a repo root.
pub fn worktree_path(repo_root: &Path, worker_name: &str) -> PathBuf {
    repo_root.join(JIG_DIR).join(worker_name)
}

/// Everything jig needs to know about a repo, loaded once.
pub struct Config {
    /// Base repository root (even when invoked from a worktree)
    pub repo_root: PathBuf,
    /// Directory containing jig-managed worktrees (<repo_root>/.jig)
    pub worktrees_path: PathBuf,
    /// The .git common directory
    pub git_common_dir: PathBuf,
    /// Global user configuration (~/.config/jig/config.toml)
    pub global: GlobalConfig,
    /// Repository configuration (jig.toml merged with jig.local.toml)
    pub repo: JigToml,
}

impl Config {
    /// Discover repo from cwd, load all config.
    pub fn from_cwd() -> Result<Self> {
        let git_repo = Repo::discover()?;
        let git_common_dir = git_repo.common_dir();
        let repo_root = git_common_dir
            .parent()
            .unwrap_or(&git_common_dir)
            .to_path_buf();
        Self::build(repo_root, git_common_dir)
    }

    /// Load from explicit repo path.
    pub fn from_path(path: &Path) -> Result<Self> {
        let git_repo = Repo::open(path)?;
        let git_common_dir = git_repo.common_dir();
        let repo_root = git_common_dir
            .parent()
            .unwrap_or(&git_common_dir)
            .to_path_buf();
        Self::build(repo_root, git_common_dir)
    }

    fn build(repo_root: PathBuf, git_common_dir: PathBuf) -> Result<Self> {
        let worktrees_path = repo_root.join(JIG_DIR);
        let repo = JigToml::load(&repo_root)?.unwrap_or_default();
        let global = GlobalConfig::load().unwrap_or_default();

        Ok(Self {
            repo_root,
            worktrees_path,
            git_common_dir,
            global,
            repo,
        })
    }

    /// Effective base branch: jig.toml > global config > "origin/main"
    pub fn base_branch(&self) -> String {
        self.repo
            .worktree
            .base
            .clone()
            .or_else(|| self.global.default_base_branch.clone())
            .unwrap_or_else(|| DEFAULT_BASE_BRANCH.to_string())
    }

    /// Resolve the effective base branch for an arbitrary repo path
    /// (without building a full Config). Used by daemon code.
    pub fn resolve_base_branch_for(repo_root: &Path) -> Result<String> {
        if let Ok(Some(jig_toml)) = JigToml::load(repo_root) {
            if let Some(base) = jig_toml.worktree.base {
                return Ok(base);
            }
        }
        let global = GlobalConfig::load().unwrap_or_default();
        Ok(global
            .default_base_branch
            .unwrap_or_else(|| DEFAULT_BASE_BRANCH.to_string()))
    }

    /// Tmux session name for this repo.
    pub fn session_name(&self) -> String {
        let repo_name = self
            .repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        format!("jig-{}", repo_name)
    }

    /// Create an issue provider based on repo and global configuration.
    pub fn issue_provider(&self) -> Result<IssueProvider> {
        issues::make_provider(&self.repo, &self.global)
    }

    /// Create a Linear provider (for mutation operations).
    pub fn linear_provider(&self) -> Result<LinearProvider> {
        issues::make_linear_provider(&self.repo, &self.global)
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

        std::env::set_var("XDG_CONFIG_HOME", config_dir.path());

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let cfg = Config::from_cwd();

        std::env::set_current_dir(&original).unwrap();

        let cfg = cfg.expect("from_cwd should succeed in a git repo");
        assert_eq!(
            cfg.repo_root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
        assert!(cfg.worktrees_path.ends_with(JIG_DIR));
        assert!(cfg.session_name().starts_with("jig-"));
        assert_eq!(cfg.base_branch(), "origin/main");
    }

    #[test]
    fn test_base_branch_from_jig_toml() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = tempfile::tempdir().unwrap();

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

        std::fs::write(
            dir.path().join("jig.toml"),
            "[worktree]\nbase = \"origin/develop\"\n",
        )
        .unwrap();

        std::env::set_var("XDG_CONFIG_HOME", config_dir.path());

        let cfg = Config::from_path(dir.path()).unwrap();
        assert_eq!(cfg.base_branch(), "origin/develop");
    }
}
