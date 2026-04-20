//! Triage tracker — tracks in-flight triage workers to prevent duplicate spawns
//! and detect stuck workers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Tracks issues currently being triaged by lightweight read-only agents.
pub struct TriageTracker {
    /// Issues currently being triaged, keyed by Linear issue ID (e.g. "JIG-38").
    active: HashMap<String, TriageEntry>,
}

/// A single in-flight triage operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageEntry {
    /// Worker name handling this triage (e.g. "triage-jig-38-add-statuses").
    pub worker_name: String,
    /// Unix timestamp when the triage worker was spawned.
    pub spawned_at: i64,
    /// Linear issue identifier.
    pub issue_id: String,
    /// Repo name this triage belongs to.
    pub repo_name: String,
}

/// Serializable wrapper for persisting tracker state.
#[derive(Serialize, Deserialize)]
struct PersistedTriages {
    entries: Vec<TriageEntry>,
}

impl TriageTracker {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    /// Register a new in-flight triage. Returns false if already active.
    pub fn register(&mut self, issue_id: String, entry: TriageEntry) -> bool {
        if self.active.contains_key(&issue_id) {
            return false;
        }
        self.active.insert(issue_id, entry);
        true
    }

    /// Check if an issue is currently being triaged.
    pub fn is_active(&self, issue_id: &str) -> bool {
        self.active.contains_key(issue_id)
    }

    /// Remove a completed/failed triage.
    pub fn remove(&mut self, issue_id: &str) -> Option<TriageEntry> {
        self.active.remove(issue_id)
    }

    /// Find triages that have exceeded the timeout.
    pub fn stuck_triages(&self, timeout_seconds: i64, now: i64) -> Vec<&TriageEntry> {
        self.active
            .values()
            .filter(|e| now - e.spawned_at > timeout_seconds)
            .collect()
    }

    /// Get all active entries.
    pub fn active_entries(&self) -> Vec<&TriageEntry> {
        self.active.values().collect()
    }

    /// Default persistence path: `~/.config/jig/state/triages.json`.
    fn default_path() -> Option<PathBuf> {
        crate::global::global_state_dir()
            .ok()
            .map(|d| d.join("triages.json"))
    }

    /// Persist current state to disk.
    pub fn persist(&self) -> std::result::Result<(), String> {
        let path = Self::default_path().ok_or("cannot resolve state dir")?;
        self.persist_to(&path)
    }

    /// Persist to a specific path.
    pub fn persist_to(&self, path: &Path) -> std::result::Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = PersistedTriages {
            entries: self.active.values().cloned().collect(),
        };
        let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Load from default path. Returns empty tracker if file is missing.
    pub fn load() -> Self {
        Self::default_path()
            .map(|p| Self::load_from(&p))
            .unwrap_or_default()
    }

    /// Load from a specific path. Returns empty tracker if file is missing or invalid.
    pub fn load_from(path: &Path) -> Self {
        let data = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return Self::new(),
        };
        let persisted: PersistedTriages = match serde_json::from_str(&data) {
            Ok(p) => p,
            Err(_) => return Self::new(),
        };
        let mut tracker = Self::new();
        for entry in persisted.entries {
            tracker.active.insert(entry.issue_id.clone(), entry);
        }
        tracker
    }

    /// Rebuild tracker from active workers whose names start with "triage-".
    pub fn rebuild_from_workers(workers: &[(String, String)], now: i64) -> Self {
        let mut tracker = Self::new();
        for (repo_name, worker_name) in workers {
            if let Some(issue_id) = worker_name.strip_prefix("triage-") {
                let issue_id = issue_id.to_uppercase();
                tracker.register(
                    issue_id.clone(),
                    TriageEntry {
                        worker_name: worker_name.clone(),
                        spawned_at: now,
                        issue_id: issue_id.clone(),
                        repo_name: repo_name.clone(),
                    },
                );
            }
        }
        tracker
    }
}

impl Default for TriageTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(worker: &str, issue: &str, repo: &str, spawned_at: i64) -> TriageEntry {
        TriageEntry {
            worker_name: worker.to_string(),
            spawned_at,
            issue_id: issue.to_string(),
            repo_name: repo.to_string(),
        }
    }

    #[test]
    fn register_and_is_active() {
        let mut tracker = TriageTracker::new();
        let entry = make_entry("triage-jig-38", "JIG-38", "my-repo", 1000);
        assert!(tracker.register("JIG-38".to_string(), entry));
        assert!(tracker.is_active("JIG-38"));
        assert!(!tracker.is_active("JIG-99"));
    }

    #[test]
    fn register_returns_false_for_duplicate() {
        let mut tracker = TriageTracker::new();
        let entry1 = make_entry("triage-jig-38", "JIG-38", "my-repo", 1000);
        let entry2 = make_entry("triage-jig-38-v2", "JIG-38", "my-repo", 2000);
        assert!(tracker.register("JIG-38".to_string(), entry1));
        assert!(!tracker.register("JIG-38".to_string(), entry2));
    }

    #[test]
    fn remove_returns_entry() {
        let mut tracker = TriageTracker::new();
        let entry = make_entry("triage-jig-38", "JIG-38", "my-repo", 1000);
        tracker.register("JIG-38".to_string(), entry);
        let removed = tracker.remove("JIG-38");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().worker_name, "triage-jig-38");
        assert!(!tracker.is_active("JIG-38"));
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut tracker = TriageTracker::new();
        assert!(tracker.remove("JIG-99").is_none());
    }

    #[test]
    fn stuck_triages_filters_by_timeout() {
        let mut tracker = TriageTracker::new();
        let entry1 = make_entry("triage-jig-1", "JIG-1", "repo", 100);
        let entry2 = make_entry("triage-jig-2", "JIG-2", "repo", 500);
        let entry3 = make_entry("triage-jig-3", "JIG-3", "repo", 900);
        tracker.register("JIG-1".to_string(), entry1);
        tracker.register("JIG-2".to_string(), entry2);
        tracker.register("JIG-3".to_string(), entry3);

        // At now=1000, timeout=600: JIG-1 (age=900) is stuck, JIG-2 (age=500) is not,
        // JIG-3 (age=100) is not
        let stuck = tracker.stuck_triages(600, 1000);
        assert_eq!(stuck.len(), 1);
        assert_eq!(stuck[0].issue_id, "JIG-1");
    }

    #[test]
    fn stuck_triages_empty_when_none_stuck() {
        let mut tracker = TriageTracker::new();
        let entry = make_entry("triage-jig-1", "JIG-1", "repo", 900);
        tracker.register("JIG-1".to_string(), entry);

        let stuck = tracker.stuck_triages(600, 1000);
        assert!(stuck.is_empty());
    }

    #[test]
    fn default_creates_empty_tracker() {
        let tracker = TriageTracker::default();
        assert!(!tracker.is_active("anything"));
    }

    #[test]
    fn persist_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("triages.json");

        let mut tracker = TriageTracker::new();
        tracker.register(
            "JIG-77".to_string(),
            make_entry("triage-jig-77", "JIG-77", "my-repo", 1000),
        );
        tracker.register(
            "JIG-81".to_string(),
            make_entry("triage-jig-81", "JIG-81", "other-repo", 2000),
        );
        tracker.persist_to(&path).unwrap();

        let loaded = TriageTracker::load_from(&path);
        assert!(loaded.is_active("JIG-77"));
        assert!(loaded.is_active("JIG-81"));
        assert!(!loaded.is_active("JIG-99"));
        assert_eq!(loaded.active_entries().len(), 2);
    }

    #[test]
    fn load_from_missing_file_returns_empty() {
        let tracker = TriageTracker::load_from(Path::new("/nonexistent/triages.json"));
        assert_eq!(tracker.active_entries().len(), 0);
    }

    #[test]
    fn load_from_invalid_json_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("triages.json");
        std::fs::write(&path, "not valid json").unwrap();
        let tracker = TriageTracker::load_from(&path);
        assert_eq!(tracker.active_entries().len(), 0);
    }

    #[test]
    fn active_entries_returns_all() {
        let mut tracker = TriageTracker::new();
        tracker.register(
            "JIG-1".to_string(),
            make_entry("triage-jig-1", "JIG-1", "repo", 100),
        );
        tracker.register(
            "JIG-2".to_string(),
            make_entry("triage-jig-2", "JIG-2", "repo", 200),
        );
        assert_eq!(tracker.active_entries().len(), 2);
    }

    #[test]
    fn rebuild_from_workers_populates_tracker() {
        let workers = vec![
            ("repo".to_string(), "triage-jig-38".to_string()),
            ("repo".to_string(), "al/jig-39-normal-worker".to_string()),
            ("repo".to_string(), "triage-eng-100".to_string()),
        ];
        let tracker = TriageTracker::rebuild_from_workers(&workers, 5000);
        assert!(tracker.is_active("JIG-38"));
        assert!(tracker.is_active("ENG-100"));
        assert!(!tracker.is_active("JIG-39"));
    }
}
