//! Triage tracker — tracks in-flight triage workers to prevent duplicate spawns
//! and detect stuck workers.

use std::collections::HashMap;

/// Tracks issues currently being triaged by lightweight read-only agents.
pub struct TriageTracker {
    /// Issues currently being triaged, keyed by Linear issue ID (e.g. "JIG-38").
    active: HashMap<String, TriageEntry>,
}

/// A single in-flight triage operation.
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

    /// Get all active entries (for per-repo timeout checking).
    pub fn stuck_entries(&self) -> Vec<&TriageEntry> {
        self.active.values().collect()
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
}
