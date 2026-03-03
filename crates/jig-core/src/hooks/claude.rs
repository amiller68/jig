//! Claude Code hook template installation.

use std::path::{Path, PathBuf};

use crate::error::Result;

/// Hook templates: (filename, script content).
pub const CLAUDE_HOOK_TEMPLATES: &[(&str, &str)] = &[
    ("PostToolUse", include_str!("templates/PostToolUse.sh")),
    ("Notification", include_str!("templates/Notification.sh")),
    ("Stop", include_str!("templates/Stop.sh")),
];

/// Return the Claude hooks directory (`~/.claude/hooks/`).
fn claude_hooks_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| crate::Error::Custom("no home directory".into()))?;
    Ok(home.join(".claude").join("hooks"))
}

/// Return the Claude settings file (`~/.claude/settings.json`).
fn claude_settings_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| crate::Error::Custom("no home directory".into()))?;
    Ok(home.join(".claude").join("settings.json"))
}

/// Result of installing Claude hooks.
#[derive(Debug, Default)]
pub struct InstallResult {
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
    pub settings_updated: bool,
}

/// Install Claude Code hook scripts to `~/.claude/hooks/` and register
/// them in `~/.claude/settings.json`.
///
/// Existing hook scripts are not overwritten (returned in `skipped`).
pub fn install_claude_hooks() -> Result<InstallResult> {
    let hooks_dir = claude_hooks_dir()?;
    let mut result = install_claude_hooks_to(&hooks_dir)?;
    result.settings_updated = register_hooks_in_settings(&hooks_dir)?;
    Ok(result)
}

/// Install Claude Code hook scripts to the given directory.
pub fn install_claude_hooks_to(hooks_dir: &Path) -> Result<InstallResult> {
    std::fs::create_dir_all(hooks_dir)?;

    let mut result = InstallResult::default();

    for (name, content) in CLAUDE_HOOK_TEMPLATES {
        let path = hooks_dir.join(name);
        if path.exists() {
            // Always update content to latest template
            std::fs::write(&path, content)?;
            make_executable(&path)?;
            result.skipped.push(name.to_string());
        } else {
            std::fs::write(&path, content)?;
            make_executable(&path)?;
            result.installed.push(name.to_string());
        }
    }

    Ok(result)
}

/// Register jig hooks in `~/.claude/settings.json` so Claude Code invokes them.
///
/// Merges into existing settings without clobbering other hook entries.
/// Returns true if settings were modified.
fn register_hooks_in_settings(hooks_dir: &Path) -> Result<bool> {
    let settings_path = claude_settings_path()?;

    // Read existing settings or start with empty object
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

    // For each hook type, ensure our jig entry exists
    for (event_name, _) in CLAUDE_HOOK_TEMPLATES {
        let script_path = hooks_dir.join(event_name);
        let script_path_str = script_path.to_string_lossy().to_string();

        let event_hooks = hooks_map
            .entry(*event_name)
            .or_insert_with(|| serde_json::json!([]));

        let entries = match event_hooks.as_array_mut() {
            Some(arr) => arr,
            None => continue,
        };

        // Check if a jig hook entry already exists
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
        // Ensure parent dir exists
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
        for (name, content) in CLAUDE_HOOK_TEMPLATES {
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

        let result = install_claude_hooks_to(&hooks_dir).unwrap();
        assert_eq!(result.installed.len(), 3);
        assert!(result.skipped.is_empty());

        for (name, _) in CLAUDE_HOOK_TEMPLATES {
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

        install_claude_hooks_to(&hooks_dir).unwrap();

        // Overwrite one with stale content
        std::fs::write(hooks_dir.join("PostToolUse"), "old content").unwrap();

        let result = install_claude_hooks_to(&hooks_dir).unwrap();
        // All should be in skipped (existing) but content updated
        assert_eq!(result.skipped.len(), 3);
        assert!(result.installed.is_empty());

        // Content should be updated
        let content = std::fs::read_to_string(hooks_dir.join("PostToolUse")).unwrap();
        assert!(content.starts_with("#!/bin/bash"));
    }
}
