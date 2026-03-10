//! Spawn operations for worker management
//!
//! High-level operations for spawning and managing workers.

use std::path::Path;

use crate::adapter;
use crate::config::{JigToml, RepoConfig};
use crate::context::RepoContext;
use crate::error::{Error, Result};
use crate::events::{Event, EventLog, EventType};
use crate::git::Repo;
use crate::global::GlobalConfig;
use crate::session;
use crate::state::OrchestratorState;
use crate::templates::{TemplateContext, TemplateEngine};
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
    pub issue_ref: Option<String>,
}

/// Register a new spawn (creates worker state)
pub fn register(
    repo: &RepoContext,
    name: &str,
    branch: &str,
    context: Option<&str>,
    issue_ref: Option<&str>,
    auto: bool,
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
            .with_field("repo", repo_name.as_str())
            .with_field("auto", auto);
        if let Some(issue) = issue_ref {
            event = event.with_field("issue", issue);
        }
        if let Some(ctx) = context {
            event = event.with_field("context", ctx);
        }
        let _ = event_log.append(&event);
    }

    Ok(())
}

/// Resume an existing worker by appending a Resume event and re-launching tmux.
///
/// Unlike `register()`, this preserves the existing event log history.
pub fn resume_worker(
    repo: &RepoContext,
    name: &str,
    auto: bool,
    context: Option<&str>,
) -> Result<()> {
    let worktree_path = repo.worktrees_dir.join(name);

    if !worktree_path.exists() {
        return Err(Error::WorktreeNotFound(name.to_string()));
    }

    // Check that tmux window does NOT already exist
    if session::window_exists(&repo.session_name, name) {
        return Err(Error::Custom(format!(
            "tmux window '{}' already exists — use `jig attach {}` instead",
            name, name
        )));
    }

    let repo_name = repo
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Read the branch from the worktree
    let branch =
        crate::git::Repo::worktree_branch(&worktree_path).unwrap_or_else(|_| name.to_string());

    // Append Resume event (preserving history)
    if let Ok(event_log) = EventLog::for_worker(&repo_name, name) {
        let mut event = Event::new(EventType::Resume)
            .with_field("branch", branch.as_str())
            .with_field("repo", repo_name.as_str());
        if let Some(ctx) = context {
            event = event.with_field("context", ctx);
        }
        let _ = event_log.append(&event);
    }

    // Launch in tmux
    launch_tmux_window(repo, name, &worktree_path, auto, context)?;

    Ok(())
}

/// Extract the original context from a worker's event log (from the last Spawn/Resume event).
pub fn extract_spawn_context(repo_name: &str, worker_name: &str) -> Option<String> {
    let event_log = EventLog::for_worker(repo_name, worker_name).ok()?;
    let events = event_log.read_all().ok()?;
    // Find the last Spawn or Resume event
    events
        .iter()
        .rev()
        .find(|e| e.event_type == EventType::Spawn || e.event_type == EventType::Resume)
        .and_then(|e| e.data.get("context").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

/// Check if the original spawn was in auto mode (from the Spawn event).
pub fn was_auto_spawn(repo_name: &str, worker_name: &str) -> bool {
    let event_log = match EventLog::for_worker(repo_name, worker_name) {
        Ok(log) => log,
        Err(_) => return false,
    };
    let events = match event_log.read_all() {
        Ok(e) => e,
        Err(_) => return false,
    };
    // Check if the original Spawn event had auto=true, or if there was ever a Resume
    // (daemon-initiated resumes are always auto)
    events.iter().any(|e| e.event_type == EventType::Resume)
        || events.iter().any(|e| {
            e.event_type == EventType::Spawn
                && e.data
                    .get("auto")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
        })
}

/// Launch a tmux window for a worker
pub fn launch_tmux_window(
    repo: &RepoContext,
    name: &str,
    worktree_path: &Path,
    auto: bool,
    context: Option<&str>,
) -> Result<()> {
    // Render preamble when auto=true
    let effective_context = if auto {
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
        Some(engine.render("spawn-preamble", &tpl_ctx)?)
    } else {
        context.map(|s| s.to_string())
    };

    // Get adapter from config (fallback to claude-code if not configured)
    let config = JigToml::load(&repo.repo_root)?.unwrap_or_default();
    let agent_adapter =
        adapter::get_adapter(&config.agent.agent_type).unwrap_or(&adapter::CLAUDE_CODE);

    // Create window in tmux
    session::create_window(&repo.session_name, name, worktree_path)?;

    // Build spawn command using adapter
    let cmd = adapter::build_spawn_command(agent_adapter, effective_context.as_deref(), auto);

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
            return Err(Error::WorkerNotFound(window.to_string()));
        }
        // Attach directly to session:window — doesn't change other clients' active window
        session::attach_window(&repo.session_name, window)
    } else {
        session::attach(&repo.session_name)
    }
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
