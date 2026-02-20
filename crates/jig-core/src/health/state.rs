//! Health state persistence
//!
//! Tracks worker health metrics and nudge counts in `.jig/.health/state.json`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;

/// File name for health state
pub const HEALTH_DIR: &str = ".health";
/// Health state file name
pub const HEALTH_FILE: &str = "state.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthState {
    pub version: String,
    pub max_nudges: u32,
    pub workers: HashMap<String, WorkerHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHealth {
    pub started_at: i64,
    pub last_commit_at: i64,
    pub commit_count: u32,
    pub last_file_mod_at: i64,
    #[serde(default)]
    pub nudges: HashMap<String, u32>,
}

impl Default for HealthState {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            max_nudges: 3,
            workers: HashMap::new(),
        }
    }
}

impl HealthState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load health state from a JSON file. Returns a default state if the file doesn't exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save health state to a JSON file with pretty-printing.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Add a worker with the given name and start time.
    pub fn add_worker(&mut self, name: &str, started_at: i64) {
        let health = WorkerHealth {
            started_at,
            last_commit_at: 0,
            commit_count: 0,
            last_file_mod_at: 0,
            nudges: HashMap::new(),
        };
        self.workers.insert(name.to_string(), health);
    }

    /// Remove a worker by name, returning its health data if it existed.
    pub fn remove_worker(&mut self, name: &str) -> Option<WorkerHealth> {
        self.workers.remove(name)
    }
}

impl WorkerHealth {
    /// Increment the nudge count for the given type.
    pub fn increment_nudge(&mut self, nudge_type: &str) {
        *self.nudges.entry(nudge_type.to_string()).or_insert(0) += 1;
    }

    /// Reset the nudge count for the given type.
    pub fn reset_nudge(&mut self, nudge_type: &str) {
        self.nudges.remove(nudge_type);
    }

    /// Get the nudge count for the given type.
    pub fn get_nudge_count(&self, nudge_type: &str) -> u32 {
        self.nudges.get(nudge_type).copied().unwrap_or(0)
    }

    /// Calculate the worker's age in hours since `started_at`.
    pub fn age_hours(&self) -> u64 {
        let now = chrono::Utc::now().timestamp();
        ((now - self.started_at).max(0) / 3600) as u64
    }

    /// Calculate hours since the last commit. Falls back to worker age if no commits.
    pub fn hours_since_commit(&self) -> u64 {
        if self.last_commit_at == 0 {
            return self.age_hours();
        }
        let now = chrono::Utc::now().timestamp();
        ((now - self.last_commit_at).max(0) / 3600) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_state_new() {
        let state = HealthState::new();
        assert_eq!(state.version, "1");
        assert_eq!(state.max_nudges, 3);
        assert!(state.workers.is_empty());
    }

    #[test]
    fn test_add_remove_worker() {
        let mut state = HealthState::new();
        state.add_worker("test", 1000);
        assert!(state.workers.contains_key("test"));

        let removed = state.remove_worker("test");
        assert!(removed.is_some());
        assert!(!state.workers.contains_key("test"));
    }

    #[test]
    fn test_remove_nonexistent_worker() {
        let mut state = HealthState::new();
        assert!(state.remove_worker("nope").is_none());
    }

    #[test]
    fn test_nudge_counts() {
        let mut state = HealthState::new();
        state.add_worker("test", 1000);
        let worker = state.workers.get_mut("test").unwrap();

        assert_eq!(worker.get_nudge_count("idle"), 0);

        worker.increment_nudge("idle");
        assert_eq!(worker.get_nudge_count("idle"), 1);

        worker.increment_nudge("idle");
        assert_eq!(worker.get_nudge_count("idle"), 2);

        worker.reset_nudge("idle");
        assert_eq!(worker.get_nudge_count("idle"), 0);
    }

    #[test]
    fn test_multiple_nudge_types() {
        let mut health = WorkerHealth {
            started_at: 1000,
            last_commit_at: 0,
            commit_count: 0,
            last_file_mod_at: 0,
            nudges: HashMap::new(),
        };

        health.increment_nudge("idle");
        health.increment_nudge("ci_failure");
        health.increment_nudge("idle");

        assert_eq!(health.get_nudge_count("idle"), 2);
        assert_eq!(health.get_nudge_count("ci_failure"), 1);
        assert_eq!(health.get_nudge_count("conflict"), 0);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".jig/.health/state.json");

        let mut state = HealthState::new();
        state.add_worker("test", 1000);
        state.workers.get_mut("test").unwrap().commit_count = 5;
        state
            .workers
            .get_mut("test")
            .unwrap()
            .increment_nudge("idle");
        state.save(&path).unwrap();

        let loaded = HealthState::load(&path).unwrap();
        assert_eq!(loaded.version, "1");
        assert_eq!(loaded.max_nudges, 3);
        assert!(loaded.workers.contains_key("test"));

        let worker = loaded.workers.get("test").unwrap();
        assert_eq!(worker.started_at, 1000);
        assert_eq!(worker.commit_count, 5);
        assert_eq!(worker.get_nudge_count("idle"), 1);
    }

    #[test]
    fn test_load_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nonexistent.json");

        let state = HealthState::load(&path).unwrap();
        assert_eq!(state.version, "1");
        assert!(state.workers.is_empty());
    }

    #[test]
    fn test_worker_age_and_commit_time() {
        let now = chrono::Utc::now().timestamp();
        let two_hours_ago = now - 7200;
        let one_hour_ago = now - 3600;

        let worker = WorkerHealth {
            started_at: two_hours_ago,
            last_commit_at: one_hour_ago,
            commit_count: 1,
            last_file_mod_at: 0,
            nudges: HashMap::new(),
        };

        assert_eq!(worker.age_hours(), 2);
        assert_eq!(worker.hours_since_commit(), 1);
    }

    #[test]
    fn test_hours_since_commit_no_commits() {
        let now = chrono::Utc::now().timestamp();
        let three_hours_ago = now - 10800;

        let worker = WorkerHealth {
            started_at: three_hours_ago,
            last_commit_at: 0,
            commit_count: 0,
            last_file_mod_at: 0,
            nudges: HashMap::new(),
        };

        // Falls back to age_hours when no commits
        assert_eq!(worker.hours_since_commit(), 3);
    }
}
