//! Claude Code agent hook installation.
//!
//! Installs the shared jig agent hook scripts into `~/.claude/hooks/`
//! and registers them in `~/.claude/settings.json`.

use std::path::{Path, PathBuf};

use crate::error::Result;

/// Agent hook scripts — event plumbing installed into the agent's hook system.
/// Each entry is (event_name, script_content).
pub const AGENT_HOOK_SCRIPTS: &[(&str, &str)] = &[
    ("PostToolUse", include_str!("agent_scripts/PostToolUse.sh")),
    (
        "Notification",
        include_str!("agent_scripts/Notification.sh"),
    ),
    ("Stop", include_str!("agent_scripts/Stop.sh")),
];

fn hooks_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| crate::Error::Custom("no home directory".into()))?;
    Ok(home.join(".claude").join("hooks"))
}

fn settings_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| crate::Error::Custom("no home directory".into()))?;
    Ok(home.join(".claude").join("settings.json"))
}

#[derive(Debug, Default)]
pub struct InstallResult {
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
    pub settings_updated: bool,
}

/// Install jig agent hook scripts to `~/.claude/hooks/` and register
/// them in `~/.claude/settings.json`.
pub fn install_claude_hooks() -> Result<InstallResult> {
    let dir = hooks_dir()?;
    let mut result = install_hooks_to(&dir)?;
    result.settings_updated = register_hooks_in_settings(&dir)?;
    Ok(result)
}

/// Install jig agent hook scripts to the given directory.
pub fn install_hooks_to(hooks_dir: &Path) -> Result<InstallResult> {
    std::fs::create_dir_all(hooks_dir)?;

    let mut result = InstallResult::default();

    for (name, content) in AGENT_HOOK_SCRIPTS {
        let path = hooks_dir.join(name);
        let existed = path.exists();
        std::fs::write(&path, content)?;
        make_executable(&path)?;
        if existed {
            result.skipped.push(name.to_string());
        } else {
            result.installed.push(name.to_string());
        }
    }

    Ok(result)
}

/// Register jig hooks in `~/.claude/settings.json` so Claude Code invokes them.
///
/// Merges into existing settings without clobbering other hook entries.
fn register_hooks_in_settings(hooks_dir: &Path) -> Result<bool> {
    let settings_path = settings_path()?;

    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let hooks_obj = settings
        .as_object_mut()
        .ok_or_else(|| crate::Error::Custom("settings.json is not an object".into()))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_map = hooks_obj
        .as_object_mut()
        .ok_or_else(|| crate::Error::Custom("hooks is not an object".into()))?;

    let mut modified = false;

    for (event_name, _) in AGENT_HOOK_SCRIPTS {
        let script_path = hooks_dir.join(event_name);
        let script_path_str = script_path.to_string_lossy().to_string();

        let event_hooks = hooks_map
            .entry(*event_name)
            .or_insert_with(|| serde_json::json!([]));

        let entries = match event_hooks.as_array_mut() {
            Some(arr) => arr,
            None => continue,
        };

        let has_jig_hook = entries.iter().any(|entry| {
            entry
                .get("hooks")
                .and_then(|h| h.as_array())
                .map(|hooks| {
                    hooks.iter().any(|h| {
                        h.get("command")
                            .and_then(|c| c.as_str())
                            .map(|c| c.contains("jig"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        });

        if !has_jig_hook {
            entries.push(serde_json::json!({
                "hooks": [{
                    "type": "command",
                    "command": script_path_str
                }]
            }));
            modified = true;
        }
    }

    if modified {
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&settings)
            .map_err(|e| crate::Error::Custom(format!("failed to serialize settings: {}", e)))?;
        std::fs::write(&settings_path, content)?;
    }

    Ok(modified)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_non_empty() {
        for (name, content) in AGENT_HOOK_SCRIPTS {
            assert!(!content.is_empty(), "{} template is empty", name);
            assert!(
                content.starts_with("#!/bin/bash"),
                "{} missing shebang",
                name
            );
        }
    }

    #[test]
    fn install_creates_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join("hooks");

        let result = install_hooks_to(&hooks_dir).unwrap();
        assert_eq!(result.installed.len(), 3);
        assert!(result.skipped.is_empty());

        for (name, _) in AGENT_HOOK_SCRIPTS {
            let path = hooks_dir.join(name);
            assert!(path.exists(), "{} not created", name);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&path).unwrap().permissions().mode();
                assert!(mode & 0o111 != 0, "{} not executable", name);
            }
        }
    }

    #[test]
    fn install_updates_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join("hooks");

        install_hooks_to(&hooks_dir).unwrap();

        std::fs::write(hooks_dir.join("PostToolUse"), "old content").unwrap();

        let result = install_hooks_to(&hooks_dir).unwrap();
        assert_eq!(result.skipped.len(), 3);
        assert!(result.installed.is_empty());

        let content = std::fs::read_to_string(hooks_dir.join("PostToolUse")).unwrap();
        assert!(content.starts_with("#!/bin/bash"));
    }
}
