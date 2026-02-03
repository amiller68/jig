//! Persistent orchestrator state
//!
//! State that survives TUI/CLI restarts.

use crate::config::RepoConfig;
use crate::error::Result;
use crate::worker::{Worker, WorkerId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Current state file version for migrations
const STATE_VERSION: u32 = 1;

/// Persistent orchestrator state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorState {
    /// Version for state migrations
    pub version: u32,
    /// Root of the git repository
    pub repo_root: PathBuf,
    /// All workers (active and archived)
    pub workers: HashMap<WorkerId, Worker>,
    /// Shared tmux session for all workers
    pub tmux_session: String,
    /// Repository configuration
    pub config: RepoConfig,
}

impl OrchestratorState {
    /// Create a new orchestrator state
    pub fn new(repo_root: PathBuf, config: RepoConfig) -> Self {
        let tmux_session = format!(
            "jig-{}",
            repo_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
        );

        Self {
            version: STATE_VERSION,
            repo_root,
            workers: HashMap::new(),
            tmux_session,
            config,
        }
    }

    /// Get the state file path for a repository
    pub fn state_file_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".worktrees").join(".jig-state.json")
    }

    /// Load state from disk
    pub fn load(repo_root: &Path) -> Result<Option<Self>> {
        let state_file = Self::state_file_path(repo_root);

        if !state_file.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&state_file)?;
        let state: Self = serde_json::from_str(&content)?;

        // TODO: Handle migrations if version differs

        Ok(Some(state))
    }

    /// Save state to disk
    pub fn save(&self) -> Result<()> {
        let state_file = Self::state_file_path(&self.repo_root);

        // Ensure directory exists
        if let Some(parent) = state_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&state_file, content)?;

        Ok(())
    }

    /// Load or create state for a repository
    pub fn load_or_create(repo_root: PathBuf, config: RepoConfig) -> Result<Self> {
        match Self::load(&repo_root)? {
            Some(state) => Ok(state),
            None => Ok(Self::new(repo_root, config)),
        }
    }

    /// Add a worker
    pub fn add_worker(&mut self, worker: Worker) {
        self.workers.insert(worker.id, worker);
    }

    /// Get a worker by ID
    pub fn get_worker(&self, id: &WorkerId) -> Option<&Worker> {
        self.workers.get(id)
    }

    /// Get a mutable worker by ID
    pub fn get_worker_mut(&mut self, id: &WorkerId) -> Option<&mut Worker> {
        self.workers.get_mut(id)
    }

    /// Get a worker by name
    pub fn get_worker_by_name(&self, name: &str) -> Option<&Worker> {
        self.workers.values().find(|w| w.name == name)
    }

    /// Get a mutable worker by name
    pub fn get_worker_by_name_mut(&mut self, name: &str) -> Option<&mut Worker> {
        self.workers.values_mut().find(|w| w.name == name)
    }

    /// Remove a worker
    pub fn remove_worker(&mut self, id: &WorkerId) -> Option<Worker> {
        self.workers.remove(id)
    }

    /// Get all active (non-terminal) workers
    pub fn active_workers(&self) -> impl Iterator<Item = &Worker> {
        self.workers.values().filter(|w| w.is_active())
    }

    /// Get all workers
    pub fn all_workers(&self) -> impl Iterator<Item = &Worker> {
        self.workers.values()
    }

    /// Count of active workers
    pub fn active_count(&self) -> usize {
        self.active_workers().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_new() {
        let state =
            OrchestratorState::new(PathBuf::from("/home/user/project"), RepoConfig::default());

        assert_eq!(state.version, STATE_VERSION);
        assert_eq!(state.tmux_session, "jig-project");
        assert!(state.workers.is_empty());
    }

    #[test]
    fn test_add_worker() {
        let mut state =
            OrchestratorState::new(PathBuf::from("/home/user/project"), RepoConfig::default());

        let worker = Worker::new(
            "test-worker".to_string(),
            PathBuf::from("/home/user/project/.worktrees/test-worker"),
            "test-worker".to_string(),
            "main".to_string(),
            "jig-project".to_string(),
        );

        let id = worker.id;
        state.add_worker(worker);

        assert!(state.get_worker(&id).is_some());
        assert!(state.get_worker_by_name("test-worker").is_some());
    }
}
