//! Global configuration — `~/.config/jig/config.toml`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use jig_core::error::{Error, Result};

use super::paths::global_config_path;

/// Notification configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    pub exec: Option<String>,
    pub webhook: Option<String>,
    pub events: Vec<String>,
}

/// Linear API configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LinearConfig {
    pub profiles: HashMap<String, LinearProfile>,
}

/// A single Linear API profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearProfile {
    pub api_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

/// Global configuration stored at `~/.config/jig/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // Health
    pub silence_threshold_seconds: u64,
    pub max_nudges: u32,

    // Spawn
    pub max_concurrent_workers: usize,

    // Daemon
    pub auto_recover: bool,
    pub tick_interval: u64,
    pub poll_interval: u64,
    pub session_prefix: String,

    // GitHub
    pub auto_cleanup_merged: bool,
    pub auto_cleanup_closed: bool,

    #[serde(default)]
    pub notify: NotifyConfig,

    #[serde(default)]
    pub linear: LinearConfig,

    pub default_base_branch: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            silence_threshold_seconds: 300,
            max_nudges: 3,
            max_concurrent_workers: 3,
            poll_interval: 120,
            auto_recover: true,
            tick_interval: 30,
            session_prefix: "jig-".to_string(),
            auto_cleanup_merged: true,
            auto_cleanup_closed: false,
            notify: NotifyConfig::default(),
            linear: LinearConfig::default(),
            default_base_branch: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = global_config_path()?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn default_path() -> Result<std::path::PathBuf> {
        global_config_path()
    }

    pub fn save(&self) -> Result<()> {
        let path = global_config_path()?;
        self.save_to(&path)
    }

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
        let cfg = Config::default();
        assert_eq!(cfg.silence_threshold_seconds, 300);
        assert_eq!(cfg.max_nudges, 3);
        assert_eq!(cfg.max_concurrent_workers, 3);
        assert_eq!(cfg.poll_interval, 120);
        assert!(cfg.auto_recover);
        assert_eq!(cfg.tick_interval, 30);
        assert!(cfg.notify.exec.is_none());
        assert!(cfg.default_base_branch.is_none());
    }

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");

        let mut cfg = Config::default();
        cfg.silence_threshold_seconds = 600;
        cfg.notify.exec = Some("notify-send".to_string());
        cfg.notify.events = vec!["worker.done".to_string()];
        cfg.default_base_branch = Some("origin/develop".to_string());

        cfg.save_to(&path).unwrap();
        let loaded = Config::load_from(&path).unwrap();

        assert_eq!(loaded.silence_threshold_seconds, 600);
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
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.silence_threshold_seconds, 300);
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

        let cfg = Config::load_from(&path).unwrap();
        let profile = cfg.linear.profiles.get("work").unwrap();
        assert_eq!(profile.api_key, "lin_api_test");
        assert_eq!(profile.team.as_deref(), Some("ENG"));
        assert_eq!(profile.projects, vec!["Backend", "Platform"]);
        assert_eq!(profile.assignee.as_deref(), Some("me"));
        assert_eq!(profile.labels, vec!["auto", "backend"]);
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(&path, "max_nudges = 5\n").unwrap();

        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.max_nudges, 5);
        assert_eq!(cfg.silence_threshold_seconds, 300);
        assert!(cfg.notify.exec.is_none());
    }
}
