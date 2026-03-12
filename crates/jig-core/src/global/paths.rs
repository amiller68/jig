//! Global path helpers
//!
//! Delegates to `Config::config_dir()` for XDG resolution.

use std::path::PathBuf;

use crate::config::Config;
use crate::error::Result;

/// `~/.config/jig/`
pub fn global_config_dir() -> Result<PathBuf> {
    Config::config_dir()
}

/// `~/.config/jig/state/`
pub fn global_state_dir() -> Result<PathBuf> {
    Ok(Config::config_dir()?.join("state"))
}

/// `~/.config/jig/hooks/`
pub fn global_hooks_dir() -> Result<PathBuf> {
    Ok(Config::config_dir()?.join("hooks"))
}

/// `~/.config/jig/state/daemon.jsonl`
pub fn daemon_log_path() -> Result<PathBuf> {
    Ok(global_state_dir()?.join("daemon.jsonl"))
}

/// `~/.config/jig/state/events/<repo>-<worker>/`
pub fn worker_events_dir(repo: &str, worker: &str) -> Result<PathBuf> {
    Ok(global_state_dir()?
        .join("events")
        .join(format!("{}-{}", repo, worker)))
}

/// Create all global directories (config, state, hooks, state/events).
pub fn ensure_global_dirs() -> Result<()> {
    let dirs = [
        global_config_dir()?,
        global_state_dir()?,
        global_hooks_dir()?,
        global_state_dir()?.join("events"),
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
    fn state_dir_ends_with_state() {
        let dir = global_state_dir().unwrap();
        assert!(dir.ends_with("state"));
        assert!(dir.parent().unwrap().ends_with("jig"));
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

        std::env::remove_var("XDG_CONFIG_HOME");
    }

    #[test]
    fn worker_events_dir_format() {
        let dir = worker_events_dir("myrepo", "feat-branch").unwrap();
        assert!(dir.ends_with("myrepo-feat-branch"));
        assert!(dir.parent().unwrap().ends_with("events"));
    }
}
