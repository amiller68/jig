//! Triage tracker — tracks in-flight triage subprocesses to prevent duplicate
//! spawns, detect stuck workers, and survive daemon restarts.
//!
//! State is persisted to `~/.config/jig/state/triages.json` so that restarting
//! the daemon can reconcile against live subprocesses (via a pid liveness
//! check) instead of scanning the worker filesystem.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::global::global_state_dir;

/// A single in-flight triage operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageEntry {
    /// Linear (or other provider) issue identifier, e.g. `JIG-38`.
    pub issue_id: String,
    /// Repo name this triage belongs to.
    pub repo_name: String,
    /// Unix timestamp when the triage subprocess was spawned.
    pub spawned_at: i64,
    /// PID of the triage subprocess.
    pub pid: u32,
    /// Path to the captured stdout/stderr log file.
    pub log_path: PathBuf,
    /// Path to the rendered triage prompt file.
    pub prompt_path: PathBuf,
}

/// On-disk format for the persisted tracker state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TriageState {
    #[serde(default)]
    active: Vec<TriageEntry>,
}

/// Tracks issues currently being triaged by lightweight read-only subprocesses.
pub struct TriageTracker {
    /// Issues currently being triaged, keyed by issue ID.
    active: HashMap<String, TriageEntry>,
    /// Path to the persistence file. Always set; defaults to the global state
    /// dir but can be overridden for tests.
    state_path: PathBuf,
}

impl TriageTracker {
    /// Create an empty in-memory tracker backed by the default state path.
    /// Primarily used by tests and as a fallback when persistence is disabled.
    pub fn new() -> Self {
        let state_path = default_state_path().unwrap_or_else(|_| PathBuf::from("triages.json"));
        Self {
            active: HashMap::new(),
            state_path,
        }
    }

    /// Load the tracker from the default state path, running reconciliation
    /// against live process ids. Returns an empty tracker if the file does
    /// not yet exist.
    pub fn load() -> Result<Self> {
        let path = default_state_path()?;
        Self::load_from(&path)
    }

    /// Load from a specific path and reconcile against live pids. Used in
    /// tests and by [`load`].
    pub fn load_from(path: &Path) -> Result<Self> {
        let mut tracker = Self {
            active: HashMap::new(),
            state_path: path.to_path_buf(),
        };

        if path.exists() {
            let content = fs::read_to_string(path)?;
            let state: TriageState = serde_json::from_str(&content)?;
            for entry in state.active {
                tracker.active.insert(entry.issue_id.clone(), entry);
            }
        }

        tracker.reconcile();
        Ok(tracker)
    }

    /// Atomically persist the current state to `state_path`.
    ///
    /// Writes to a sibling `<state_path>.tmp` file first, then renames into
    /// place so that a partial write cannot corrupt the stored state.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let state = TriageState {
            active: self.active.values().cloned().collect(),
        };
        let content = serde_json::to_string_pretty(&state)?;

        let tmp_path = {
            let mut name = self
                .state_path
                .file_name()
                .map(|n| n.to_os_string())
                .unwrap_or_default();
            name.push(".tmp");
            self.state_path.with_file_name(name)
        };

        fs::write(&tmp_path, content)?;
        fs::rename(&tmp_path, &self.state_path)?;
        Ok(())
    }

    /// Drop any entries whose pid no longer refers to a live process.
    ///
    /// Uses `kill(pid, 0)` — a no-op signal that returns `ESRCH` when the
    /// target process does not exist. Persistence is not touched here; the
    /// caller is expected to either ignore the in-memory-only drop or follow
    /// up with a [`save`](Self::save).
    pub fn reconcile(&mut self) {
        let before = self.active.len();
        self.active.retain(|_, entry| {
            if pid_is_live(entry.pid) {
                true
            } else {
                tracing::debug!(
                    issue = %entry.issue_id,
                    pid = entry.pid,
                    "reconcile: dropping triage entry — pid no longer alive"
                );
                false
            }
        });
        let dropped = before - self.active.len();
        if dropped > 0 {
            tracing::debug!(dropped, "triage tracker reconciled");
        }
    }

    /// Register a new in-flight triage. Returns `false` without persisting if
    /// the issue is already active.
    pub fn register(&mut self, issue_id: String, entry: TriageEntry) -> bool {
        if self.active.contains_key(&issue_id) {
            return false;
        }
        self.active.insert(issue_id, entry);
        if let Err(err) = self.save() {
            tracing::warn!(error = %err, "failed to persist triage tracker after register");
        }
        true
    }

    /// Check if an issue is currently being triaged.
    pub fn is_active(&self, issue_id: &str) -> bool {
        self.active.contains_key(issue_id)
    }

    /// Remove a completed/failed triage.
    pub fn remove(&mut self, issue_id: &str) -> Option<TriageEntry> {
        let removed = self.active.remove(issue_id);
        if removed.is_some() {
            if let Err(err) = self.save() {
                tracing::warn!(error = %err, "failed to persist triage tracker after remove");
            }
        }
        removed
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

/// Resolve the default on-disk state file path (`<state>/triages.json`).
fn default_state_path() -> Result<PathBuf> {
    Ok(global_state_dir()?.join("triages.json"))
}

/// Liveness check for a pid. Returns `true` if a process with `pid` exists
/// (regardless of whether we own it). On non-Unix builds this always returns
/// `true` — the tracker degrades gracefully rather than dropping state.
#[cfg(unix)]
fn pid_is_live(pid: u32) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;

    // Reject pids that `kill(2)` assigns special meaning to: 0 sends to the
    // caller's process group, and values that cast to negative i32 (e.g.
    // u32::MAX → -1) broadcast to process groups or all processes.
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }

    // Signal 0 performs error checking without delivering a signal.
    match kill(Pid::from_raw(pid as i32), None) {
        Ok(()) => true,
        // EPERM means the process exists but we don't own it.
        Err(Errno::EPERM) => true,
        // ESRCH means no such process.
        Err(Errno::ESRCH) => false,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn pid_is_live(_pid: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(issue: &str, repo: &str, spawned_at: i64, pid: u32) -> TriageEntry {
        TriageEntry {
            issue_id: issue.to_string(),
            repo_name: repo.to_string(),
            spawned_at,
            pid,
            log_path: PathBuf::from(format!("/tmp/triage-{}.log", issue)),
            prompt_path: PathBuf::from(format!("/tmp/triage-{}.prompt", issue)),
        }
    }

    /// Build a tracker whose state file lives in a tempdir. The tempdir handle
    /// is returned so tests can keep it alive for the duration of the test.
    fn tracker_in(dir: &tempfile::TempDir) -> TriageTracker {
        TriageTracker {
            active: HashMap::new(),
            state_path: dir.path().join("triages.json"),
        }
    }

    #[test]
    fn register_and_is_active() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tracker = tracker_in(&tmp);
        let entry = make_entry("JIG-38", "my-repo", 1000, std::process::id());
        assert!(tracker.register("JIG-38".to_string(), entry));
        assert!(tracker.is_active("JIG-38"));
        assert!(!tracker.is_active("JIG-99"));
    }

    #[test]
    fn register_returns_false_for_duplicate() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tracker = tracker_in(&tmp);
        let entry1 = make_entry("JIG-38", "my-repo", 1000, std::process::id());
        let entry2 = make_entry("JIG-38", "my-repo", 2000, std::process::id());
        assert!(tracker.register("JIG-38".to_string(), entry1));
        assert!(!tracker.register("JIG-38".to_string(), entry2));
    }

    #[test]
    fn remove_returns_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tracker = tracker_in(&tmp);
        let entry = make_entry("JIG-38", "my-repo", 1000, std::process::id());
        tracker.register("JIG-38".to_string(), entry);
        let removed = tracker.remove("JIG-38");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().issue_id, "JIG-38");
        assert!(!tracker.is_active("JIG-38"));
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tracker = tracker_in(&tmp);
        assert!(tracker.remove("JIG-99").is_none());
    }

    #[test]
    fn stuck_triages_filters_by_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tracker = tracker_in(&tmp);
        tracker.register(
            "JIG-1".to_string(),
            make_entry("JIG-1", "repo", 100, std::process::id()),
        );
        tracker.register(
            "JIG-2".to_string(),
            make_entry("JIG-2", "repo", 500, std::process::id()),
        );
        tracker.register(
            "JIG-3".to_string(),
            make_entry("JIG-3", "repo", 900, std::process::id()),
        );

        // At now=1000, timeout=600: JIG-1 (age=900) is stuck, JIG-2 (age=500)
        // is not, JIG-3 (age=100) is not.
        let stuck = tracker.stuck_triages(600, 1000);
        assert_eq!(stuck.len(), 1);
        assert_eq!(stuck[0].issue_id, "JIG-1");
    }

    #[test]
    fn stuck_triages_empty_when_none_stuck() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tracker = tracker_in(&tmp);
        tracker.register(
            "JIG-1".to_string(),
            make_entry("JIG-1", "repo", 900, std::process::id()),
        );

        let stuck = tracker.stuck_triages(600, 1000);
        assert!(stuck.is_empty());
    }

    #[test]
    fn default_creates_empty_tracker() {
        let tracker = TriageTracker::default();
        assert!(!tracker.is_active("anything"));
    }

    #[test]
    fn roundtrip_serde_preserves_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("triages.json");

        let mut tracker = TriageTracker {
            active: HashMap::new(),
            state_path: path.clone(),
        };
        tracker.active.insert(
            "JIG-38".to_string(),
            make_entry("JIG-38", "my-repo", 1234, std::process::id()),
        );
        tracker.save().unwrap();

        let loaded = TriageTracker::load_from(&path).unwrap();
        let entry = loaded
            .active
            .get("JIG-38")
            .expect("entry should survive round-trip");
        assert_eq!(entry.issue_id, "JIG-38");
        assert_eq!(entry.repo_name, "my-repo");
        assert_eq!(entry.spawned_at, 1234);
        assert_eq!(entry.pid, std::process::id());
        assert_eq!(entry.log_path, PathBuf::from("/tmp/triage-JIG-38.log"));
        assert_eq!(
            entry.prompt_path,
            PathBuf::from("/tmp/triage-JIG-38.prompt")
        );
    }

    #[test]
    fn reconcile_drops_dead_pids() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("triages.json");

        let mut tracker = TriageTracker {
            active: HashMap::new(),
            state_path: path.clone(),
        };
        // i32::MAX is safely outside any real pid on Linux/macOS. We avoid
        // u32::MAX because it casts to -1, which kill() treats as a
        // broadcast rather than a missing-pid check.
        tracker.active.insert(
            "JIG-99".to_string(),
            make_entry("JIG-99", "repo", 100, i32::MAX as u32),
        );
        tracker.save().unwrap();

        let loaded = TriageTracker::load_from(&path).unwrap();
        assert!(
            !loaded.is_active("JIG-99"),
            "dead pid should be reconciled away on load"
        );
    }

    #[test]
    fn reconcile_keeps_live_pids() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("triages.json");

        let mut tracker = TriageTracker {
            active: HashMap::new(),
            state_path: path.clone(),
        };
        // Use our own pid — guaranteed to be alive.
        tracker.active.insert(
            "JIG-1".to_string(),
            make_entry("JIG-1", "repo", 100, std::process::id()),
        );
        tracker.save().unwrap();

        let loaded = TriageTracker::load_from(&path).unwrap();
        assert!(
            loaded.is_active("JIG-1"),
            "live pid should survive reconcile on load"
        );
    }

    #[test]
    fn register_persists_to_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("triages.json");
        {
            let mut tracker = TriageTracker {
                active: HashMap::new(),
                state_path: path.clone(),
            };
            tracker.register(
                "JIG-10".to_string(),
                make_entry("JIG-10", "repo", 100, std::process::id()),
            );
        }

        assert!(path.exists(), "register should persist state file");

        let loaded = TriageTracker::load_from(&path).unwrap();
        assert!(loaded.is_active("JIG-10"));
    }

    #[test]
    fn remove_persists_to_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("triages.json");

        let mut tracker = TriageTracker {
            active: HashMap::new(),
            state_path: path.clone(),
        };
        tracker.register(
            "JIG-10".to_string(),
            make_entry("JIG-10", "repo", 100, std::process::id()),
        );
        tracker.remove("JIG-10");

        let loaded = TriageTracker::load_from(&path).unwrap();
        assert!(
            !loaded.is_active("JIG-10"),
            "remove should persist so next load sees the entry gone"
        );
    }

    #[test]
    fn save_is_atomic_no_tmp_leftover() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("triages.json");

        let mut tracker = TriageTracker {
            active: HashMap::new(),
            state_path: path.clone(),
        };
        tracker.active.insert(
            "JIG-1".to_string(),
            make_entry("JIG-1", "repo", 0, std::process::id()),
        );
        tracker.save().unwrap();

        assert!(path.exists());
        assert!(
            !tmp.path().join("triages.json.tmp").exists(),
            "atomic save should rename the tmp file away"
        );
    }

    #[test]
    fn load_from_missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let tracker = TriageTracker::load_from(&path).unwrap();
        assert!(!tracker.is_active("anything"));
    }
}
