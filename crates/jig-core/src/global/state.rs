//! Aggregated worker state (JSON)
//!
//! Stored at `~/.config/jig/state/workers.json`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::paths::global_state_dir;

/// A single worker entry in the global state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEntry {
    pub repo: String,
    pub branch: String,
    pub status: String,
    pub issue: Option<String>,
    pub pr_url: Option<String>,
    pub started_at: i64,
    pub last_event_at: i64,
    #[serde(default)]
    pub nudge_counts: HashMap<String, u32>,
}

/// Aggregated worker state across all repos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkersState {
    pub version: String,
    pub workers: HashMap<String, WorkerEntry>,
}

impl Default for WorkersState {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            workers: HashMap::new(),
        }
    }
}

impl WorkersState {
    /// Load from the default path. Returns empty state if missing.
    pub fn load() -> Result<Self> {
        let path = global_state_dir()?.join("workers.json");
        Self::load_from(&path)
    }

    /// Load from a specific path. Returns empty state if missing.
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)?;
        let state: WorkersState = serde_json::from_str(&content)?;
        Ok(state)
    }

    /// Save to the default path.
    pub fn save(&self) -> Result<()> {
        let path = global_state_dir()?.join("workers.json");
        self.save_to(&path)
    }

    /// Save to a specific path, creating parent directories.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Insert or update a worker entry.
    pub fn set_worker(&mut self, key: &str, entry: WorkerEntry) {
        self.workers.insert(key.to_string(), entry);
    }

    /// Get a worker entry by key.
    pub fn get_worker(&self, key: &str) -> Option<&WorkerEntry> {
        self.workers.get(key)
    }

    /// Remove a worker entry by key.
    pub fn remove_worker(&mut self, key: &str) -> Option<WorkerEntry> {
        self.workers.remove(key)
    }

    /// Get all workers for a given repo name.
    pub fn workers_for_repo(&self, repo: &str) -> Vec<(&String, &WorkerEntry)> {
        self.workers
            .iter()
            .filter(|(_, entry)| entry.repo == repo)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(repo: &str, branch: &str) -> WorkerEntry {
        WorkerEntry {
            repo: repo.to_string(),
            branch: branch.to_string(),
            status: "running".to_string(),
            issue: None,
            pr_url: None,
            started_at: 1000,
            last_event_at: 2000,
            nudge_counts: HashMap::new(),
        }
    }

    #[test]
    fn new_state_is_empty() {
        let state = WorkersState::default();
        assert_eq!(state.version, "1");
        assert!(state.workers.is_empty());
    }

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("workers.json");

        let mut state = WorkersState::default();
        state.set_worker("myrepo/feat", make_entry("myrepo", "feat"));

        state.save_to(&path).unwrap();
        let loaded = WorkersState::load_from(&path).unwrap();

        assert_eq!(loaded.workers.len(), 1);
        let entry = loaded.get_worker("myrepo/feat").unwrap();
        assert_eq!(entry.repo, "myrepo");
        assert_eq!(entry.branch, "feat");
    }

    #[test]
    fn missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let state = WorkersState::load_from(&path).unwrap();
        assert!(state.workers.is_empty());
    }

    #[test]
    fn set_and_remove() {
        let mut state = WorkersState::default();
        state.set_worker("r/b", make_entry("r", "b"));
        assert!(state.get_worker("r/b").is_some());

        let removed = state.remove_worker("r/b");
        assert!(removed.is_some());
        assert!(state.get_worker("r/b").is_none());
    }

    #[test]
    fn workers_for_repo_filters() {
        let mut state = WorkersState::default();
        state.set_worker("alpha/main", make_entry("alpha", "main"));
        state.set_worker("alpha/feat", make_entry("alpha", "feat"));
        state.set_worker("beta/main", make_entry("beta", "main"));

        let alpha = state.workers_for_repo("alpha");
        assert_eq!(alpha.len(), 2);

        let beta = state.workers_for_repo("beta");
        assert_eq!(beta.len(), 1);

        let gamma = state.workers_for_repo("gamma");
        assert!(gamma.is_empty());
    }
}
