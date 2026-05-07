//! Hook registry — tracks which git hooks jig has installed.
//!
//! Stored at `.jig/jig-hooks.json` (gitignored, per-machine state).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::context::hook_registry_path;
use jig_core::error::Result;

/// Tracks installed git hooks for idempotent init and safe uninstall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRegistry {
    pub version: String,
    pub installed: HashMap<String, HookEntry>,
}

/// Metadata about a single installed hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    pub installed_at: String,
    pub jig_version: String,
    pub had_existing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backed_up_to: Option<String>,
}

impl HookRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            version: "1".to_string(),
            installed: HashMap::new(),
        }
    }

    /// Load registry from `<repo_path>/.jig/hooks/hooks.json`.
    ///
    /// Falls back to legacy `<repo_path>/jig-hooks.json` if the new path
    /// doesn't exist, to support migration from older installs.
    /// Returns a fresh registry if neither file exists.
    pub fn load(repo_path: &Path) -> Result<Self> {
        let path = registry_path(repo_path);
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let registry = serde_json::from_str(&content)?;
            return Ok(registry);
        }

        // Try legacy location (<repo_root>/jig-hooks.json)
        let legacy_path = repo_path.join("jig-hooks.json");
        if legacy_path.exists() {
            let content = std::fs::read_to_string(&legacy_path)?;
            let registry: Self = serde_json::from_str(&content)?;
            // Migrate: save to new location and remove legacy file
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let new_content = serde_json::to_string_pretty(&registry)?;
            std::fs::write(&path, new_content)?;
            let _ = std::fs::remove_file(&legacy_path);
            tracing::debug!("migrated jig-hooks.json to .jig/hooks/hooks.json");
            return Ok(registry);
        }

        Ok(Self::new())
    }

    /// Save registry to `<repo_path>/.jig/hooks/hooks.json`.
    pub fn save(&self, repo_path: &Path) -> Result<()> {
        let path = registry_path(repo_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Record that a hook was installed.
    pub fn mark_installed(&mut self, hook_name: &str) {
        let entry = HookEntry {
            installed_at: chrono::Utc::now().to_rfc3339(),
            jig_version: env!("CARGO_PKG_VERSION").to_string(),
            had_existing: false,
            backed_up_to: None,
        };
        self.installed.insert(hook_name.to_string(), entry);
    }

    /// Record that a user hook was backed up before installation.
    pub fn mark_existing_backed_up(&mut self, hook_name: &str, backup_name: &str) {
        if let Some(entry) = self.installed.get_mut(hook_name) {
            entry.had_existing = true;
            entry.backed_up_to = Some(backup_name.to_string());
        }
    }

    /// Check if a hook is currently managed by jig.
    pub fn is_installed(&self, hook_name: &str) -> bool {
        self.installed.contains_key(hook_name)
    }

    /// Remove a hook from the registry. Returns the entry if it existed.
    pub fn remove(&mut self, hook_name: &str) -> Option<HookEntry> {
        self.installed.remove(hook_name)
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Path to the hook registry file for a given repo.
pub fn registry_path(repo_path: &Path) -> PathBuf {
    hook_registry_path(repo_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let registry = HookRegistry::new();
        assert_eq!(registry.version, "1");
        assert!(registry.installed.is_empty());
    }

    #[test]
    fn mark_installed_and_check() {
        let mut registry = HookRegistry::new();
        assert!(!registry.is_installed("post-commit"));

        registry.mark_installed("post-commit");
        assert!(registry.is_installed("post-commit"));
        assert!(!registry.is_installed("pre-commit"));
    }

    #[test]
    fn mark_existing_backed_up() {
        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");
        registry.mark_existing_backed_up("post-commit", "post-commit.backup-2026-02-24");

        let entry = &registry.installed["post-commit"];
        assert!(entry.had_existing);
        assert_eq!(
            entry.backed_up_to.as_deref(),
            Some("post-commit.backup-2026-02-24")
        );
    }

    #[test]
    fn remove_returns_entry() {
        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");

        let entry = registry.remove("post-commit");
        assert!(entry.is_some());
        assert!(!registry.is_installed("post-commit"));

        assert!(registry.remove("post-commit").is_none());
    }

    #[test]
    fn save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");
        registry.mark_installed("pre-commit");
        registry.mark_existing_backed_up("post-commit", "post-commit.backup");

        registry.save(tmp.path()).unwrap();

        let loaded = HookRegistry::load(tmp.path()).unwrap();
        assert_eq!(loaded.version, "1");
        assert!(loaded.is_installed("post-commit"));
        assert!(loaded.is_installed("pre-commit"));
        assert!(loaded.installed["post-commit"].had_existing);
    }

    #[test]
    fn load_missing_file_returns_new() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = HookRegistry::load(tmp.path()).unwrap();
        assert_eq!(loaded.version, "1");
        assert!(loaded.installed.is_empty());
    }

    #[test]
    fn save_is_pretty_printed() {
        let tmp = tempfile::tempdir().unwrap();
        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");
        registry.save(tmp.path()).unwrap();

        let content = std::fs::read_to_string(registry_path(tmp.path())).unwrap();
        assert!(content.contains('\n'));
        assert!(content.contains("  ")); // indented
    }
}
