//! Hook registry for tracking installed git hooks
//!
//! Persists to `.git/jig-hooks.json` and tracks which hooks
//! are managed by jig, when they were installed, and backup
//! locations for any pre-existing user hooks.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Registry file name stored inside `.git/`
const REGISTRY_FILENAME: &str = "jig-hooks.json";

/// Registry tracking all jig-managed git hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRegistry {
    /// Schema version for future migrations
    pub version: String,
    /// Map of hook name to installation metadata
    pub installed: HashMap<String, HookEntry>,
}

/// Metadata for a single installed hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    /// ISO 8601 timestamp of installation
    pub installed_at: String,
    /// Version of jig that installed the hook
    pub jig_version: String,
    /// Whether a user hook existed before installation
    pub had_existing: bool,
    /// Path to the backed-up user hook, if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backed_up_to: Option<String>,
}

impl HookRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            version: "1".to_string(),
            installed: HashMap::new(),
        }
    }

    /// Load registry from a repository's `.git/` directory.
    /// Returns a new empty registry if the file doesn't exist.
    pub fn load(repo_path: &Path) -> Result<Self> {
        let path = repo_path.join(".git").join(REGISTRY_FILENAME);

        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(&path)?;
        let registry = serde_json::from_str(&content)?;
        Ok(registry)
    }

    /// Save registry to the repository's `.git/` directory as pretty-printed JSON.
    pub fn save(&self, repo_path: &Path) -> Result<()> {
        let path = repo_path.join(".git").join(REGISTRY_FILENAME);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Record that a hook was installed by jig.
    pub fn mark_installed(&mut self, hook_name: &str) {
        let entry = HookEntry {
            installed_at: chrono::Utc::now().to_rfc3339(),
            jig_version: env!("CARGO_PKG_VERSION").to_string(),
            had_existing: false,
            backed_up_to: None,
        };
        self.installed.insert(hook_name.to_string(), entry);
    }

    /// Record that an existing user hook was backed up before installation.
    pub fn mark_existing_backed_up(&mut self, hook_name: &str, backup_path: &str) {
        if let Some(entry) = self.installed.get_mut(hook_name) {
            entry.had_existing = true;
            entry.backed_up_to = Some(backup_path.to_string());
        }
    }

    /// Check whether a hook is managed by jig.
    pub fn is_installed(&self, hook_name: &str) -> bool {
        self.installed.contains_key(hook_name)
    }

    /// Remove a hook from the registry, returning its metadata if it existed.
    pub fn remove(&mut self, hook_name: &str) -> Option<HookEntry> {
        self.installed.remove(hook_name)
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = HookRegistry::new();
        assert_eq!(registry.version, "1");
        assert!(registry.installed.is_empty());
    }

    #[test]
    fn test_mark_installed() {
        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");
        assert!(registry.is_installed("post-commit"));
        assert!(!registry.is_installed("post-merge"));

        let entry = &registry.installed["post-commit"];
        assert!(!entry.had_existing);
        assert!(entry.backed_up_to.is_none());
    }

    #[test]
    fn test_mark_existing_backed_up() {
        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");
        registry.mark_existing_backed_up("post-commit", ".git/hooks/post-commit.backup");

        let entry = &registry.installed["post-commit"];
        assert!(entry.had_existing);
        assert_eq!(
            entry.backed_up_to.as_deref(),
            Some(".git/hooks/post-commit.backup")
        );
    }

    #[test]
    fn test_mark_existing_backed_up_noop_on_missing() {
        let mut registry = HookRegistry::new();
        // Should not panic when hook doesn't exist
        registry.mark_existing_backed_up("post-commit", ".git/hooks/post-commit.backup");
        assert!(!registry.is_installed("post-commit"));
    }

    #[test]
    fn test_remove() {
        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");

        let entry = registry.remove("post-commit");
        assert!(entry.is_some());
        assert!(!registry.is_installed("post-commit"));

        // Removing again returns None
        assert!(registry.remove("post-commit").is_none());
    }

    #[test]
    fn test_save_load_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        // Create .git directory to mimic a repo
        std::fs::create_dir(temp.path().join(".git")).unwrap();

        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");
        registry.mark_existing_backed_up("post-commit", ".git/hooks/post-commit.backup");
        registry.mark_installed("post-merge");

        registry.save(temp.path()).unwrap();
        let loaded = HookRegistry::load(temp.path()).unwrap();

        assert_eq!(loaded.version, "1");
        assert!(loaded.is_installed("post-commit"));
        assert!(loaded.is_installed("post-merge"));

        let entry = &loaded.installed["post-commit"];
        assert!(entry.had_existing);
        assert_eq!(
            entry.backed_up_to.as_deref(),
            Some(".git/hooks/post-commit.backup")
        );
    }

    #[test]
    fn test_load_missing_file_returns_new() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join(".git")).unwrap();

        let registry = HookRegistry::load(temp.path()).unwrap();
        assert_eq!(registry.version, "1");
        assert!(registry.installed.is_empty());
    }

    #[test]
    fn test_save_produces_pretty_json() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join(".git")).unwrap();

        let mut registry = HookRegistry::new();
        registry.mark_installed("post-commit");
        registry.save(temp.path()).unwrap();

        let content =
            std::fs::read_to_string(temp.path().join(".git").join(REGISTRY_FILENAME)).unwrap();
        // Pretty-printed JSON has newlines and indentation
        assert!(content.contains('\n'));
        assert!(content.contains("  "));
    }
}
