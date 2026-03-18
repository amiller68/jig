//! Spawn operations for worker management
//!
//! High-level operations for spawning and managing workers.

use std::path::Path;

use crate::adapter;
use crate::config::{JigToml, RepoConfig};
use crate::context::RepoContext;
use crate::error::{Error, Result};
use crate::events::{Event, EventLog, EventType, WorkerState};
use crate::git::Repo;
use crate::global::GlobalConfig;
use crate::session;
use crate::state::OrchestratorState;
use crate::templates::{TemplateContext, TemplateEngine};
use crate::worker::{TaskContext, Worker, WorkerStatus};

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
    pub issue_ref: Option<String>,
}

/// Register a new spawn (creates worker state)
pub fn register(
    repo: &RepoContext,
    name: &str,
    branch: &str,
    context: Option<&str>,
    issue_ref: Option<&str>,
) -> Result<()> {
    let worktree_path = repo.worktrees_dir.join(name);

    let config = RepoConfig {
        base_branch: repo.base_branch.clone(),
        ..Default::default()
    };

    let mut state = OrchestratorState::load_or_create(repo.repo_root.clone(), config)?;

    let mut worker = Worker::new(
        name.to_string(),
        worktree_path,
        branch.to_string(),
        repo.base_branch.clone(),
        repo.session_name.clone(),
    );

    // Set task context if provided
    if let Some(ctx) = context {
        let mut task = TaskContext::new(ctx.to_string());
        if let Some(issue) = issue_ref {
            task = task.with_issue(issue.to_string());
        }
        worker.set_task(task);
    } else if let Some(issue) = issue_ref {
        worker.set_task(TaskContext::new(String::new()).with_issue(issue.to_string()));
    }

    worker.tmux_window = Some(name.to_string());
    state.add_worker(worker);
    state.save()?;

    // Emit Spawn event so daemon and ps --watch can discover this worker.
    // Reset the log first — if a previous worker had this name, start fresh.
    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    if let Ok(event_log) = EventLog::for_worker(&repo_name, name) {
        let _ = event_log.reset();
        let mut event = Event::new(EventType::Spawn)
            .with_field("branch", branch)
            .with_field("repo", repo_name.as_str());
        if let Some(issue) = issue_ref {
            event = event.with_field("issue", issue);
        }
        let _ = event_log.append(&event);
    }

    Ok(())
}

/// Launch a tmux window for a worker (always uses auto mode)
pub fn launch_tmux_window(
    repo: &RepoContext,
    name: &str,
    worktree_path: &Path,
    context: Option<&str>,
) -> Result<()> {
    // Always render preamble
    let engine = TemplateEngine::new().with_repo(&repo.repo_root);
    let global_config = GlobalConfig::load()?;
    let mut tpl_ctx = TemplateContext::new();
    tpl_ctx.set_num("max_nudges", global_config.health.max_nudges);
    tpl_ctx.set(
        "task_context",
        context.unwrap_or(
            "No specific task provided. Check CLAUDE.md and the issue tracker for context.",
        ),
    );
    let effective_context = engine.render("spawn-preamble", &tpl_ctx)?;

    // Get adapter from config (fallback to claude-code if not configured)
    let config = JigToml::load(&repo.repo_root)?.unwrap_or_default();
    let agent_adapter =
        adapter::get_adapter(&config.agent.agent_type).unwrap_or(&adapter::CLAUDE_CODE);

    // Create window in tmux
    session::create_window(&repo.session_name, name, worktree_path)?;

    // Build spawn command using adapter (always auto)
    let cmd = adapter::build_spawn_command(agent_adapter, Some(&effective_context));

    // Send command to window
    session::send_keys(&repo.session_name, name, &cmd)?;

    Ok(())
}

/// List all tasks (workers) with their status
pub fn list_tasks(repo: &RepoContext) -> Result<Vec<TaskInfo>> {
    // Clean up stale workers and get the (already loaded) state
    let state = cleanup_stale_workers(&repo.repo_root, &repo.session_name)?;

    let mut tasks = Vec::new();

    // If we have state, use workers from state
    if let Some(state) = state {
        for worker in state.workers.values() {
            let status = get_worker_status(&repo.session_name, &worker.name);
            let worktree_path = repo.worktrees_dir.join(&worker.name);

            let (commits_ahead, is_dirty) = if worktree_path.exists() {
                let commits = Repo::commits_ahead(&worktree_path, &repo.base_branch)
                    .unwrap_or_default()
                    .len();
                let dirty = Repo::has_uncommitted_changes(&worktree_path).unwrap_or(false);
                (commits, dirty)
            } else {
                (0, false)
            };

            let issue_ref = worker.task.as_ref().and_then(|t| t.issue_ref.clone());

            tasks.push(TaskInfo {
                name: worker.name.clone(),
                status,
                branch: worker.branch.clone(),
                commits_ahead,
                is_dirty,
                issue_ref,
            });
        }
    } else {
        // Fall back to checking tmux windows directly
        let windows = session::list_windows(&repo.session_name)?;

        for window_name in windows {
            let worktree_path = repo.worktrees_dir.join(&window_name);
            if !worktree_path.exists() {
                continue;
            }

            let status = get_worker_status(&repo.session_name, &window_name);
            let branch = Repo::worktree_branch(&worktree_path).unwrap_or_default();

            let commits_ahead = Repo::commits_ahead(&worktree_path, &repo.base_branch)
                .unwrap_or_default()
                .len();
            let is_dirty = Repo::has_uncommitted_changes(&worktree_path).unwrap_or(false);

            tasks.push(TaskInfo {
                name: window_name,
                status,
                branch,
                commits_ahead,
                is_dirty,
                issue_ref: None,
            });
        }
    }

    tasks.sort_by(|a, b| a.name.cmp(&b.name));

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
pub fn attach(repo: &RepoContext, name: Option<&str>) -> Result<()> {
    if let Some(window) = name {
        if !session::window_exists(&repo.session_name, window) {
            // Check if the worker is initializing or failed
            let worktree_path = repo.worktrees_dir.join(window);
            if worktree_path.exists() {
                if let Some(status) = get_worker_event_status(repo, window) {
                    match status {
                        WorkerStatus::Initializing => {
                            return Err(Error::WorkerInitializing(window.to_string()));
                        }
                        WorkerStatus::Failed => {
                            return Err(Error::WorkerSetupFailed(
                                window.to_string(),
                                get_worker_fail_reason(repo, window)
                                    .unwrap_or_else(|| "unknown".to_string()),
                            ));
                        }
                        _ => {}
                    }
                }
            }
            return Err(Error::WorkerNotFound(window.to_string()));
        }
        // Attach directly to session:window — doesn't change other clients' active window
        session::attach_window(&repo.session_name, window)
    } else {
        session::attach(&repo.session_name)
    }
}

/// Derive worker status from event log.
fn get_worker_event_status(repo: &RepoContext, name: &str) -> Option<WorkerStatus> {
    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let event_log = EventLog::for_worker(&repo_name, name).ok()?;
    let events = event_log.read_all().ok()?;
    if events.is_empty() {
        return None;
    }
    let config = GlobalConfig::load().ok()?.health;
    Some(WorkerState::reduce(&events, &config).status)
}

/// Get the failure reason from a worker's event log.
fn get_worker_fail_reason(repo: &RepoContext, name: &str) -> Option<String> {
    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let event_log = EventLog::for_worker(&repo_name, name).ok()?;
    let events = event_log.read_all().ok()?;
    events.iter().rev().find_map(|e| {
        e.data
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    })
}

/// Kill a worker's tmux window (without updating state)
pub fn kill_window(repo: &RepoContext, name: &str) -> Result<()> {
    session::kill_window(&repo.session_name, name)?;
    Ok(())
}

/// Unregister a worker from state (removes entirely) and clean up event log.
pub fn unregister(repo: &RepoContext, name: &str) -> Result<()> {
    if let Some(mut state) = OrchestratorState::load(&repo.repo_root)? {
        let id = state.get_worker_by_name(name).map(|w| w.id);
        if let Some(id) = id {
            state.remove_worker(&id);
            state.save()?;
        }
    }

    // Clean up event log
    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    if let Ok(event_log) = EventLog::for_worker(&repo_name, name) {
        let _ = event_log.remove();
    }

    Ok(())
}

/// Remove stale workers (whose tmux windows no longer exist) from state.
/// Preserves workers that are Initializing or Failed (they have no tmux window by design).
/// Returns the cleaned state if one existed.
fn cleanup_stale_workers(
    repo_root: &std::path::Path,
    session_name: &str,
) -> Result<Option<OrchestratorState>> {
    let mut state = match OrchestratorState::load(repo_root)? {
        Some(s) => s,
        None => return Ok(None),
    };

    let repo_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let health_config = GlobalConfig::load().map(|g| g.health).unwrap_or_default();

    let stale_ids: Vec<_> = state
        .workers
        .values()
        .filter(|w| {
            let tmux_status = get_worker_status(session_name, &w.name);
            if !matches!(tmux_status, TaskStatus::NoWindow | TaskStatus::NoSession) {
                return false;
            }
            // Don't clean up workers that are initializing or failed —
            // they intentionally have no tmux window
            if let Ok(event_log) = EventLog::for_worker(&repo_name, &w.name) {
                if let Ok(events) = event_log.read_all() {
                    if !events.is_empty() {
                        let ws = WorkerState::reduce(&events, &health_config);
                        if matches!(ws.status, WorkerStatus::Initializing | WorkerStatus::Failed) {
                            return false;
                        }
                    }
                }
            }
            true
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
