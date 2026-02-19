//! Spawn operations for worker management
//!
//! High-level operations for spawning and managing workers.

use std::path::Path;

use crate::adapter;
use crate::config::{get_base_branch, JigToml, RepoConfig};
use crate::error::{Error, Result};
use crate::git;
use crate::session;
use crate::state::OrchestratorState;
use crate::worker::{TaskContext, Worker};

/// Task status for ps command
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Running,
    Exited,
    NoSession,
    NoWindow,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Running => "running",
            TaskStatus::Exited => "exited",
            TaskStatus::NoSession => "no-session",
            TaskStatus::NoWindow => "no-window",
        }
    }
}

/// Task info for ps command
#[derive(Debug)]
pub struct TaskInfo {
    pub name: String,
    pub status: TaskStatus,
    pub branch: String,
    pub commits_ahead: usize,
    pub is_dirty: bool,
}

/// Get the tmux session name for this repo
pub fn get_session_name() -> Result<String> {
    let repo_root = git::get_base_repo()?;
    let name = repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    Ok(format!("jig-{}", name))
}

/// Register a new spawn (creates worker state)
pub fn register(name: &str, branch: &str, context: Option<&str>) -> Result<()> {
    let repo_root = git::get_base_repo()?;
    let worktrees_dir = git::get_worktrees_dir()?;
    let worktree_path = worktrees_dir.join(name);
    let session_name = get_session_name()?;
    let base_branch = get_base_branch()?;

    let config = RepoConfig {
        base_branch: base_branch.clone(),
        ..Default::default()
    };

    let mut state = OrchestratorState::load_or_create(repo_root, config)?;

    let mut worker = Worker::new(
        name.to_string(),
        worktree_path,
        branch.to_string(),
        base_branch,
        session_name.clone(),
    );

    // Set task context if provided
    if let Some(ctx) = context {
        worker.set_task(TaskContext::new(ctx.to_string()));
    }

    worker.tmux_window = Some(name.to_string());
    state.add_worker(worker);
    state.save()?;

    Ok(())
}

/// Launch a tmux window for a worker
pub fn launch_tmux_window(
    name: &str,
    worktree_path: &Path,
    auto: bool,
    context: Option<&str>,
) -> Result<()> {
    let session_name = get_session_name()?;
    let repo_root = git::get_base_repo()?;

    // Get adapter from config (fallback to claude-code if not configured)
    let config = JigToml::load(&repo_root)?.unwrap_or_default();
    let agent_adapter =
        adapter::get_adapter(&config.agent.agent_type).unwrap_or(&adapter::CLAUDE_CODE);

    // Create window in tmux
    session::create_window(&session_name, name, worktree_path)?;

    // Build spawn command using adapter
    let cmd = adapter::build_spawn_command(agent_adapter, context, auto);

    // Send command to window
    session::send_keys(&session_name, name, &cmd)?;

    Ok(())
}

/// List all tasks (workers) with their status
pub fn list_tasks() -> Result<Vec<TaskInfo>> {
    let repo_root = git::get_base_repo()?;
    let session_name = get_session_name()?;
    let base_branch = get_base_branch()?;
    let worktrees_dir = git::get_worktrees_dir()?;

    // Clean up stale workers and get the (already loaded) state
    let state = cleanup_stale_workers(&repo_root, &session_name)?;

    let mut tasks = Vec::new();

    // If we have state, use workers from state
    if let Some(state) = state {
        for worker in state.workers.values() {
            let status = get_worker_status(&session_name, &worker.name);
            let worktree_path = worktrees_dir.join(&worker.name);

            let (commits_ahead, is_dirty) = if worktree_path.exists() {
                let commits = git::get_commits_ahead(&worktree_path, &base_branch)
                    .unwrap_or_default()
                    .len();
                let dirty = git::has_uncommitted_changes(&worktree_path).unwrap_or(false);
                (commits, dirty)
            } else {
                (0, false)
            };

            tasks.push(TaskInfo {
                name: worker.name.clone(),
                status,
                branch: worker.branch.clone(),
                commits_ahead,
                is_dirty,
            });
        }
    } else {
        // Fall back to checking tmux windows directly
        let windows = session::list_windows(&session_name)?;

        for window_name in windows {
            let worktree_path = worktrees_dir.join(&window_name);
            if !worktree_path.exists() {
                continue;
            }

            let status = get_worker_status(&session_name, &window_name);
            let branch = git::get_worktree_branch(&worktree_path).unwrap_or_default();

            let commits_ahead = git::get_commits_ahead(&worktree_path, &base_branch)
                .unwrap_or_default()
                .len();
            let is_dirty = git::has_uncommitted_changes(&worktree_path).unwrap_or(false);

            tasks.push(TaskInfo {
                name: window_name,
                status,
                branch,
                commits_ahead,
                is_dirty,
            });
        }
    }

    Ok(tasks)
}

fn get_worker_status(session: &str, window: &str) -> TaskStatus {
    if !session::session_exists(session) {
        return TaskStatus::NoSession;
    }

    if !session::window_exists(session, window) {
        return TaskStatus::NoWindow;
    }

    if session::pane_is_running(session, window) {
        TaskStatus::Running
    } else {
        TaskStatus::Exited
    }
}

/// Attach to tmux session
pub fn attach(name: Option<&str>) -> Result<()> {
    let session_name = get_session_name()?;

    // If a specific window is requested, select it first
    if let Some(window) = name {
        if !session::window_exists(&session_name, window) {
            return Err(Error::WorkerNotFound(window.to_string()));
        }
        session::select_window(&session_name, window)?;
    }

    // Attach to session
    session::attach(&session_name)
}

/// Kill a worker's tmux window
pub fn kill(name: &str) -> Result<()> {
    let session_name = get_session_name()?;
    let repo_root = git::get_base_repo()?;

    // Kill tmux window
    session::kill_window(&session_name, name)?;

    // Remove worker from state entirely
    if let Some(mut state) = OrchestratorState::load(&repo_root)? {
        let id = state.get_worker_by_name(name).map(|w| w.id);
        if let Some(id) = id {
            state.remove_worker(&id);
            state.save()?;
        }
    }

    Ok(())
}

/// Kill a worker's tmux window (without updating state)
pub fn kill_window(name: &str) -> Result<()> {
    let session_name = get_session_name()?;
    session::kill_window(&session_name, name)?;
    Ok(())
}

/// Unregister a worker from state (removes entirely)
pub fn unregister(name: &str) -> Result<()> {
    let repo_root = git::get_base_repo()?;

    if let Some(mut state) = OrchestratorState::load(&repo_root)? {
        let id = state.get_worker_by_name(name).map(|w| w.id);
        if let Some(id) = id {
            state.remove_worker(&id);
            state.save()?;
        }
    }

    Ok(())
}

/// Remove stale workers (whose tmux windows no longer exist) from state.
/// Returns the cleaned state if one existed.
fn cleanup_stale_workers(
    repo_root: &std::path::Path,
    session_name: &str,
) -> Result<Option<OrchestratorState>> {
    let mut state = match OrchestratorState::load(repo_root)? {
        Some(s) => s,
        None => return Ok(None),
    };

    let stale_ids: Vec<_> = state
        .workers
        .values()
        .filter(|w| {
            let status = get_worker_status(session_name, &w.name);
            matches!(status, TaskStatus::NoWindow | TaskStatus::NoSession)
        })
        .map(|w| w.id)
        .collect();

    if !stale_ids.is_empty() {
        for id in &stale_ids {
            state.remove_worker(id);
        }
        state.save()?;
    }

    Ok(Some(state))
}
