//! Path helpers — all jig directories and file paths.

use std::path::PathBuf;

use jig_core::error::{Error, Result};

/// `~/.config/jig/`
pub fn global_config_dir() -> Result<PathBuf> {
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

/// `~/.config/jig/config.toml`
pub fn global_config_path() -> Result<PathBuf> {
    Ok(global_config_dir()?.join("config.toml"))
}

/// `~/.config/jig/hooks/`
pub fn global_hooks_dir() -> Result<PathBuf> {
    Ok(global_config_dir()?.join("hooks"))
}

/// `<repo_root>/.jig/hooks/hooks.json`
pub fn hook_registry_path(repo_root: &std::path::Path) -> PathBuf {
    repo_root
        .join(super::JIG_DIR)
        .join("hooks")
        .join("hooks.json")
}

/// `~/.config/jig/state/`
pub fn global_state_dir() -> Result<PathBuf> {
    Ok(global_config_dir()?.join("state"))
}

/// `~/.config/jig/state/daemon.jsonl`
pub fn daemon_log_path() -> Result<PathBuf> {
    Ok(global_state_dir()?.join("daemon.jsonl"))
}

/// `~/.config/jig/<repo>/<branch>/`
pub fn worker_events_dir(repo: &str, branch: &str) -> Result<PathBuf> {
    Ok(global_config_dir()?.join(repo).join(branch))
}

/// `~/.config/jig/repos.json`
pub fn repo_registry_path() -> Result<PathBuf> {
    Ok(global_config_dir()?.join("repos.json"))
}

/// `~/.config/jig/state/notifications.jsonl`
pub fn notifications_path() -> Result<PathBuf> {
    Ok(global_state_dir()?.join("notifications.jsonl"))
}

/// `~/.config/jig/state/triages.json`
pub fn triages_path() -> Result<PathBuf> {
    Ok(global_state_dir()?.join("triages.json"))
}

/// `~/.config/jig/state/events/`
pub fn global_events_dir() -> Result<PathBuf> {
    Ok(global_state_dir()?.join("events"))
}

/// `~/.config/jig/state/logs/`
pub fn daemon_logs_dir() -> Result<PathBuf> {
    Ok(global_state_dir()?.join("logs"))
}

/// Create a new session log path: `~/.config/jig/state/logs/<YYYYMMDDTHHMMSSZ>.log`
pub fn new_daemon_log_path() -> Result<PathBuf> {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    Ok(daemon_logs_dir()?.join(format!("{}.log", ts)))
}

/// Find the most recent daemon log file (lexicographic sort on ISO timestamps).
pub fn latest_daemon_log() -> Result<Option<PathBuf>> {
    let dir = daemon_logs_dir()?;
    if !dir.exists() {
        return Ok(None);
    }
    let mut logs: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "log")
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    logs.sort();
    Ok(logs.pop())
}

/// Create all global directories.
pub fn ensure_global_dirs() -> Result<()> {
    let dirs = [
        global_config_dir()?,
        global_hooks_dir()?,
        global_state_dir()?,
        global_events_dir()?,
        daemon_logs_dir()?,
    ];
    for dir in &dirs {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_ends_with_jig() {
        let dir = global_config_dir().unwrap();
        assert!(dir.ends_with("jig"));
    }

    #[test]
    fn ensure_global_dirs_creates_structure() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());

        ensure_global_dirs().unwrap();

        assert!(tmp.path().join("jig").is_dir());
        assert!(tmp.path().join("jig/state").is_dir());
        assert!(tmp.path().join("jig/hooks").is_dir());
        assert!(tmp.path().join("jig/state/events").is_dir());
        assert!(tmp.path().join("jig/state/logs").is_dir());

        std::env::remove_var("XDG_CONFIG_HOME");
    }
}
