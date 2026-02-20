//! Persistent orchestrator state
//!
//! State that survives TUI/CLI restarts.

use crate::config::{self, RepoConfig};
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
        repo_root
            .join(config::JIG_DIR)
            .join(config::STATE_DIR)
            .join(config::STATE_FILE)
    }

    /// Get the legacy state file path (for migration from pre-0.5 layout).
    /// Before 0.5, state lived at `<repo>/.worktrees/.jig-state.json`.
    /// These values are frozen historical paths and must not change.
    fn legacy_state_file_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".worktrees").join(".jig-state.json")
    }

    /// Migrate from .worktrees/ to .jig/ layout
    fn migrate_if_needed(repo_root: &Path) -> Result<()> {
        let legacy_state = Self::legacy_state_file_path(repo_root);
        let new_state = Self::state_file_path(repo_root);

        // Only migrate if legacy exists and new doesn't
        if !legacy_state.exists() || new_state.exists() {
            return Ok(());
        }

        // Pre-0.5 worktree directory (frozen historical path)
        let old_dir = repo_root.join(".worktrees");
        let new_dir = repo_root.join(config::JIG_DIR);

        // Create new .jig/.state/ directory
        if let Some(parent) = new_state.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Move state file
        std::fs::copy(&legacy_state, &new_state)?;
        std::fs::remove_file(&legacy_state)?;

        // Move worktree directories (everything in .worktrees/ except hidden files)
        if old_dir.exists() {
            for entry in std::fs::read_dir(&old_dir)? {
                let entry = entry?;
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Skip hidden files (like .jig-state.json which we already moved)
                if name_str.starts_with('.') {
                    continue;
                }
                let src = entry.path();
                let dst = new_dir.join(&name);
                if src.is_dir() && !dst.exists() {
                    std::fs::rename(&src, &dst)?;
                }
            }

            // Remove old directory if empty
            if std::fs::read_dir(&old_dir)?.next().is_none() {
                let _ = std::fs::remove_dir(&old_dir);
            }
        }

        // Update worktree_path in the migrated state
        let content = std::fs::read_to_string(&new_state)?;
        if let Ok(mut state) = serde_json::from_str::<OrchestratorState>(&content) {
            for worker in state.workers.values_mut() {
                let old_prefix = old_dir.to_string_lossy().to_string();
                let new_prefix = new_dir.to_string_lossy().to_string();
                let path_str = worker.worktree_path.to_string_lossy().to_string();
                if path_str.starts_with(&old_prefix) {
                    worker.worktree_path =
                        PathBuf::from(path_str.replacen(&old_prefix, &new_prefix, 1));
                }
            }
            let content = serde_json::to_string_pretty(&state)?;
            std::fs::write(&new_state, content)?;
        }

        Ok(())
    }

    /// Load state from disk
    pub fn load(repo_root: &Path) -> Result<Option<Self>> {
        // Migrate from legacy layout if needed
        Self::migrate_if_needed(repo_root)?;

        let state_file = Self::state_file_path(repo_root);

        if !state_file.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&state_file)?;
        let state: Self = serde_json::from_str(&content)?;

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

    /// Get a worker by name
    pub fn get_worker_by_name(&self, name: &str) -> Option<&Worker> {
        self.workers.values().find(|w| w.name == name)
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
    fn test_migrate_from_legacy_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path();

        // Set up pre-0.5 legacy layout
        let old_dir = repo_root.join(".worktrees");
        std::fs::create_dir_all(&old_dir).unwrap();

        let mut state = OrchestratorState::new(repo_root.to_path_buf(), RepoConfig::default());
        let worker = Worker::new(
            "test-worker".to_string(),
            old_dir.join("test-worker"),
            "test-worker".to_string(),
            "main".to_string(),
            "jig-project".to_string(),
        );
        state.add_worker(worker);

        // Write state to legacy location
        let legacy_path = OrchestratorState::legacy_state_file_path(repo_root);
        let content = serde_json::to_string_pretty(&state).unwrap();
        std::fs::write(&legacy_path, &content).unwrap();

        // Create a fake worktree directory in old location
        std::fs::create_dir_all(old_dir.join("test-worker")).unwrap();

        // Now load (should trigger migration)
        let loaded = OrchestratorState::load(repo_root).unwrap().unwrap();

        // Verify new state file exists
        let new_state_path = OrchestratorState::state_file_path(repo_root);
        assert!(new_state_path.exists(), "new state file should exist");

        // Verify legacy state file is gone
        assert!(!legacy_path.exists(), "legacy state file should be removed");

        // Verify worktree directory moved
        let new_dir = repo_root.join(config::JIG_DIR);
        assert!(
            new_dir.join("test-worker").exists(),
            "worktree should be in new dir"
        );
        assert!(
            !old_dir.join("test-worker").exists(),
            "worktree should not be in legacy dir"
        );

        // Verify worker path was updated in state
        let worker = loaded.get_worker_by_name("test-worker").unwrap();
        let expected_fragment = format!("{}/test-worker", config::JIG_DIR);
        assert!(
            worker
                .worktree_path
                .to_string_lossy()
                .contains(&expected_fragment),
            "worker path should reference new dir, got: {}",
            worker.worktree_path.display()
        );
    }

    #[test]
    fn test_add_worker() {
        let mut state =
            OrchestratorState::new(PathBuf::from("/home/user/project"), RepoConfig::default());

        let worktree_path = PathBuf::from("/home/user/project")
            .join(config::JIG_DIR)
            .join("test-worker");
        let worker = Worker::new(
            "test-worker".to_string(),
            worktree_path,
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
