//! Configuration management
//!
//! Handles both file-based user config (~/.config/jig/config) and
//! repository config (jig.toml).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

const DEFAULT_BASE_BRANCH: &str = "origin/main";
const DEFAULT_WORKTREE_DIR: &str = ".worktrees";

/// Repository configuration (stored in jig.toml and state file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    /// Default base branch for new worktrees
    pub base_branch: String,
    /// Directory for worktrees (relative to repo root)
    pub worktree_dir: String,
    /// Shell command to run after worktree creation
    pub on_create_hook: Option<String>,
    /// Automatically transition to WaitingReview when worker is idle
    pub auto_review: bool,
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            base_branch: DEFAULT_BASE_BRANCH.to_string(),
            worktree_dir: DEFAULT_WORKTREE_DIR.to_string(),
            on_create_hook: None,
            auto_review: true,
        }
    }
}

/// Global user configuration
#[derive(Debug, Clone, Default)]
pub struct Config {
    entries: HashMap<String, String>,
}

impl Config {
    /// Get the config directory path
    pub fn config_dir() -> Result<PathBuf> {
        let config_dir = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(xdg).join("jig")
        } else {
            dirs::home_dir()
                .ok_or_else(|| Error::Custom("Could not find home directory".to_string()))?
                .join(".config")
                .join("jig")
        };

        Ok(config_dir)
    }

    /// Get the config file path
    pub fn config_file() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config"))
    }

    /// Load config from disk
    pub fn load() -> Result<Self> {
        let config_file = Self::config_file()?;
        let mut entries = HashMap::new();

        if config_file.exists() {
            let content = fs::read_to_string(&config_file)?;
            for line in content.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    entries.insert(key.to_string(), value.to_string());
                }
            }
        }

        Ok(Self { entries })
    }

    /// Save config to disk
    pub fn save(&self) -> Result<()> {
        let config_file = Self::config_file()?;

        fs::create_dir_all(config_file.parent().unwrap())?;

        let content: String = self
            .entries
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        fs::write(&config_file, content + "\n")?;
        Ok(())
    }

    /// Get a config value
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    /// Set a config value
    pub fn set(&mut self, key: String, value: String) {
        self.entries.insert(key, value);
    }

    /// Remove a config value
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.entries.remove(key)
    }

    /// Get all entries
    pub fn entries(&self) -> &HashMap<String, String> {
        &self.entries
    }

    /// Get the effective base branch for a repository
    pub fn get_base_branch(&self, repo_path: &Path) -> String {
        let repo_key = repo_path.to_string_lossy().to_string();

        // Try repo-specific first
        if let Some(branch) = self.entries.get(&repo_key) {
            return branch.clone();
        }

        // Try global default
        if let Some(branch) = self.entries.get("_default") {
            return branch.clone();
        }

        // Hardcoded fallback
        DEFAULT_BASE_BRANCH.to_string()
    }

    /// Set repo-specific base branch
    pub fn set_repo_base_branch(&mut self, repo_path: &Path, branch: &str) {
        let key = repo_path.to_string_lossy().to_string();
        self.set(key, branch.to_string());
    }

    /// Unset repo-specific base branch
    pub fn unset_repo_base_branch(&mut self, repo_path: &Path) {
        let key = repo_path.to_string_lossy().to_string();
        self.remove(&key);
    }

    /// Get repo-specific base branch (if set)
    pub fn get_repo_base_branch(&self, repo_path: &Path) -> Option<String> {
        let key = repo_path.to_string_lossy().to_string();
        self.entries.get(&key).cloned()
    }

    /// Set global default base branch
    pub fn set_global_base_branch(&mut self, branch: &str) {
        self.set("_default".to_string(), branch.to_string());
    }

    /// Unset global default base branch
    pub fn unset_global_base_branch(&mut self) {
        self.remove("_default");
    }

    /// Get global default base branch (if set)
    pub fn get_global_base_branch(&self) -> Option<String> {
        self.entries.get("_default").cloned()
    }

    /// Get on-create hook for repo
    pub fn get_on_create_hook(&self, repo_path: &Path) -> Option<String> {
        let key = format!("{}:on_create", repo_path.to_string_lossy());
        self.entries.get(&key).cloned()
    }

    /// Set on-create hook for repo
    pub fn set_on_create_hook(&mut self, repo_path: &Path, command: &str) {
        let key = format!("{}:on_create", repo_path.to_string_lossy());
        self.set(key, command.to_string());
    }

    /// Unset on-create hook for repo
    pub fn unset_on_create_hook(&mut self, repo_path: &Path) {
        let key = format!("{}:on_create", repo_path.to_string_lossy());
        self.remove(&key);
    }

    /// Get all config entries for --list
    pub fn list_all(&self) -> Vec<(String, String, String)> {
        let mut entries = Vec::new();

        for (key, value) in &self.entries {
            let category = if key == "_default" {
                "[global]".to_string()
            } else if key.contains(":on_create") {
                let path = key.strip_suffix(":on_create").unwrap_or(key);
                format!("[{}] on-create", path)
            } else {
                format!("[{}]", key)
            };

            let display_key = if key == "_default" {
                "base".to_string()
            } else if key.contains(":on_create") {
                "on-create".to_string()
            } else {
                "base".to_string()
            };

            entries.push((category, display_key, value.clone()));
        }

        entries
    }
}

/// Run on-create hook in a directory
pub fn run_on_create_hook(hook: &str, dir: &Path) -> Result<bool> {
    tracing::info!("Running on-create hook: {}", hook);

    let output = std::process::Command::new("sh")
        .args(["-c", hook])
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        tracing::warn!(
            "on-create hook failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(false);
    }

    Ok(true)
}

/// JigToml configuration from jig.toml
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct JigToml {
    #[serde(default)]
    pub worktree: WorktreeConfig,
    #[serde(default)]
    pub spawn: SpawnConfig,
    #[serde(default)]
    pub agent: AgentConfig,
}

/// Worktree configuration in jig.toml
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WorktreeConfig {
    /// Base branch for new worktrees (overrides global config)
    #[serde(default)]
    pub base: Option<String>,
    /// Shell command to run after worktree creation
    #[serde(default)]
    pub on_create: Option<String>,
    /// Gitignored files to copy to new worktrees (e.g., [".env", ".env.local"])
    #[serde(default)]
    pub copy: Vec<String>,
}

/// Spawn configuration in jig.toml
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SpawnConfig {
    #[serde(default)]
    pub auto: bool,
}

/// Agent configuration in jig.toml
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent type (e.g., "claude-code", "cursor")
    #[serde(rename = "type", default = "default_agent_type")]
    pub agent_type: String,
}

fn default_agent_type() -> String {
    "claude".to_string()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_type: default_agent_type(),
        }
    }
}

impl JigToml {
    /// Read jig.toml from a repository (falls back to jig.toml for compatibility)
    pub fn load(repo_root: &Path) -> Result<Option<Self>> {
        // Try jig.toml first
        let toml_path = repo_root.join("jig.toml");
        if toml_path.exists() {
            let content = fs::read_to_string(&toml_path)?;
            let config: JigToml = toml::from_str(&content)?;
            return Ok(Some(config));
        }

        // Fall back to jig.toml for backward compatibility
        let legacy_path = repo_root.join("jig.toml");
        if legacy_path.exists() {
            let content = fs::read_to_string(&legacy_path)?;
            let config: JigToml = toml::from_str(&content)?;
            return Ok(Some(config));
        }

        Ok(None)
    }

    /// Check if jig.toml (or jig.toml) exists
    pub fn exists(repo_root: &Path) -> bool {
        repo_root.join("jig.toml").exists() || repo_root.join("jig.toml").exists()
    }
}

/// Get the base branch for the current repository (convenience function)
/// Priority: jig.toml > repo-specific global config > global default > hardcoded fallback
pub fn get_base_branch() -> Result<String> {
    let repo_path = crate::git::get_base_repo()?;

    // Check jig.toml first
    if let Some(jig_toml) = JigToml::load(&repo_path)? {
        if let Some(base) = jig_toml.worktree.base {
            return Ok(base);
        }
    }

    // Fall back to global config
    let config = Config::load()?;
    Ok(config.get_base_branch(&repo_path))
}

/// Read jig.toml from current repository (convenience function)
pub fn read_jig_toml() -> Result<Option<JigToml>> {
    let repo_root = crate::git::get_base_repo()?;
    JigToml::load(&repo_root)
}

/// Run on-create hook if configured for current repo (convenience function)
/// Priority: jig.toml > global config
pub fn run_on_create_hook_for_repo(worktree_path: &Path) -> Result<()> {
    let repo_path = crate::git::get_base_repo()?;

    // Check jig.toml first
    let hook = if let Some(jig_toml) = JigToml::load(&repo_path)? {
        jig_toml.worktree.on_create
    } else {
        None
    };

    // Fall back to global config
    let hook = hook.or_else(|| {
        Config::load()
            .ok()
            .and_then(|c| c.get_on_create_hook(&repo_path))
    });

    if let Some(hook) = hook {
        let success = run_on_create_hook(&hook, worktree_path)?;
        if !success {
            tracing::warn!("on-create hook returned non-zero exit code");
        }
    }

    Ok(())
}

/// Configuration display for `jig config` command
pub struct ConfigDisplay {
    pub effective_base: String,
    pub toml_base: Option<String>,
    pub repo_base: Option<String>,
    pub global_base: Option<String>,
    pub effective_on_create: Option<String>,
    pub toml_on_create: Option<String>,
    pub global_on_create: Option<String>,
}

impl ConfigDisplay {
    pub fn load(repo_path: &Path) -> Result<Self> {
        let config = Config::load()?;
        let jig_toml = JigToml::load(repo_path)?.unwrap_or_default();

        // Get effective base branch (jig.toml > repo config > global > default)
        let effective_base = jig_toml
            .worktree
            .base
            .clone()
            .or_else(|| config.get_repo_base_branch(repo_path))
            .or_else(|| config.get_global_base_branch())
            .unwrap_or_else(|| DEFAULT_BASE_BRANCH.to_string());

        // Get effective on-create hook (jig.toml > global config)
        let effective_on_create = jig_toml
            .worktree
            .on_create
            .clone()
            .or_else(|| config.get_on_create_hook(repo_path));

        Ok(Self {
            effective_base,
            toml_base: jig_toml.worktree.base,
            repo_base: config.get_repo_base_branch(repo_path),
            global_base: config.get_global_base_branch(),
            effective_on_create,
            toml_on_create: jig_toml.worktree.on_create,
            global_on_create: config.get_on_create_hook(repo_path),
        })
    }

    /// Load config display for current repository
    pub fn load_auto() -> Result<Self> {
        let repo_path = crate::git::get_base_repo()?;
        Self::load(&repo_path)
    }
}

// Convenience functions for CLI that operate on current repo

/// Get all config entries for --list
pub fn list_all_config() -> Result<Vec<(String, String, String)>> {
    let config = Config::load()?;
    Ok(config.list_all())
}

/// Set repo-specific base branch for current repo
pub fn set_repo_base_branch(branch: &str) -> Result<()> {
    let repo_path = crate::git::get_base_repo()?;
    let mut config = Config::load()?;
    config.set_repo_base_branch(&repo_path, branch);
    config.save()
}

/// Unset repo-specific base branch for current repo
pub fn unset_repo_base_branch() -> Result<()> {
    let repo_path = crate::git::get_base_repo()?;
    let mut config = Config::load()?;
    config.unset_repo_base_branch(&repo_path);
    config.save()
}

/// Get repo-specific base branch for current repo
pub fn get_repo_base_branch() -> Result<Option<String>> {
    let repo_path = crate::git::get_base_repo()?;
    let config = Config::load()?;
    Ok(config.get_repo_base_branch(&repo_path))
}

/// Set global default base branch
pub fn set_global_base_branch(branch: &str) -> Result<()> {
    let mut config = Config::load()?;
    config.set_global_base_branch(branch);
    config.save()
}

/// Unset global default base branch
pub fn unset_global_base_branch() -> Result<()> {
    let mut config = Config::load()?;
    config.unset_global_base_branch();
    config.save()
}

/// Get global default base branch
pub fn get_global_base_branch() -> Result<Option<String>> {
    let config = Config::load()?;
    Ok(config.get_global_base_branch())
}

/// Set on-create hook for current repo
pub fn set_on_create_hook(command: &str) -> Result<()> {
    let repo_path = crate::git::get_base_repo()?;
    let mut config = Config::load()?;
    config.set_on_create_hook(&repo_path, command);
    config.save()
}

/// Unset on-create hook for current repo
pub fn unset_on_create_hook() -> Result<()> {
    let repo_path = crate::git::get_base_repo()?;
    let mut config = Config::load()?;
    config.unset_on_create_hook(&repo_path);
    config.save()
}

/// Get on-create hook for current repo
/// Priority: jig.toml > global config
pub fn get_on_create_hook() -> Result<Option<String>> {
    let repo_path = crate::git::get_base_repo()?;

    // Check jig.toml first
    if let Some(jig_toml) = JigToml::load(&repo_path)? {
        if jig_toml.worktree.on_create.is_some() {
            return Ok(jig_toml.worktree.on_create);
        }
    }

    // Fall back to global config
    let config = Config::load()?;
    Ok(config.get_on_create_hook(&repo_path))
}

/// Check if jig.toml exists in current repo
pub fn has_jig_toml() -> Result<bool> {
    let repo_root = crate::git::get_base_repo()?;
    Ok(JigToml::exists(&repo_root))
}

/// Get list of files to copy to new worktrees
pub fn get_copy_files() -> Result<Vec<String>> {
    let repo_root = crate::git::get_base_repo()?;
    if let Some(jig_toml) = JigToml::load(&repo_root)? {
        Ok(jig_toml.worktree.copy)
    } else {
        Ok(Vec::new())
    }
}

/// Copy configured files from source to destination
pub fn copy_worktree_files(src_root: &Path, dst_root: &Path, files: &[String]) -> Result<()> {
    for file in files {
        let src = src_root.join(file);
        let dst = dst_root.join(file);

        if src.exists() {
            // Create parent directories if needed
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src, &dst)?;
            tracing::info!("Copied {} to worktree", file);
        }
    }
    Ok(())
}
