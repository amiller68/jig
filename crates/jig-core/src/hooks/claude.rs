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

/// Result of installing Claude hooks.
#[derive(Debug, Default)]
pub struct InstallResult {
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
}

/// Install Claude Code hook scripts to `~/.claude/hooks/`.
///
/// Existing hooks are not overwritten (returned in `skipped`).
pub fn install_claude_hooks() -> Result<InstallResult> {
    install_claude_hooks_to(&claude_hooks_dir()?)
}

/// Install Claude Code hook scripts to the given directory.
pub fn install_claude_hooks_to(hooks_dir: &Path) -> Result<InstallResult> {
    std::fs::create_dir_all(hooks_dir)?;

    let mut result = InstallResult::default();

    for (name, content) in CLAUDE_HOOK_TEMPLATES {
        let path = hooks_dir.join(name);
        if path.exists() {
            result.skipped.push(name.to_string());
        } else {
            std::fs::write(&path, content)?;
            make_executable(&path)?;
            result.installed.push(name.to_string());
        }
    }

    Ok(result)
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
    fn install_skips_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join("hooks");

        install_claude_hooks_to(&hooks_dir).unwrap();

        let result = install_claude_hooks_to(&hooks_dir).unwrap();
        assert!(result.installed.is_empty());
        assert_eq!(result.skipped.len(), 3);
    }
}
