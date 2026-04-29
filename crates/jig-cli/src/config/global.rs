//! Structured global configuration (TOML)
//!
//! Stored at `~/.config/jig/config.toml`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use jig_core::error::{Error, Result};

use super::paths::global_config_path;

/// Health-check configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HealthConfig {
    /// Seconds of silence before a worker is considered stale.
    pub silence_threshold_seconds: u64,
    /// Maximum nudges before escalating.
    pub max_nudges: u32,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            silence_threshold_seconds: 300,
            max_nudges: 3,
        }
    }
}

/// Notification configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    /// Shell command to exec for notifications.
    pub exec: Option<String>,
    /// Webhook URL for notifications.
    pub webhook: Option<String>,
    /// Event names to subscribe to.
    pub events: Vec<String>,
}

/// GitHub integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitHubConfig {
    /// Auto-cleanup workers whose PRs have been merged.
    pub auto_cleanup_merged: bool,
    /// Auto-cleanup workers whose PRs have been closed without merging.
    pub auto_cleanup_closed: bool,
}

impl Default for GitHubConfig {
    fn default() -> Self {
        Self {
            auto_cleanup_merged: true,
            auto_cleanup_closed: false,
        }
    }
}

/// Linear API configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LinearConfig {
    /// Named profiles, each holding an API key.
    pub profiles: HashMap<String, LinearProfile>,
}

/// A single Linear API profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearProfile {
    pub api_key: String,
    /// Default team key (e.g. "ENG"). Used when per-repo config omits `team`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    /// Default project filter. Used when per-repo config omits `projects`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<String>,
    /// Default assignee filter. "me" resolves to the API key owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// Default label filter. Issues must carry all listed labels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

/// Daemon spawn configuration (global defaults).
///
/// Per-repo `jig.toml` can override these if explicitly set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalSpawnConfig {
    /// Max concurrent workers the daemon will auto-spawn per repo.
    pub max_concurrent_workers: usize,
    /// Seconds between issue polls for auto-spawn.
    pub auto_spawn_interval: u64,
}

impl Default for GlobalSpawnConfig {
    fn default() -> Self {
        Self {
            max_concurrent_workers: 3,
            auto_spawn_interval: 120,
        }
    }
}

/// Daemon configuration (global).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalDaemonConfig {
    /// Automatically recover orphaned workers on daemon startup.
    pub auto_recover: bool,
    /// Tick interval in seconds.
    pub interval_seconds: u64,
    /// Tmux session prefix (default: "jig-").
    pub session_prefix: String,
}

impl Default for GlobalDaemonConfig {
    fn default() -> Self {
        Self {
            auto_recover: true,
            interval_seconds: 30,
            session_prefix: "jig-".to_string(),
        }
    }
}

/// Global configuration stored at `~/.config/jig/config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    pub health: HealthConfig,
    pub notify: NotifyConfig,
    pub github: GitHubConfig,
    pub linear: LinearConfig,
    pub spawn: GlobalSpawnConfig,
    pub daemon: GlobalDaemonConfig,
    /// Default base branch for all repos (fallback when jig.toml doesn't set one).
    pub default_base_branch: Option<String>,
}

impl GlobalConfig {
    /// Load from the default path. Returns defaults if the file is missing.
    pub fn load() -> Result<Self> {
        let path = global_config_path()?;
        Self::load_from(&path)
    }

    /// Load from a specific path. Returns defaults if the file is missing.
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)?;
        let config: GlobalConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Return the default config file path.
    pub fn default_path() -> Result<std::path::PathBuf> {
        global_config_path()
    }

    /// Save to the default path.
    pub fn save(&self) -> Result<()> {
        let path = global_config_path()?;
        self.save_to(&path)
    }

    /// Save to a specific path, creating parent directories.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).map_err(|e| Error::Custom(e.to_string()))?;
        fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg = GlobalConfig::default();
        assert_eq!(cfg.health.silence_threshold_seconds, 300);
        assert_eq!(cfg.health.max_nudges, 3);
        assert!(cfg.notify.exec.is_none());
        assert!(cfg.notify.webhook.is_none());
        assert!(cfg.notify.events.is_empty());
        assert!(cfg.default_base_branch.is_none());
    }

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        let mut cfg = GlobalConfig::default();
        cfg.health.silence_threshold_seconds = 600;
        cfg.notify.exec = Some("notify-send".to_string());
        cfg.notify.events = vec!["worker.done".to_string()];
        cfg.default_base_branch = Some("origin/develop".to_string());

        cfg.save_to(&path).unwrap();
        let loaded = GlobalConfig::load_from(&path).unwrap();

        assert_eq!(loaded.health.silence_threshold_seconds, 600);
        assert_eq!(loaded.notify.exec.as_deref(), Some("notify-send"));
        assert_eq!(loaded.notify.events, vec!["worker.done"]);
        assert_eq!(
            loaded.default_base_branch.as_deref(),
            Some("origin/develop")
        );
    }

    #[test]
    fn missing_file_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        let cfg = GlobalConfig::load_from(&path).unwrap();
        assert_eq!(cfg.health.silence_threshold_seconds, 300);
    }

    #[test]
    fn linear_profile_with_filters() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            r#"
[linear.profiles.work]
api_key = "lin_api_test"
team = "ENG"
projects = ["Backend", "Platform"]
assignee = "me"
labels = ["auto", "backend"]
"#,
        )
        .unwrap();

        let cfg = GlobalConfig::load_from(&path).unwrap();
        let profile = cfg.linear.profiles.get("work").unwrap();
        assert_eq!(profile.api_key, "lin_api_test");
        assert_eq!(profile.team.as_deref(), Some("ENG"));
        assert_eq!(profile.projects, vec!["Backend", "Platform"]);
        assert_eq!(profile.assignee.as_deref(), Some("me"));
        assert_eq!(profile.labels, vec!["auto", "backend"]);
    }

    #[test]
    fn linear_profile_without_filters() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            r#"
[linear.profiles.minimal]
api_key = "lin_api_minimal"
"#,
        )
        .unwrap();

        let cfg = GlobalConfig::load_from(&path).unwrap();
        let profile = cfg.linear.profiles.get("minimal").unwrap();
        assert_eq!(profile.api_key, "lin_api_minimal");
        assert!(profile.team.is_none());
        assert!(profile.projects.is_empty());
        assert!(profile.assignee.is_none());
        assert!(profile.labels.is_empty());
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(&path, "[health]\nmax_nudges = 5\n").unwrap();

        let cfg = GlobalConfig::load_from(&path).unwrap();
        assert_eq!(cfg.health.max_nudges, 5);
        assert_eq!(cfg.health.silence_threshold_seconds, 300);
        assert!(cfg.notify.exec.is_none());
    }
}
