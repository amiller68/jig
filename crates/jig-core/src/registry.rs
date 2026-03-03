//! Repo registry — global tracking of jig-managed repositories
//!
//! Stores a list of repos at `~/.config/jig/repos.json` so that future
//! global commands can iterate across all tracked projects.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::Result;

const REGISTRY_FILE: &str = "repos.json";

/// A single registered repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub path: PathBuf,
    pub added: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
}

/// Global registry of jig-managed repositories
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoRegistry {
    pub repos: Vec<RepoEntry>,
}

impl RepoRegistry {
    /// Load registry from disk, returning empty registry if file doesn't exist
    pub fn load() -> Result<Self> {
        let path = Self::registry_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)?;
        let registry: Self = serde_json::from_str(&content)?;
        Ok(registry)
    }

    /// Save registry to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::registry_path()?;
        fs::create_dir_all(path.parent().unwrap())?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Register a repo path. Returns true if it was newly added.
    pub fn register(&mut self, path: PathBuf) -> bool {
        let canonical = fs::canonicalize(&path).unwrap_or(path);
        if self.find(&canonical).is_some() {
            self.touch(&canonical);
            return false;
        }
        let now = Utc::now();
        self.repos.push(RepoEntry {
            path: canonical,
            added: now,
            last_used: now,
        });
        true
    }

    /// Remove a repo by path. Returns true if it was found and removed.
    pub fn remove(&mut self, path: &Path) -> bool {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let len_before = self.repos.len();
        self.repos.retain(|e| e.path != canonical);
        self.repos.len() < len_before
    }

    /// Remove entries whose paths no longer exist on disk. Returns removed paths.
    pub fn prune(&mut self) -> Vec<PathBuf> {
        let mut removed = Vec::new();
        self.repos.retain(|e| {
            if e.path.exists() {
                true
            } else {
                removed.push(e.path.clone());
                false
            }
        });
        removed
    }

    /// Update last_used timestamp for a repo
    pub fn touch(&mut self, path: &Path) {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(entry) = self.find_mut(&canonical) {
            entry.last_used = Utc::now();
        }
    }

    /// Get all registered repos
    pub fn repos(&self) -> &[RepoEntry] {
        &self.repos
    }

    fn find(&self, path: &Path) -> Option<&RepoEntry> {
        self.repos.iter().find(|e| e.path == path)
    }

    fn find_mut(&mut self, path: &Path) -> Option<&mut RepoEntry> {
        self.repos.iter_mut().find(|e| e.path == path)
    }

    fn registry_path() -> Result<PathBuf> {
        Ok(Config::config_dir()?.join(REGISTRY_FILE))
    }
}
