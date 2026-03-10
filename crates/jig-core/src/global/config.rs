//! Structured global configuration (TOML)
//!
//! Stored at `~/.config/jig/config.toml`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::paths::global_config_dir;

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
}

/// Daemon spawn configuration (global defaults).
///
/// Per-repo `jig.toml` can override these if explicitly set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalSpawnConfig {
    /// Whether the daemon should auto-spawn workers for eligible issues.
    pub auto_spawn: bool,
    /// Max concurrent workers the daemon will auto-spawn per repo.
    pub max_concurrent_workers: usize,
    /// Seconds between issue polls for auto-spawn.
    pub auto_spawn_interval: u64,
}

impl Default for GlobalSpawnConfig {
    fn default() -> Self {
        Self {
            auto_spawn: false,
            max_concurrent_workers: 3,
            auto_spawn_interval: 120,
        }
    }
}

/// Daemon recovery configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecoveryConfig {
    /// Automatically recover orphaned workers on daemon startup.
    pub auto_recover: bool,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self { auto_recover: true }
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
    pub recovery: RecoveryConfig,
}

impl GlobalConfig {
    /// Load from the default path. Returns defaults if the file is missing.
    pub fn load() -> Result<Self> {
        let path = global_config_dir()?.join("config.toml");
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

    /// Save to the default path.
    pub fn save(&self) -> Result<()> {
        let path = global_config_dir()?.join("config.toml");
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
    }

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        let mut cfg = GlobalConfig::default();
        cfg.health.silence_threshold_seconds = 600;
        cfg.notify.exec = Some("notify-send".to_string());
        cfg.notify.events = vec!["worker.done".to_string()];

        cfg.save_to(&path).unwrap();
        let loaded = GlobalConfig::load_from(&path).unwrap();

        assert_eq!(loaded.health.silence_threshold_seconds, 600);
        assert_eq!(loaded.notify.exec.as_deref(), Some("notify-send"));
        assert_eq!(loaded.notify.events, vec!["worker.done"]);
    }

    #[test]
    fn missing_file_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        let cfg = GlobalConfig::load_from(&path).unwrap();
        assert_eq!(cfg.health.silence_threshold_seconds, 300);
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(&path, "[health]\nmax_nudges = 5\n").unwrap();

        let cfg = GlobalConfig::load_from(&path).unwrap();
        assert_eq!(cfg.health.max_nudges, 5);
        // silence_threshold_seconds should be default
        assert_eq!(cfg.health.silence_threshold_seconds, 300);
        // notify should be fully default
        assert!(cfg.notify.exec.is_none());
    }
}
