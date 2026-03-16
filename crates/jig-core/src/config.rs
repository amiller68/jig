//! Configuration management
//!
//! Handles both file-based user config (~/.config/jig/config) and
//! repository config (jig.toml).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub const DEFAULT_BASE_BRANCH: &str = "origin/main";

/// Directory name for jig-managed worktrees (relative to repo root)
pub const JIG_DIR: &str = ".jig";
/// Subdirectory within JIG_DIR for internal state files
pub const STATE_DIR: &str = ".state";
/// State file name
pub const STATE_FILE: &str = "state.json";

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
            worktree_dir: JIG_DIR.to_string(),
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

/// Build the worktree path for a worker within a repo root.
pub fn worktree_path(repo_root: &Path, worker_name: &str) -> PathBuf {
    repo_root.join(JIG_DIR).join(worker_name)
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

/// Conventional commits validation configuration in jig.toml `[commits]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConventionalCommitsConfig {
    /// Allowed commit types.
    pub types: Vec<String>,
    /// Require a scope.
    pub require_scope: bool,
    /// Allowed scopes (empty = any).
    pub scopes: Vec<String>,
    /// Allow breaking changes.
    pub allow_breaking: bool,
    /// Max subject line length.
    pub max_subject_length: usize,
    /// Require lowercase first char in subject.
    pub require_lowercase: bool,
}

impl Default for ConventionalCommitsConfig {
    fn default() -> Self {
        Self {
            types: [
                "feat", "fix", "docs", "style", "refactor", "perf", "test", "chore", "ci",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            require_scope: false,
            scopes: vec![],
            allow_breaking: true,
            max_subject_length: 72,
            require_lowercase: true,
        }
    }
}

impl ConventionalCommitsConfig {
    /// Convert to the core validation config.
    pub fn to_validation_config(&self) -> crate::commits::ValidationConfig {
        crate::commits::ValidationConfig {
            allowed_types: self.types.clone(),
            require_scope: self.require_scope,
            allowed_scopes: self.scopes.clone(),
            allow_breaking: self.allow_breaking,
            max_subject_length: self.max_subject_length,
            require_lowercase: self.require_lowercase,
        }
    }
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
    #[serde(default)]
    pub issues: IssuesConfig,
    #[serde(default)]
    pub health: RepoHealthConfig,
    #[serde(default)]
    pub commits: ConventionalCommitsConfig,
}

/// Per-repo health/nudge configuration in jig.toml `[health]`.
///
/// All fields are optional — when absent, the global config is used.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoHealthConfig {
    /// Override global silence threshold (seconds before a worker is "stalled").
    pub silence_threshold_seconds: Option<u64>,
    /// Override global max nudges before escalation.
    pub max_nudges: Option<u32>,
    /// Per-nudge-type overrides.
    #[serde(default)]
    pub nudge: NudgeTypeConfigs,
}

/// Per-nudge-type configuration overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NudgeTypeConfigs {
    pub idle: Option<NudgeTypeConfig>,
    pub stalled: Option<NudgeTypeConfig>,
    pub ci: Option<NudgeTypeConfig>,
    pub review: Option<NudgeTypeConfig>,
    pub conflict: Option<NudgeTypeConfig>,
    pub bad_commits: Option<NudgeTypeConfig>,
}

/// Configuration for a single nudge type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NudgeTypeConfig {
    /// Max nudges of this type before escalation.
    pub max: Option<u32>,
    /// Minimum seconds between nudges of this type.
    pub cooldown_seconds: Option<u64>,
}

/// Resolved nudge parameters for a specific nudge type.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedNudgeConfig {
    pub max: u32,
    pub cooldown_seconds: u64,
}

impl RepoHealthConfig {
    /// Resolve the effective silence threshold: jig.toml > global config.
    pub fn resolve_silence_threshold(&self, global: &crate::global::HealthConfig) -> u64 {
        self.silence_threshold_seconds
            .unwrap_or(global.silence_threshold_seconds)
    }

    /// Resolve the effective max nudges: jig.toml > global config.
    pub fn resolve_max_nudges(&self, global: &crate::global::HealthConfig) -> u32 {
        self.max_nudges.unwrap_or(global.max_nudges)
    }

    /// Resolve (max, cooldown) for a specific nudge type.
    ///
    /// Resolution: `[health.nudge.<type>]` > `[health]` > global config > defaults.
    pub fn resolve_for_nudge_type(
        &self,
        nudge_type_key: &str,
        global: &crate::global::HealthConfig,
    ) -> ResolvedNudgeConfig {
        let base_max = self.resolve_max_nudges(global);
        let base_cooldown = self.resolve_silence_threshold(global);

        let type_config = match nudge_type_key {
            "idle" => &self.nudge.idle,
            "stuck" | "stalled" => &self.nudge.stalled,
            "ci" => &self.nudge.ci,
            "review" => &self.nudge.review,
            "conflict" => &self.nudge.conflict,
            "bad_commits" => &self.nudge.bad_commits,
            _ => &None,
        };

        let max = type_config.as_ref().and_then(|c| c.max).unwrap_or(base_max);
        let cooldown_seconds = type_config
            .as_ref()
            .and_then(|c| c.cooldown_seconds)
            .unwrap_or(base_cooldown);

        ResolvedNudgeConfig {
            max,
            cooldown_seconds,
        }
    }
}

/// Issue tracking configuration in jig.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuesConfig {
    /// Provider type ("file" or "linear").
    #[serde(default = "default_issues_provider")]
    pub provider: String,
    /// Directory containing issue files (relative to repo root).
    #[serde(default = "default_issues_directory")]
    pub directory: String,
    /// Linear-specific configuration (required when provider = "linear").
    #[serde(default)]
    pub linear: Option<LinearIssuesConfig>,
    /// Labels required for auto-spawn (all must match). When set, only issues
    /// carrying all of these labels are eligible for daemon auto-spawning.
    #[serde(default)]
    pub spawn_labels: Vec<String>,
}

/// Linear issue provider configuration in jig.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIssuesConfig {
    /// Name of the profile in global config to use for API key.
    pub profile: String,
    /// Linear team key (e.g. "ENG"). Optional — falls back to profile default.
    #[serde(default)]
    pub team: Option<String>,
    /// Optional list of allowed project names to filter by.
    #[serde(default)]
    pub projects: Vec<String>,
    /// Optional assignee filter. "me" resolves to the API key owner.
    #[serde(default)]
    pub assignee: Option<String>,
    /// Optional label filter. Issues must carry all listed labels.
    #[serde(default)]
    pub labels: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_issues_provider() -> String {
    "file".to_string()
}

fn default_issues_directory() -> String {
    "issues".to_string()
}

impl Default for IssuesConfig {
    fn default() -> Self {
        Self {
            provider: default_issues_provider(),
            directory: default_issues_directory(),
            linear: None,
            spawn_labels: Vec::new(),
        }
    }
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

/// Spawn configuration in jig.toml (per-repo, committed).
///
/// `auto` is project-level (should the agent auto-start in spawned windows).
/// The daemon fields (`auto_spawn`, `max_concurrent_workers`,
/// `auto_spawn_interval`) are optional overrides of the global config
/// defaults in `~/.config/jig/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnConfig {
    /// Auto-start Claude in spawned windows (defaults to true).
    #[serde(default = "default_true")]
    pub auto: bool,
    /// Override global auto_spawn setting for this repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_spawn: Option<bool>,
    /// Override global max_concurrent_workers for this repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_workers: Option<usize>,
    /// Override global auto_spawn_interval for this repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_spawn_interval: Option<u64>,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            auto: true,
            auto_spawn: None,
            max_concurrent_workers: None,
            auto_spawn_interval: None,
        }
    }
}

impl SpawnConfig {
    /// Resolve auto_spawn: jig.toml override → global config default.
    pub fn resolve_auto_spawn(&self, global: &crate::global::GlobalSpawnConfig) -> bool {
        self.auto_spawn.unwrap_or(global.auto_spawn)
    }

    /// Resolve max_concurrent_workers: jig.toml override → global config default.
    pub fn resolve_max_concurrent_workers(
        &self,
        global: &crate::global::GlobalSpawnConfig,
    ) -> usize {
        self.max_concurrent_workers
            .unwrap_or(global.max_concurrent_workers)
    }

    /// Resolve auto_spawn_interval: jig.toml override → global config default.
    pub fn resolve_auto_spawn_interval(&self, global: &crate::global::GlobalSpawnConfig) -> u64 {
        self.auto_spawn_interval
            .unwrap_or(global.auto_spawn_interval)
    }
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

/// Run on-create hook if configured for a repo.
/// Priority: jig.toml > global config.
pub fn run_on_create_hook_for_repo(repo_root: &Path, worktree_path: &Path) -> Result<()> {
    // Check jig.toml first
    let hook = if let Some(jig_toml) = JigToml::load(repo_root)? {
        jig_toml.worktree.on_create
    } else {
        None
    };

    // Fall back to global config
    let hook = hook.or_else(|| {
        Config::load()
            .ok()
            .and_then(|c| c.get_on_create_hook(repo_root))
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
    // Auto-spawn fields
    pub auto_spawn: bool,
    pub auto_spawn_source: String,
    pub auto_start: bool,
    pub max_concurrent_workers: usize,
    pub max_concurrent_workers_source: String,
    pub auto_spawn_interval: u64,
    pub auto_spawn_interval_source: String,
    pub spawn_labels: Vec<String>,
    // Nudge health config
    pub silence_threshold_seconds: u64,
    pub silence_threshold_source: String,
    pub max_nudges: u32,
    pub max_nudges_source: String,
    /// Per-nudge-type resolved configs: (type_name, resolved, source).
    pub nudge_type_configs: Vec<(String, ResolvedNudgeConfig, String)>,
}

impl ConfigDisplay {
    pub fn load(repo_path: &Path) -> Result<Self> {
        let config = Config::load()?;
        let jig_toml = JigToml::load(repo_path)?.unwrap_or_default();
        let global_config = crate::global::GlobalConfig::load().unwrap_or_default();

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

        // Resolve auto-spawn settings (jig.toml override > global config > default)
        let global_spawn = &global_config.spawn;
        let spawn = &jig_toml.spawn;

        let (auto_spawn, auto_spawn_source) = if spawn.auto_spawn.is_some() {
            (
                spawn.resolve_auto_spawn(global_spawn),
                "jig.toml".to_string(),
            )
        } else {
            (global_spawn.auto_spawn, "global config".to_string())
        };

        let (max_concurrent_workers, max_concurrent_workers_source) =
            if spawn.max_concurrent_workers.is_some() {
                (
                    spawn.resolve_max_concurrent_workers(global_spawn),
                    "jig.toml".to_string(),
                )
            } else {
                (
                    global_spawn.max_concurrent_workers,
                    "global config".to_string(),
                )
            };

        let (auto_spawn_interval, auto_spawn_interval_source) =
            if spawn.auto_spawn_interval.is_some() {
                (
                    spawn.resolve_auto_spawn_interval(global_spawn),
                    "jig.toml".to_string(),
                )
            } else {
                (
                    global_spawn.auto_spawn_interval,
                    "global config".to_string(),
                )
            };

        // Resolve nudge health config
        let health = &jig_toml.health;
        let global_health = &global_config.health;

        let (silence_threshold_seconds, silence_threshold_source) =
            if health.silence_threshold_seconds.is_some() {
                (
                    health.resolve_silence_threshold(global_health),
                    "jig.toml".to_string(),
                )
            } else {
                (
                    global_health.silence_threshold_seconds,
                    "global config".to_string(),
                )
            };

        let (max_nudges, max_nudges_source) = if health.max_nudges.is_some() {
            (
                health.resolve_max_nudges(global_health),
                "jig.toml".to_string(),
            )
        } else {
            (global_health.max_nudges, "global config".to_string())
        };

        // Resolve per-type configs
        let nudge_types = ["idle", "stalled", "ci", "review", "conflict", "bad_commits"];
        let nudge_type_configs: Vec<(String, ResolvedNudgeConfig, String)> = nudge_types
            .iter()
            .map(|&nt| {
                let resolved = health.resolve_for_nudge_type(nt, global_health);
                let type_cfg = match nt {
                    "idle" => &health.nudge.idle,
                    "stalled" => &health.nudge.stalled,
                    "ci" => &health.nudge.ci,
                    "review" => &health.nudge.review,
                    "conflict" => &health.nudge.conflict,
                    "bad_commits" => &health.nudge.bad_commits,
                    _ => &None,
                };
                let source = if type_cfg.is_some() {
                    "jig.toml [health.nudge]".to_string()
                } else if health.max_nudges.is_some() || health.silence_threshold_seconds.is_some()
                {
                    "jig.toml [health]".to_string()
                } else {
                    "global config".to_string()
                };
                (nt.to_string(), resolved, source)
            })
            .collect();

        Ok(Self {
            effective_base,
            toml_base: jig_toml.worktree.base,
            repo_base: config.get_repo_base_branch(repo_path),
            global_base: config.get_global_base_branch(),
            effective_on_create,
            toml_on_create: jig_toml.worktree.on_create,
            global_on_create: config.get_on_create_hook(repo_path),
            auto_spawn,
            auto_spawn_source,
            auto_start: jig_toml.spawn.auto,
            max_concurrent_workers,
            max_concurrent_workers_source,
            auto_spawn_interval,
            auto_spawn_interval_source,
            spawn_labels: jig_toml.issues.spawn_labels,
            silence_threshold_seconds,
            silence_threshold_source,
            max_nudges,
            max_nudges_source,
            nudge_type_configs,
        })
    }
}

// Convenience functions that don't need repo context (global operations)

/// Get all config entries for --list
pub fn list_all_config() -> Result<Vec<(String, String, String)>> {
    let config = Config::load()?;
    Ok(config.list_all())
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

// Convenience functions that accept repo_root

/// Set repo-specific base branch
pub fn set_repo_base_branch(repo_root: &Path, branch: &str) -> Result<()> {
    let mut config = Config::load()?;
    config.set_repo_base_branch(repo_root, branch);
    config.save()
}

/// Unset repo-specific base branch
pub fn unset_repo_base_branch(repo_root: &Path) -> Result<()> {
    let mut config = Config::load()?;
    config.unset_repo_base_branch(repo_root);
    config.save()
}

/// Get repo-specific base branch
pub fn get_repo_base_branch(repo_root: &Path) -> Result<Option<String>> {
    let config = Config::load()?;
    Ok(config.get_repo_base_branch(repo_root))
}

/// Set on-create hook for a repo
pub fn set_on_create_hook(repo_root: &Path, command: &str) -> Result<()> {
    let mut config = Config::load()?;
    config.set_on_create_hook(repo_root, command);
    config.save()
}

/// Unset on-create hook for a repo
pub fn unset_on_create_hook(repo_root: &Path) -> Result<()> {
    let mut config = Config::load()?;
    config.unset_on_create_hook(repo_root);
    config.save()
}

/// Get on-create hook for a repo.
/// Priority: jig.toml > global config.
pub fn get_on_create_hook(repo_root: &Path) -> Result<Option<String>> {
    // Check jig.toml first
    if let Some(jig_toml) = JigToml::load(repo_root)? {
        if jig_toml.worktree.on_create.is_some() {
            return Ok(jig_toml.worktree.on_create);
        }
    }

    // Fall back to global config
    let config = Config::load()?;
    Ok(config.get_on_create_hook(repo_root))
}

/// Get list of files to copy to new worktrees
pub fn get_copy_files(repo_root: &Path) -> Result<Vec<String>> {
    if let Some(jig_toml) = JigToml::load(repo_root)? {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global::HealthConfig;

    fn global_health() -> HealthConfig {
        HealthConfig {
            silence_threshold_seconds: 300,
            max_nudges: 3,
        }
    }

    #[test]
    fn parse_jig_toml_with_assignee() {
        let toml_str = r#"
[issues]
provider = "linear"

[issues.linear]
profile = "work"
team = "ENG"
projects = ["Backend"]
assignee = "alice@co.com"
labels = ["auto"]
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        let linear = config.issues.linear.unwrap();
        assert_eq!(linear.profile, "work");
        assert_eq!(linear.team.as_deref(), Some("ENG"));
        assert_eq!(linear.projects, vec!["Backend"]);
        assert_eq!(linear.assignee.as_deref(), Some("alice@co.com"));
        assert_eq!(linear.labels, vec!["auto"]);
    }

    #[test]
    fn parse_jig_toml_linear_minimal() {
        let toml_str = r#"
[issues]
provider = "linear"

[issues.linear]
profile = "work"
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        let linear = config.issues.linear.unwrap();
        assert_eq!(linear.profile, "work");
        assert!(linear.team.is_none());
        assert!(linear.projects.is_empty());
        assert!(linear.assignee.is_none());
        assert!(linear.labels.is_empty());
    }

    #[test]
    fn parse_jig_toml_with_health() {
        let toml_str = r#"
[health]
silence_threshold_seconds = 600
max_nudges = 5

[health.nudge.idle]
max = 3
cooldown_seconds = 120

[health.nudge.ci]
max = 2
cooldown_seconds = 60
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        assert_eq!(config.health.silence_threshold_seconds, Some(600));
        assert_eq!(config.health.max_nudges, Some(5));
        assert_eq!(config.health.nudge.idle.as_ref().unwrap().max, Some(3));
        assert_eq!(
            config.health.nudge.idle.as_ref().unwrap().cooldown_seconds,
            Some(120)
        );
        assert_eq!(config.health.nudge.ci.as_ref().unwrap().max, Some(2));
        assert!(config.health.nudge.review.is_none());
    }

    #[test]
    fn parse_jig_toml_without_health() {
        let toml_str = r#"
[worktree]
base = "origin/main"
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        assert!(config.health.silence_threshold_seconds.is_none());
        assert!(config.health.max_nudges.is_none());
    }

    #[test]
    fn resolve_defaults_to_global() {
        let repo_health = RepoHealthConfig::default();
        let global = global_health();

        let resolved = repo_health.resolve_for_nudge_type("idle", &global);
        assert_eq!(resolved.max, 3);
        assert_eq!(resolved.cooldown_seconds, 300);
    }

    #[test]
    fn resolve_repo_overrides_global() {
        let repo_health = RepoHealthConfig {
            silence_threshold_seconds: Some(600),
            max_nudges: Some(5),
            ..Default::default()
        };
        let global = global_health();

        let resolved = repo_health.resolve_for_nudge_type("idle", &global);
        assert_eq!(resolved.max, 5);
        assert_eq!(resolved.cooldown_seconds, 600);
    }

    #[test]
    fn resolve_per_type_overrides_repo() {
        let repo_health = RepoHealthConfig {
            silence_threshold_seconds: Some(600),
            max_nudges: Some(5),
            nudge: NudgeTypeConfigs {
                ci: Some(NudgeTypeConfig {
                    max: Some(2),
                    cooldown_seconds: Some(60),
                }),
                ..Default::default()
            },
        };
        let global = global_health();

        // CI type uses per-type overrides
        let ci = repo_health.resolve_for_nudge_type("ci", &global);
        assert_eq!(ci.max, 2);
        assert_eq!(ci.cooldown_seconds, 60);

        // Idle type falls back to repo-level
        let idle = repo_health.resolve_for_nudge_type("idle", &global);
        assert_eq!(idle.max, 5);
        assert_eq!(idle.cooldown_seconds, 600);
    }

    #[test]
    fn resolve_partial_type_config() {
        let repo_health = RepoHealthConfig {
            max_nudges: Some(5),
            nudge: NudgeTypeConfigs {
                review: Some(NudgeTypeConfig {
                    max: Some(10),
                    cooldown_seconds: None, // falls back to silence_threshold
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let global = global_health();

        let review = repo_health.resolve_for_nudge_type("review", &global);
        assert_eq!(review.max, 10);
        assert_eq!(review.cooldown_seconds, 300); // from global silence_threshold
    }
}
