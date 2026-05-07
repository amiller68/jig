//! Context module — configuration, state, and runtime context.
//!
//! Three tiers of config:
//! - Global: `~/.config/jig/config.toml` (user-wide defaults)
//! - Repo committed: `jig.toml` (checked into the repo)
//! - Repo local: `jig.local.toml` (gitignored overrides, merged on top)
//!
//! `Context` composes config + repo registry + resolved repo configs.

pub mod config;
pub mod log;
pub mod paths;
pub mod registry;
pub mod repo;

use std::path::{Path, PathBuf};

use jig_core::error::Result;
use jig_core::git::{Branch, Repo};
use jig_core::issues::{IssueProvider, LinearProvider};

pub use config::{Config, LinearConfig, LinearProfile, NotifyConfig};
pub use paths::{
    daemon_log_path, daemon_logs_dir, ensure_global_dirs, global_config_dir, global_config_path,
    global_events_dir, global_hooks_dir, global_state_dir, hook_registry_path, latest_daemon_log,
    new_daemon_log_path, notifications_path, repo_registry_path, triages_path, worker_events_dir,
};
pub use registry::{RepoEntry, RepoRegistry};
pub use repo::{
    AgentConfig, IssuesConfig, JigToml, LinearIssuesConfig, SpawnConfig, TriageConfig,
    WorktreeConfig,
};

/// Directory name for jig-managed worktrees (relative to repo root)
pub const JIG_DIR: &str = ".jig";
/// Repo config file name
pub const JIG_TOML: &str = "jig.toml";
/// Local (gitignored) config overlay file name
pub const JIG_LOCAL_TOML: &str = "jig.local.toml";
/// Default base branch when nothing is configured
pub const DEFAULT_BASE_BRANCH: &str = "origin/main";

/// Build the worktree path for a worker within a repo root.
pub fn worktree_path(repo_root: &Path, worker_name: &str) -> PathBuf {
    repo_root.join(JIG_DIR).join(worker_name)
}

/// Per-repo configuration: paths + jig.toml.
pub struct RepoConfig {
    pub repo_root: PathBuf,
    pub worktrees_path: PathBuf,
    pub git_common_dir: PathBuf,
    pub repo: JigToml,
}

impl RepoConfig {
    pub fn from_cwd() -> Result<Self> {
        let git_repo = Repo::discover()?;
        let git_common_dir = git_repo.common_dir();
        let repo_root = git_common_dir
            .parent()
            .unwrap_or(&git_common_dir)
            .to_path_buf();
        Self::build(repo_root, git_common_dir)
    }

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
        Ok(Self {
            repo_root,
            worktrees_path,
            git_common_dir,
            repo,
        })
    }

    /// Effective base branch: jig.toml > global config > "origin/main"
    pub fn base_branch(&self, config: &Config) -> Branch {
        let name = self
            .repo
            .worktree
            .base
            .clone()
            .or_else(|| config.default_base_branch.clone())
            .unwrap_or_else(|| DEFAULT_BASE_BRANCH.to_string());
        Branch::new(name)
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

    /// Create an issue provider from whatever backend is configured.
    pub fn issue_provider(&self, config: &Config) -> Result<IssueProvider> {
        if self.repo.issues.linear.is_some() {
            return Ok(IssueProvider::new(Box::new(self.linear_provider(config)?)));
        }
        Err(jig_core::error::Error::Custom(
            "no issue provider configured — add [issues.linear] to jig.toml".into(),
        ))
    }

    /// Create a Linear provider.
    pub fn linear_provider(&self, config: &Config) -> Result<LinearProvider> {
        let linear_config = self.repo.issues.linear.as_ref().ok_or_else(|| {
            jig_core::error::Error::Custom(
                "[issues.linear] config required when provider = \"linear\"".into(),
            )
        })?;

        let profile = config
            .linear
            .profiles
            .get(&linear_config.profile)
            .ok_or_else(|| {
                jig_core::error::Error::Custom(format!(
                    "Linear profile '{}' not found in global config (~/.config/jig/config.toml)",
                    linear_config.profile,
                ))
            })?;

        let team = linear_config
            .team
            .clone()
            .or_else(|| profile.team.clone())
            .ok_or_else(|| {
                jig_core::error::Error::Custom(
                    "Linear team key is required — set 'team' in [issues.linear] in jig.toml or in the profile in ~/.config/jig/config.toml"
                        .to_string(),
                )
            })?;

        let projects = if linear_config.projects.is_empty() {
            profile.projects.clone()
        } else {
            linear_config.projects.clone()
        };

        let labels = if linear_config.labels.is_empty() {
            profile.labels.clone()
        } else {
            linear_config.labels.clone()
        };

        let assignee = linear_config
            .assignee
            .clone()
            .or_else(|| profile.assignee.clone());

        LinearProvider::new(&profile.api_key, team, projects, assignee, labels)
    }
}

/// Runtime context: config + repo registry + resolved repo configs.
pub struct Context {
    pub config: Config,
    pub registry: RepoRegistry,
    pub repos: Vec<RepoConfig>,
}

impl Context {
    /// Single repo from cwd.
    pub fn from_cwd() -> Result<Self> {
        let config = Config::load().unwrap_or_default();
        let repo = RepoConfig::from_cwd()?;
        let mut registry = RepoRegistry::default();
        registry.register(repo.repo_root.clone());
        Ok(Self {
            config,
            registry,
            repos: vec![repo],
        })
    }

    /// All tracked repos.
    pub fn from_global() -> Result<Self> {
        let config = Config::load().unwrap_or_default();
        let registry = RepoRegistry::load()?;
        let repos = registry
            .repos()
            .iter()
            .filter(|e| e.path.exists())
            .filter_map(|e| RepoConfig::from_path(&e.path).ok())
            .collect();
        Ok(Self {
            config,
            registry,
            repos,
        })
    }

    /// Single repo convenience — errors if no repos.
    pub fn repo(&self) -> Result<&RepoConfig> {
        self.repos
            .first()
            .ok_or(jig_core::error::Error::NotInGitRepo)
    }
}

/// Resolve the effective base branch for an arbitrary repo path
/// (without building a full Context). Used by daemon code.
pub fn resolve_base_branch_for(repo_root: &Path) -> Result<Branch> {
    if let Ok(Some(jig_toml)) = JigToml::load(repo_root) {
        if let Some(base) = jig_toml.worktree.base {
            return Ok(Branch::new(base));
        }
    }
    let config = Config::load().unwrap_or_default();
    Ok(Branch::new(
        config
            .default_base_branch
            .unwrap_or_else(|| DEFAULT_BASE_BRANCH.to_string()),
    ))
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

        let ctx = Context::from_cwd();

        std::env::set_current_dir(&original).unwrap();

        let ctx = ctx.expect("from_cwd should succeed in a git repo");
        let repo = ctx.repo().unwrap();
        assert_eq!(
            repo.repo_root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
        assert!(repo.worktrees_path.ends_with(JIG_DIR));
        assert!(repo.session_name().starts_with("jig-"));
        assert_eq!(repo.base_branch(&ctx.config), "origin/main");
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

        let repo = RepoConfig::from_path(dir.path()).unwrap();
        let config = Config::load().unwrap_or_default();
        assert_eq!(repo.base_branch(&config), "origin/develop");
    }
}
